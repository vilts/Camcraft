//! #221 in the surface: the `SetToolNumber` command kind rides the history
//! (undo / redo walk it, a refused number pushes nothing), and the tool
//! editor's draft shows a tool's own number when it syncs to that tool.
//! Provenance: our own tests over our own command plumbing.

use camcraft_app::model;
use camcraft_app::tools::catalog;

use crate::state::{CommandKind, State};

const TOOL: &str = "EM-0400-2FL";

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn with_tool() -> State {
    let mut state = State::new();
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == TOOL)
        .map(|(_, t)| t)
        .expect("catalog tool");
    run(
        &mut state,
        CommandKind::AddTool {
            name: TOOL.to_owned(),
            tool,
        },
    );
    state
}

fn number(state: &State) -> Option<u32> {
    model::find_by_name(&state.world, TOOL).and_then(|e| model::tool_number_of(&state.world, &e))
}

#[test]
fn setting_a_tool_number_is_a_command_with_undo() {
    let mut state = with_tool();
    run(
        &mut state,
        CommandKind::SetToolNumber {
            name: TOOL.to_owned(),
            number: Some(31),
        },
    );
    assert_eq!(number(&state), Some(31));
    assert!(state.status.contains("T31"), "{}", state.status);
    run(&mut state, CommandKind::Undo);
    assert_eq!(number(&state), Some(1), "undo restores the number assigned at add");
    run(&mut state, CommandKind::Redo);
    assert_eq!(number(&state), Some(31), "redo restores it");
    // A refused number (0 names no tool) changes nothing.
    run(
        &mut state,
        CommandKind::SetToolNumber {
            name: TOOL.to_owned(),
            number: Some(0),
        },
    );
    assert_eq!(number(&state), Some(31));
    assert!(
        state.status.contains("tool number not set"),
        "{}",
        state.status
    );
}

#[test]
fn the_editor_draft_shows_the_tools_own_number() {
    let mut state = with_tool();
    state.tool_ui.name = TOOL.to_owned();
    state.sync_tool_ui();
    assert_eq!(
        state.tool_ui.number, 1,
        "a freshly added tool starts with its assigned T number"
    );
    run(
        &mut state,
        CommandKind::SetToolNumber {
            name: TOOL.to_owned(),
            number: Some(31),
        },
    );
    state.sync_tool_ui();
    assert_eq!(
        state.tool_ui.number, 31,
        "the draft mirrors the tool's number"
    );
    // Clearing to None (#2442) is refused and leaves the tool's number intact.
    run(
        &mut state,
        CommandKind::SetToolNumber {
            name: TOOL.to_owned(),
            number: None,
        },
    );
    assert_eq!(number(&state), Some(31));
    assert!(
        state.status.contains("tool number not set"),
        "{}",
        state.status
    );
    state.sync_tool_ui();
    assert_eq!(state.tool_ui.number, 31);
}
