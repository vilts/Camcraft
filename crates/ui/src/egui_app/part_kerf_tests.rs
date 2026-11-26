//! The Part page's kerf rows (#3469): "The part is" (Cut-off piece /
//! Chuck piece) and "Back stock" on the Passes tab. The pick edits the
//! page draft, and Apply rides `EditOperationParams` - undo restores
//! the #3452 default.
//! Provenance: our own page law; no vendor material consulted.

use egui_kittest::kittest::Queryable;

use camcraft_app::commands::{AddTool, CreateOperation};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::turning::{PartKept, PartKerf};

use super::dialog_tests::page;
use crate::state::{CommandKind, State, UiTarget};

fn part_off() -> OpParams {
    OpParams::Part {
        tolerance: 0.05,
        clearance_z: Height::fixed(10.0),
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-20.0),
        stepdown: 2.0,
        feed: 120.0,
        plunge_feed: 60.0,
        bar_load: false,
        bar_advance: 0.0,
        bar_unload: false,
        bar_transfer: false,
        inspect_tol: 0.0,
        kerf: PartKerf::default(),
        picks: Default::default(),
    }
}

/// A mounted lathe, bar stock, the GT-0300 blade and one Part op named
/// "Op", opened on the page's Passes tab.
fn opened() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::LoadBuiltinMachine("Lathe".to_string()));
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Cylinder { diameter: 34.0, height: 84.0 },
        pos: [0.0, 0.0, -84.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let gt = catalog().into_iter().find(|(n, _)| *n == "GT-0300").map(|(_, t)| t).expect("catalog blade");
    state.history.execute(&state.world, AddTool::new(&state.world, "GT-0300", gt).expect("tool"));
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "Op", "GT-0300", SrcRef::Stock, part_off()).expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("Op".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("Op".into());
    state.sync_ops_ui();
    state.op_tab = super::op_tabs::SEC_PASSES;
    state
}

fn kerf(state: &State) -> PartKerf {
    match model::find_by_name(&state.world, "Op").and_then(|e| model::operation_of(&state.world, &e)).map(|o| o.params) {
        Some(OpParams::Part { kerf, .. }) => kerf,
        other => panic!("a Part op expected, got {other:?}"),
    }
}

#[test]
fn the_kept_piece_row_names_both_pieces_and_its_pick_applies_and_undoes() {
    let mut harness = page(opened());
    harness.run();
    for label in ["The part is", "Cut-off piece", "Chuck piece", "Back stock"] {
        assert!(harness.query_by_label(label).is_some(), "the Passes tab carries {label:?}");
    }
    assert_eq!(kerf(harness.state()), PartKerf::default(), "the cut-off piece unless the user says the chuck piece");
    harness.get_by_label("Chuck piece").click();
    harness.run();
    assert_eq!(kerf(harness.state()), PartKerf::default(), "the world waits for Apply");
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    let want = PartKerf { kept: PartKept::Chuck, back_stock: 0.0 };
    assert_eq!(kerf(harness.state()), want, "Apply committed the pick: {}", harness.state().status);
    let st = harness.state_mut();
    st.pending.push_back(CommandKind::Undo);
    st.drain_actions();
    assert_eq!(kerf(harness.state()), PartKerf::default(), "Ctrl+Z restores it");
    let st = harness.state_mut();
    st.pending.push_back(CommandKind::Redo);
    st.drain_actions();
    assert_eq!(kerf(harness.state()), want, "Ctrl+Y reapplies it");
}
