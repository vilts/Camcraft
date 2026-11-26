//! Provenance: issue #1453, our tool editor's staged-edit and Apply contract.

use egui::{Vec2, accesskit::Role};
use egui_kittest::{Harness, kittest::{NodeT, Queryable}};
use camcraft_app::{model, tools::{catalog, ThreadHand}};
use crate::state::{CommandKind, State};

const LABEL: &str = "Left-hand tap (posting unavailable)";

fn editor(name: &str) -> Harness<'static, State> {
    let mut state = State::new();
    let tool = catalog().into_iter().find(|(n, _)| *n == name).unwrap().1;
    state.pending.push_back(CommandKind::AddTool { name: name.into(), tool });
    state.drain_actions();
    // #3930: the tool settings are edited in the tool library window;
    // Properties shows a summary.
    state.open_tool_in_library(name);
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1600.0, 1200.0))
        .build_ui_state(
            |ui, state: &mut State| super::super::tool_library_dialog::tool_library_window(ui.ctx(), state),
            state,
        );
    harness.run_steps(2);
    harness
}

fn hand(state: &State) -> ThreadHand {
    let entity = model::find_by_name(&state.world, "TAP-M6").unwrap();
    model::tool_of(&state.world, &entity).unwrap().thread_hand
}

#[test]
fn tap_hand_checkbox_stages_until_apply_and_undo_restores_right() {
    let mut harness = editor("TAP-M6");
    let row = harness.get_by_label(LABEL).rect();
    let checkbox = harness.query_all_by_role(Role::CheckBox)
        .find(|node| (node.rect().center().y - row.center().y).abs() < 4.0)
        .expect("the tap-hand row has a checkbox");
    assert_eq!(checkbox.accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
    checkbox.click();
    harness.run();
    let staged = harness.state().tool_library.as_ref().and_then(|v| v.editor.as_ref()).map(|e| e.draft.thread_hand);
    assert_eq!(staged, Some(ThreadHand::Left));
    assert_eq!(hand(harness.state()), ThreadHand::Right, "checkbox only stages the edit");
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert_eq!(hand(harness.state()), ThreadHand::Left);
    harness.state_mut().pending.push_back(CommandKind::Undo);
    harness.state_mut().drain_actions();
    assert_eq!(hand(harness.state()), ThreadHand::Right);
}

#[test]
fn a_non_tap_tool_has_no_tap_hand_checkbox() {
    let harness = editor("EM-0600-4FL");
    assert!(harness.query_by_label(LABEL).is_none());
}
