//! The threading page's Hand row (#629): Right hand / Left hand by name
//! on the Passes tab, the pick editing the page draft and Apply riding
//! `EditOperationParams` - undo restores the hand.
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

fn threading() -> OpParams {
    OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(14.3),
        bottom_z: Height::fixed(7.7),
        pitch: 2.0,
        depth: 1.2,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: Default::default(),
        hand: ThreadHand::Right,
    }
}

/// A mounted lathe, bar stock, the threading insert and one threading
/// op named "Op", opened on the page's Passes tab.
fn opened() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::LoadBuiltinMachine("Lathe".to_string()));
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Cylinder { diameter: 12.0, height: 56.0 },
        pos: [0.0, 0.0, -16.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let td = catalog().into_iter().find(|(n, _)| *n == "TD-1732-60").map(|(_, t)| t).expect("catalog insert");
    state.history.execute(&state.world, AddTool::new(&state.world, "TD-1732-60", td).expect("tool"));
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "Op", "TD-1732-60", SrcRef::Stock, threading()).expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("Op".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("Op".into());
    state.sync_ops_ui();
    state.op_tab = super::op_tabs::SEC_PASSES;
    state
}

fn hand(state: &State) -> ThreadHand {
    match model::find_by_name(&state.world, "Op").and_then(|e| model::operation_of(&state.world, &e)).map(|o| o.params) {
        Some(OpParams::Threading { hand, .. }) => hand,
        other => panic!("a threading op expected, got {other:?}"),
    }
}

#[test]
fn the_hand_row_names_both_hands_and_its_pick_applies_and_undoes() {
    let mut harness = page(opened());
    harness.run();
    assert!(harness.query_by_label("Hand").is_some(), "the Passes tab carries the Hand row");
    assert!(harness.query_by_label("Right hand").is_some() && harness.query_by_label("Left hand").is_some());
    assert_eq!(hand(harness.state()), ThreadHand::Right, "right-hand unless the drawing says LH");
    harness.get_by_label("Left hand").click();
    harness.run();
    assert_eq!(hand(harness.state()), ThreadHand::Right, "the world waits for Apply");
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert_eq!(hand(harness.state()), ThreadHand::Left, "Apply committed the hand");
    let st = harness.state_mut();
    st.pending.push_back(CommandKind::Undo);
    st.drain_actions();
    assert_eq!(hand(harness.state()), ThreadHand::Right, "Ctrl+Z restores it");
    let st = harness.state_mut();
    st.pending.push_back(CommandKind::Redo);
    st.drain_actions();
    assert_eq!(hand(harness.state()), ThreadHand::Left, "Ctrl+Y reapplies it");
}
