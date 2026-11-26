//! The Boundary group's ui-side half (M11.5): the page draft carries the
//! area controls, Apply commits them with the parameters as ONE undoable
//! edit, an untouched group queues the plain parameter edit it always
//! did, and a viewport pick made while the group is collecting fills the
//! draft - never the selection, never the world until Apply. The command
//! law itself is pinned in `crates/app/tests/area_open.rs`.
//! Provenance: our own tests, specced from the page's contract.
//!
//! Gate map (M11.5): covers ui::state (OpUi, CommandKind::EditOperationBoundary,
//! BoundaryPick), ui::interact (queue_feature_pick), ui::egui_app::boundary,
//! app::commands (EditOperationParams).

use camcraft_app::model;
use camcraft_app::operation::{
    AreaControls, BoundaryMode, BoundaryShape, Containment, FeatureSel, GeoSel, Height, OpParams, SrcRef,
};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_ui::interact::queue_feature_pick;
use camcraft_ui::state::{BoundaryPick, CommandKind, OpUi, State};

fn face_params() -> OpParams {
    OpParams::Face { holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

fn face_world() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    state.pending.push_back(CommandKind::AddTool { name: "EM-0400-2FL".into(), tool });
    state.pending.push_back(CommandKind::CreateOperation {
        name: "Face1".into(),
        tool: "EM-0400-2FL".into(),
        src: SrcRef::Stock,
        params: face_params(),
    });
    state.drain_actions();
    let e = model::find_by_name(&state.world, "Face1").expect("op");
    let op = model::operation_of(&state.world, &e).expect("op");
    state.op_ui = OpUi::from_op("Face1", &op);
    state
}

fn live_area(state: &State) -> AreaControls {
    let e = model::find_by_name(&state.world, "Face1").expect("op");
    model::operation_of(&state.world, &e).expect("op").area
}

#[test]
fn apply_commits_the_boundary_draft_as_one_undoable_edit() {
    let mut state = face_world();
    state.op_ui.area = AreaControls { containment: Some(Containment::On), extension: 4.0, ..Default::default() };
    state.apply_operation_params("Face1");
    assert!(
        matches!(state.pending.back(), Some(CommandKind::EditOperationBoundary { .. })),
        "a moved Boundary group rides the edit that carries it"
    );
    state.drain_actions();
    assert_eq!(live_area(&state), state.op_ui.area);
    assert!(state.status.contains("boundary updated"), "{}", state.status);
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(live_area(&state), AreaControls::default(), "one undo restores the identity");
}

#[test]
fn an_untouched_boundary_queues_the_plain_parameter_edit() {
    let mut state = face_world();
    if let OpParams::Face { stepover, .. } = &mut state.op_ui.params {
        *stepover = 2.0;
    }
    state.apply_operation_params("Face1");
    assert!(matches!(state.pending.back(), Some(CommandKind::EditOperationParams { .. })));
}

#[test]
fn a_pick_while_collecting_fills_the_draft_not_the_selection() {
    let mut state = face_world();
    state.boundary_pick = Some(BoundaryPick::Exclude);
    let pick = GeoSel { entity: "Stock".into(), feature: FeatureSel::Face(7) };
    queue_feature_pick(&mut state, pick.clone(), false);
    assert!(state.pending.is_empty(), "nothing queued - the draft holds it until Apply");
    assert_eq!(state.op_ui.area.extra.len(), 1);
    let b = &state.op_ui.area.extra[0];
    assert_eq!(b.mode, BoundaryMode::Exclude);
    assert_eq!(b.shape, BoundaryShape::Picks(vec![pick.clone()]));
    // Ctrl adds to the same boundary; a plain click starts a new one.
    let other = GeoSel { entity: "Stock".into(), feature: FeatureSel::Face(9) };
    queue_feature_pick(&mut state, other.clone(), true);
    assert_eq!(state.op_ui.area.extra[0].shape, BoundaryShape::Picks(vec![pick, other]));
    state.boundary_pick = Some(BoundaryPick::Open);
    let edge = GeoSel { entity: "Stock".into(), feature: FeatureSel::Chain(3) };
    queue_feature_pick(&mut state, edge.clone(), false);
    assert_eq!(state.op_ui.area.open.as_ref().map(|o| o.picks.clone()), Some(vec![edge]));
    assert_eq!(live_area(&state), AreaControls::default(), "the world is untouched until Apply");
}
