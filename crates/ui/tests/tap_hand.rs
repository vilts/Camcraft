//! #1453: a tap's hand survives the tool draft and undoable editing.
//! Provenance: our own catalog fixture and public UI command contract.
//! Gate map (#1453): covers ui::state, ui::egui_app::pages.

use camcraft_app::{model, tools::{catalog, ThreadHand, Tool}};
use camcraft_ui::state::{CommandKind, State, ToolUi};

fn tap() -> Tool {
    catalog().into_iter().find(|(name, _)| *name == "TAP-M6").unwrap().1
}

fn push(state: &mut State, command: CommandKind) {
    state.pending.push_back(command);
    state.drain_actions();
}

fn stored(state: &State) -> Tool {
    let entity = model::find_by_name(&state.world, "Handed tap").unwrap();
    model::tool_of(&state.world, &entity).unwrap()
}

#[test]
fn tap_tool_draft_preserves_both_hands() {
    for hand in [ThreadHand::Right, ThreadHand::Left] {
        let mut tool = tap();
        tool.thread_hand = hand;
        let draft = ToolUi::from_tool("Handed tap", &tool, true);
        assert_eq!(draft.thread_hand, hand);
        assert_eq!(draft.to_tool(), tool);
    }
}

#[test]
fn left_hand_tap_add_is_undoable_and_redoable() {
    let mut state = State::new();
    let mut tool = tap();
    tool.thread_hand = ThreadHand::Left;
    let draft = ToolUi::from_tool("Handed tap", &tool, false);
    push(&mut state, CommandKind::AddTool {
        name: draft.name.clone(), tool: draft.to_tool(),
    });
    assert_eq!(stored(&state), tool);
    push(&mut state, CommandKind::Undo);
    assert!(model::find_by_name(&state.world, "Handed tap").is_none());
    push(&mut state, CommandKind::Redo);
    assert_eq!(stored(&state), tool);
}

#[test]
fn changing_tap_hand_roundtrips_through_undo_and_redo() {
    let mut state = State::new();
    let original = tap();
    push(&mut state, CommandKind::AddTool {
        name: "Handed tap".into(), tool: original.clone(),
    });
    let mut draft = ToolUi::from_tool("Handed tap", &original, true);
    draft.thread_hand = ThreadHand::Left;
    let edited = draft.to_tool();
    push(&mut state, CommandKind::ChangeTool {
        name: draft.name.clone(), tool: edited.clone(),
    });
    assert_eq!(stored(&state), edited);
    push(&mut state, CommandKind::Undo);
    assert_eq!(stored(&state), original);
    push(&mut state, CommandKind::Redo);
    assert_eq!(stored(&state), edited);
}
