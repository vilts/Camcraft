//! The operation page's refusal lifecycle, the Clear toolpath verb and
//! the Apply button's spoken reason, driven through `State`'s queue.
//! Provenance: our own tests, specced from the hand-test report (a
//! "generate failed" line that outlived the state it described; Delete
//! taken for "clear the generated path").
//!
//! Gate map (M4.4): covers ui::feedback, ui::state, app::commands, app::operation.

use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::STOCK_NAME;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_ui::state::{CommandKind, ErrorScope, State, UiTarget};

fn face_params() -> OpParams {
    OpParams::Face { holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn set_stock() -> CommandKind {
    CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    }
}

/// Stock, a tool, two face ops (Face1 on the page), Face1 generated.
fn two_faces() -> State {
    let mut state = State::new();
    state.pending.push_back(set_stock());
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog tool");
    state.pending.push_back(CommandKind::AddTool { name: "EM-0400-2FL".into(), tool });
    for name in ["Face1", "Face2"] {
        state.pending.push_back(CommandKind::CreateOperation {
            name: name.into(),
            tool: "EM-0400-2FL".into(),
            src: SrcRef::Stock,
            params: face_params(),
        });
    }
    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.pending.push_back(CommandKind::Select(Some("Face1".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("Face1".into());
    state.sync_ops_ui();
    assert_eq!(state.op_ui.name, "Face1");
    state
}

/// Remove the stock under Face1 and ask for a generate: refused.
fn failed_generate(state: &mut State) {
    state.pending.push_back(CommandKind::DeleteByName(STOCK_NAME.into()));
    state.drain_actions();
    state.sync_ops_ui();
    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    assert!(state.status.starts_with("generate failed"), "{:?}", state.status);
    assert!(state.error_for(ErrorScope::Operation).is_some(), "the page shows the refusal");
}

fn cache(state: &State, name: &str) -> Option<camcraft_app::schema::Toolpath> {
    model::find_by_name(&state.world, name).and_then(|e| model::toolpath_of(&state.world, &e))
}

fn status(state: &State, name: &str) -> OpStatus {
    let e = model::find_by_name(&state.world, name).expect("operation entity");
    operation::status_of(&state.world, &e)
}

#[test]
fn a_draft_edit_retires_the_refusal_and_its_status_mirror() {
    let mut state = two_faces();
    failed_generate(&mut state);
    // The user starts correcting: the sentence was about the old values.
    if let OpParams::Face { stepover, .. } = &mut state.op_ui.params {
        *stepover = 2.0;
    }
    assert_eq!(state.error_for(ErrorScope::Operation), None, "gone the moment the draft moves");
    state.drain_actions();
    assert!(!state.status.starts_with("generate failed"), "the strip retired too: {:?}", state.status);
}

#[test]
fn a_param_command_or_undo_retires_the_refusal() {
    let mut state = two_faces();
    failed_generate(&mut state);
    let mut params = face_params();
    if let OpParams::Face { stepover, .. } = &mut params {
        *stepover = 2.5;
    }
    state.pending.push_back(CommandKind::EditOperationParams { name: "Face1".into(), params });
    state.drain_actions();
    assert_eq!(state.error_for(ErrorScope::Operation), None);

    failed_generate_again(&mut state);
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(state.error_for(ErrorScope::Operation), None, "undo retires it");
}

fn failed_generate_again(state: &mut State) {
    state.sync_ops_ui();
    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    assert!(state.error_for(ErrorScope::Operation).is_some());
}

#[test]
fn a_successful_generate_retires_the_refusal() {
    let mut state = two_faces();
    failed_generate(&mut state);
    state.pending.push_back(set_stock());
    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    assert_eq!(state.error_for(ErrorScope::Operation), None);
    assert!(!state.status.starts_with("generate failed"), "{:?}", state.status);
    assert_eq!(status(&state, "Face1"), OpStatus::Ok);
}

#[test]
fn another_operation_on_the_page_retires_the_refusal() {
    let mut state = two_faces();
    failed_generate(&mut state);
    // Browsing to another op is view state, not a command - the refusal
    // still must not follow the user onto Face2's page.
    state.ui_target = UiTarget::Operation("Face2".into());
    assert_eq!(state.error_for(ErrorScope::Operation), None);
    state.drain_actions();
    assert_eq!(state.feedback, None, "retired, not just hidden");
    assert_eq!(state.status, "ready");
}

#[test]
fn clear_toolpath_keeps_the_op_and_undo_restores_the_path_exactly() {
    let mut state = two_faces();
    let before = cache(&state, "Face1").expect("generated");
    state.pending.push_back(CommandKind::ClearToolpath("Face1".into()));
    state.drain_actions();
    let hint = format!("{} restores", camcraft_ui::shortcut_keys::chord("Z"));
    assert!(state.status.contains(&hint), "{:?}", state.status);
    assert_eq!(cache(&state, "Face1"), None, "the path is gone");
    assert_eq!(status(&state, "Face1"), OpStatus::Never, "reads never generated");
    assert!(model::find_by_name(&state.world, "Face1").is_some(), "the operation is kept");

    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(cache(&state, "Face1"), Some(before), "byte-identical cache back");
    assert_eq!(status(&state, "Face1"), OpStatus::Ok);

    // Clearing an op that has nothing cached is a named refusal.
    state.pending.push_back(CommandKind::ClearToolpath("Face2".into()));
    state.drain_actions();
    assert!(state.status.starts_with("toolpath not cleared"), "{:?}", state.status);

    // Delete is still delete.
    state.pending.push_back(CommandKind::DeleteOperation("Face1".into()));
    state.drain_actions();
    assert!(model::find_by_name(&state.world, "Face1").is_none());
}

#[test]
fn apply_hint_says_why_apply_is_dark_and_which_field_is_bad() {
    let live = face_params();
    let why = camcraft_ui::feedback::apply_hint(Some(&live), &live, true).expect("a reason");
    assert!(why.contains("no parameter differs from the operation"), "{why}");

    let mut good = face_params();
    if let OpParams::Face { stepover, .. } = &mut good {
        *stepover = 2.0;
    }
    assert_eq!(camcraft_ui::feedback::apply_hint(Some(&live), &good, false), None);

    let mut bad = face_params();
    if let OpParams::Face { plunge_feed, .. } = &mut bad {
        *plunge_feed = -5.0;
    }
    let why = camcraft_ui::feedback::apply_hint(Some(&live), &bad, false).expect("refusal ahead");
    assert!(why.contains("plunge feed"), "names the field: {why}");
}
