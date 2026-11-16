//! The Thread operation page's pitch and hand rows (#894): the Passes
//! tab states the pitch on demand and names both hands; the picks edit
//! the page draft and Apply rides `EditOperationParams` - undo restores
//! the unstated pitch and the right hand. The Geometry tab (#985) says
//! whether the thread is external: holes become bosses, the detect
//! button reads the other kind, and Apply is one undo step.
//! Provenance: our own page law; no vendor material consulted.

use egui_kittest::kittest::Queryable;

use camcraft_app::cl::ThreadHand;
use camcraft_app::commands::{AddTool, CreateOperation};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use super::dialog_tests::page;
use crate::state::{CommandKind, State, UiTarget};

const MILL: &str = "TMS-0600-P050-150";

/// Box stock, the catalog's single-form thread mill and one Thread op
/// named "Op" with its pitch unstated, opened on the Passes tab.
fn opened() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let mill = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog thread mill");
    state.history.execute(&state.world, AddTool::new(&state.world, MILL, mill).expect("tool"));
    let params = OpParams::Thread {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        infeed: 0.2,
        feed: 300.0,
        plunge_feed: 100.0,
        climb: true,
        holes: Vec::new(),
        pitch: None,
        hand: ThreadHand::Right,
        external: false,
        passes: camcraft_app::machining2d::ThreadPasses::default(),
    };
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "Op", MILL, SrcRef::Stock, params).expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("Op".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("Op".into());
    state.sync_ops_ui();
    state.op_tab = super::op_tabs::SEC_PASSES;
    state
}

fn thread(state: &State) -> (Option<f64>, ThreadHand) {
    match model::find_by_name(&state.world, "Op").and_then(|e| model::operation_of(&state.world, &e)).map(|o| o.params) {
        Some(OpParams::Thread { pitch, hand, .. }) => (pitch, hand),
        other => panic!("a thread op expected, got {other:?}"),
    }
}

#[test]
fn the_thread_page_states_the_pitch_and_the_hand_and_apply_undoes() {
    let mut harness = page(opened());
    harness.run();
    assert!(harness.query_by_label("State the pitch").is_some(), "the Passes tab offers to state the pitch");
    assert!(harness.query_by_label("Thread pitch").is_none(), "an unstated pitch shows no number");
    assert!(harness.query_by_label("Radial infeed per pass").is_some());
    assert!(harness.query_by_label("Right hand").is_some() && harness.query_by_label("Left hand").is_some());
    harness.get_by_label("State the pitch").click();
    harness.run();
    harness.run();
    assert!(harness.query_by_label("Thread pitch").is_some(), "stating it shows the pitch row");
    harness.get_by_label("Left hand").click();
    harness.run();
    assert_eq!(thread(harness.state()), (None, ThreadHand::Right), "the world waits for Apply");
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert_eq!(thread(harness.state()), (Some(1.0), ThreadHand::Left), "Apply committed the pitch and the hand");
    let st = harness.state_mut();
    st.pending.push_back(CommandKind::Undo);
    st.drain_actions();
    assert_eq!(thread(harness.state()), (None, ThreadHand::Right), "Ctrl+Z restores both");
}

fn is_external(state: &State) -> bool {
    match model::find_by_name(&state.world, "Op").and_then(|e| model::operation_of(&state.world, &e)).map(|o| o.params) {
        Some(OpParams::Thread { external, .. }) => external,
        other => panic!("a thread op expected, got {other:?}"),
    }
}

#[test]
fn the_geometry_tab_switches_between_holes_and_bosses_and_apply_undoes() {
    let mut state = opened();
    state.op_tab = super::op_tabs::SEC_GEO;
    let mut harness = page(state);
    harness.run();
    assert!(harness.query_by_label("External thread (on a boss)").is_some(), "the Geometry tab offers the external thread");
    assert!(harness.query_by_label("Detect holes").is_some() && harness.query_by_label("Detect bosses").is_none());
    harness.get_by_label("External thread (on a boss)").click();
    harness.run();
    harness.run();
    assert!(harness.query_by_label("Detect bosses").is_some() && harness.query_by_label("Detect holes").is_none());
    assert!(!is_external(harness.state()), "the world waits for Apply");
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert!(is_external(harness.state()), "Apply committed the external thread");
    let st = harness.state_mut();
    st.pending.push_back(CommandKind::Undo);
    st.drain_actions();
    assert!(!is_external(harness.state()), "Ctrl+Z restores the internal thread");
}

#[test]
fn a_listed_feature_can_be_removed_from_the_draft() {
    use camcraft_app::drilling::Hole;
    use camcraft_app::geo2d::Point2;
    let mut state = opened();
    state.op_tab = super::op_tabs::SEC_GEO;
    let wall = |x: f64| Hole { xy: Point2::new(x, 30.0), radius: 5.0, entry_z: 20.0, floor_z: 8.0, through: false, stepped: false };
    if let OpParams::Thread { holes, .. } = &mut state.op_ui.params {
        *holes = vec![wall(20.0), wall(40.0)];
    }
    let mut harness = page(state);
    harness.run();
    assert!(harness.query_by_label("Remove 1").is_some() && harness.query_by_label("Remove 2").is_some());
    harness.get_by_label("Remove 1").click();
    harness.run();
    harness.run();
    assert!(harness.query_by_label("Remove 2").is_none(), "one row is left");
    match &harness.state().op_ui.params {
        OpParams::Thread { holes, .. } => assert_eq!(holes.iter().map(|h| h.xy.x).collect::<Vec<_>>(), vec![40.0]),
        other => panic!("a thread draft expected, got {other:?}"),
    }
}

fn passes(state: &State) -> camcraft_app::machining2d::ThreadPasses {
    match model::find_by_name(&state.world, "Op").and_then(|e| model::operation_of(&state.world, &e)).map(|o| o.params) {
        Some(OpParams::Thread { passes, .. }) => passes,
        other => panic!("a thread op expected, got {other:?}"),
    }
}

/// #988: the Passes tab picks the decreasing schedule, the Feeds tab
/// the M4 spindle with a left-hand-cut mill; each Apply is one undo step.
#[test]
fn the_pass_control_rows_edit_the_draft_and_apply_undoes() {
    use camcraft_app::machining2d::PassSchedule;
    let mut harness = page(opened());
    harness.run();
    for label in ["Decreasing", "Equal", "Finishing pass depth", "Spring passes", "Depth allowance"] {
        assert!(harness.query_by_label(label).is_some(), "the Passes tab shows {label}");
    }
    harness.get_by_label("Decreasing").click();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert_eq!(passes(harness.state()).schedule, PassSchedule::ConstantArea, "Apply committed the schedule");

    let st = harness.state_mut();
    st.op_tab = super::op_tabs::SEC_FEEDS;
    harness.run();
    harness.get_by_label("M4 (counter-clockwise)").click();
    harness.run();
    harness.get_by_label("Left-hand-cut thread mill").click();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    let got = passes(harness.state());
    assert!(got.spindle_reverse && got.cutter_left, "Apply committed the spindle and the cutter: {got:?}");
    let st = harness.state_mut();
    st.pending.push_back(CommandKind::Undo);
    st.drain_actions();
    let got = passes(harness.state());
    assert!(!got.spindle_reverse && !got.cutter_left, "Ctrl+Z restores M3: {got:?}");
}
