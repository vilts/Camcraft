//! The turning kinds' Start point / Exit point groups (#3331), driven
//! through the real page code: a Facing shows both groups off, the switch
//! seeds a point with a Z, Apply commits it through the command history
//! and undo clears it. Threading says by name that its own law sets the
//! start.
//! Provenance: our own page law; no vendor material consulted.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::commands::{AddTool, CreateOperation};
use camcraft_app::model;
use camcraft_app::operation::{KindInfo, OpParams, OpType, SrcRef, PICK_THREAD_LAW};
use camcraft_app::path_picks::{PathPicks, PickRole};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use super::super::dialog_tests::page;
use super::super::op_tabs::SEC_GEO;
use super::super::start_exit_rows::{EXIT_GROUP, NOT_TAKEN, START_GROUP};
use crate::state::{CommandKind, State, UiTarget};

const INSERT: &str = "TN-1600-0R8";
const THREAD_INSERT: &str = "TD-1732-60";

/// A mounted lathe, bar stock, a turning insert and one operation of
/// `ty` named "Op" (the registry's quick-add seed), on the Geometry tab.
fn opened(ty: OpType) -> Harness<'static, State> {
    let mut state = State::new();
    state.pending.push_back(CommandKind::LoadBuiltinMachine("Lathe".to_string()));
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Cylinder { diameter: 34.0, height: 84.0 },
        pos: [0.0, 0.0, -84.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    // Threading cuts with a threading insert; the other kinds with a turning insert.
    let name = if ty == OpType::Threading { THREAD_INSERT } else { INSERT };
    let insert = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog insert");
    state.history.execute(&state.world, AddTool::new(&state.world, name, insert).expect("tool"));
    let kind = KindInfo::ALL.iter().position(|k| k.ty == ty).expect("the kind");
    let params = crate::operations::retype(&super::super::dialog_tests::face(), kind);
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "Op", name, SrcRef::Stock, params).expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("Op".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("Op".into());
    state.sync_ops_ui();
    state.op_tab = SEC_GEO;
    let mut harness = page(state);
    harness.run();
    harness
}

fn live(state: &State) -> PathPicks {
    model::find_by_name(&state.world, "Op")
        .and_then(|e| model::operation_of(&state.world, &e))
        .map(|o| o.params.path_picks())
        .expect("the op")
}

fn draft(h: &Harness<'static, State>) -> PathPicks {
    h.state().op_ui.params.path_picks()
}

#[test]
fn the_turning_kinds_show_both_point_groups_off_by_default() {
    for ty in [OpType::Facing, OpType::ProfileRough, OpType::ProfileFinish] {
        let harness = opened(ty);
        for label in [START_GROUP, EXIT_GROUP, "Start point stated", "Exit point stated"] {
            assert!(harness.query_by_label(label).is_some(), "{ty:?} has {label} on Geometry");
        }
        for row in ["Start Z", "Start X", "Exit Z", "Exit X"] {
            assert!(harness.query_by_label(row).is_none(), "{ty:?}: {row} hides while the point is not stated");
        }
        assert_eq!(harness.query_all_by_label_contains(NOT_TAKEN).count(), 0, "{ty:?} takes both picks");
        assert!(draft(&harness).is_empty(), "drawing the tab writes no point");
    }
}

#[test]
fn a_stated_start_point_has_a_z_and_applies_through_the_history() {
    let mut harness = opened(OpType::Facing);
    harness.get_by_label("Start point stated").click();
    harness.run();
    harness.run();
    for row in ["Start Z", "Start X"] {
        assert!(harness.query_by_label(row).is_some(), "{row} shows when the start is stated");
    }
    assert!(harness.query_by_label("Exit Z").is_none(), "the exit stays off");
    assert!(harness.query_by_label_contains("refused by name - the start never moves").is_some());
    let start = draft(&harness).first(PickRole::Start).expect("the seeded start");
    assert_eq!((start.xy.x, start.xy.y, start.z), (0.0, 0.0, Some(0.0)), "the seed is a point of the (Z, X) plane");
    assert!(live(harness.state()).is_empty(), "the world waits for Apply");

    // A point in air: past the bar's free end (z 0) and outside its radius.
    if let Some(p) = harness.state_mut().op_ui.params.path_picks_mut().and_then(|p| p.start.first_mut()) {
        p.z = Some(5.0);
        p.xy.x = 25.0;
    }
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    let got = live(harness.state()).first(PickRole::Start).expect("Apply committed the pick");
    assert_eq!((got.xy.x, got.z), (25.0, Some(5.0)), "{}", harness.state().status);

    let st = harness.state_mut();
    st.pending.push_back(CommandKind::Undo);
    st.drain_actions();
    assert!(live(harness.state()).is_empty(), "Ctrl+Z clears the pick");
    let st = harness.state_mut();
    st.pending.push_back(CommandKind::Redo);
    st.drain_actions();
    assert_eq!(live(harness.state()).first(PickRole::Start), Some(got), "Ctrl+Y restores it");
}

#[test]
fn switching_a_stated_point_off_clears_it() {
    let mut harness = opened(OpType::ProfileFinish);
    harness.get_by_label("Exit point stated").click();
    harness.run();
    harness.run();
    assert!(draft(&harness).first(PickRole::Exit).is_some());
    harness.get_by_label("Exit point stated").click();
    harness.run();
    harness.run();
    assert!(draft(&harness).is_empty(), "off writes no pick - the automatic exit");
    assert!(harness.query_by_label("Exit Z").is_none(), "and the rows leave the tree");
}

#[test]
fn threading_says_by_name_that_the_thread_start_law_sets_its_start() {
    let harness = opened(OpType::Threading);
    assert!(matches!(harness.state().op_ui.params, OpParams::Threading { .. }));
    assert_eq!(harness.query_all_by_label_contains(NOT_TAKEN).count(), 2, "the start and the exit are not taken");
    assert_eq!(harness.query_all_by_label_contains(PICK_THREAD_LAW).count(), 2, "each names the thread start law");
    assert!(harness.query_by_label("Start point stated").is_none(), "no switch on Threading");
}
