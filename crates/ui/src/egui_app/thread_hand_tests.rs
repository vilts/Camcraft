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

fn dip_values(state: &State, draft: bool) -> camcraft_app::turning::ThreadDips {
    let params = if draft {
        state.op_ui.params.clone()
    } else {
        model::find_by_name(&state.world, "Op")
            .and_then(|e| model::operation_of(&state.world, &e))
            .expect("threading operation")
            .params
    };
    match params {
        OpParams::Threading { infeed_plan, .. } => infeed_plan.dips,
        other => panic!("a threading op expected, got {other:?}"),
    }
}

/// Edit the actual numeric widget beside its painted label, without
/// reaching into the page draft. The form seam does not name these spins.
fn edit_number(harness: &mut egui_kittest::Harness<'static, State>, label: &str, text: &str) {
    use egui::accesskit::Role;
    let row = harness.get_by_label(label).rect();
    let spin = harness
        .query_all_by(|n| n.role() == Role::SpinButton)
        .find(|n| (n.rect().center().y - row.center().y).abs() < 2.0)
        .expect("a numeric control on the named row")
        .rect();
    // #2516: Enter in the field above moves the focus here with the text
    // selected; a click on the focused field would only place the cursor.
    let node = harness.query_all_by(|n| n.role() == Role::SpinButton).find(|n| n.rect() == spin).expect("spin");
    if !node.is_focused() {
        node.click();
        harness.run();
    }
    harness
        .query_all_by(|n| n.role() == Role::SpinButton)
        .find(|n| (n.rect().top() - spin.top()).abs() < 1.0)
        .expect("editing the number")
        .type_text(text);
    harness.run();
    // Commit the text edit before another click changes the layout.
    harness.key_press(egui::Key::Enter);
    harness.run();
}

fn dips(state: &State) -> camcraft_app::turning::ThreadDips {
    match model::find_by_name(&state.world, "Op").and_then(|e| model::operation_of(&state.world, &e)).map(|o| o.params) {
        Some(OpParams::Threading { infeed_plan, .. }) => infeed_plan.dips,
        other => panic!("a threading op expected, got {other:?}"),
    }
}

fn dip_number<'a>(
    h: &'a egui_kittest::Harness<'static, State>,
    label: &str,
) -> egui_kittest::Node<'a> {
    let row = h.get_by_label(label).rect();
    h.query_all_by(|n| n.role() == egui::accesskit::Role::SpinButton)
        .find(|n| (n.rect().center().y - row.center().y).abs() < 12.0)
        .expect("a numeric control beside the dip label")
}

fn type_dip(h: &mut egui_kittest::Harness<'static, State>, label: &str, value: &str) {
    // #2516: Enter in the field above moves the focus here with the text
    // selected; a click on the focused field would only place the cursor.
    if !dip_number(h, label).is_focused() {
        dip_number(h, label).click();
        h.run();
    }
    dip_number(h, label).type_text(value);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
}

#[test]
fn chip_break_controls_disclose_the_dip_dimensions_only_when_enabled() {
    let mut h = page(opened());
    h.run();
    assert!(h.query_by_label("Chip-break every").is_some());
    assert!(h.query_by_label("Dip height").is_none());
    assert!(h.query_by_label("Dip length").is_none());
    type_dip(&mut h, "Chip-break every", "4");
    assert_eq!(dip_values(h.state(), true).interval, 4.0);
    for label in ["Dip height", "Dip length"] {
        assert!(
            h.query_by_label(label).is_some(),
            "enabled dips disclose {label}"
        );
        let _number = dip_number(&h, label);
    }
    type_dip(&mut h, "Chip-break every", "0");
    assert!(h.query_by_label("Dip height").is_none());
    assert!(h.query_by_label("Dip length").is_none());
    assert_eq!(
        dip_values(h.state(), false),
        camcraft_app::turning::ThreadDips::default(),
        "editing waits for Apply"
    );
}

#[test]
fn inch_dip_controls_convert_then_apply_undo_and_redo_all_three_values() {
    let mut state = opened();
    state
        .pending
        .push_back(CommandKind::SetUnits(camcraft_app::units::Unit::Inches));
    state.drain_actions();
    let mut h = page(state);
    h.run();
    type_dip(&mut h, "Chip-break every", "0.2");
    type_dip(&mut h, "Dip height", "0.01");
    type_dip(&mut h, "Dip length", "0.05");
    let expected = camcraft_app::turning::ThreadDips {
        interval: 5.08,
        height: 0.254,
        length: 1.27,
    };
    assert_eq!(
        dip_values(h.state(), true),
        expected,
        "all three inch inputs store millimeters"
    );
    let original = camcraft_app::turning::ThreadDips::default();
    assert_eq!(
        dip_values(h.state(), false),
        original,
        "the world waits for Apply"
    );
    h.get_by_label("Apply").click();
    h.run();
    h.state_mut().drain_actions();
    assert_eq!(
        dip_values(h.state(), false),
        expected,
        "Apply commits all three dip values: {}",
        h.state().status
    );
    h.state_mut().pending.push_back(CommandKind::Undo);
    h.state_mut().drain_actions();
    assert_eq!(
        dip_values(h.state(), false),
        original,
        "one undo restores all three values"
    );
    h.state_mut().pending.push_back(CommandKind::Redo);
    h.state_mut().drain_actions();
    assert_eq!(
        dip_values(h.state(), false),
        expected,
        "redo restores all three values"
    );
}

#[test]
fn chip_break_controls_are_conditional_and_apply_and_undo_all_values() {
    use camcraft_app::turning::ThreadDips;

    let mut harness = page(opened());
    harness.run();
    let original = dips(harness.state());
    assert_eq!(original, ThreadDips::default());
    assert!(harness.query_by_label("Chip-break every").is_some(), "interval is always available");
    assert!(harness.query_by_label("Dip height").is_none(), "disabled dips hide height");
    assert!(harness.query_by_label("Dip length").is_none(), "disabled dips hide length");

    edit_number(&mut harness, "Chip-break every", "4");
    assert!(harness.query_by_label("Dip height").is_some(), "positive interval reveals height");
    assert!(harness.query_by_label("Dip length").is_some(), "positive interval reveals length");
    edit_number(&mut harness, "Dip height", "0.1");
    edit_number(&mut harness, "Dip length", "1");
    let changed = ThreadDips { interval: 4.0, height: 0.1, length: 1.0 };
    assert!(matches!(&harness.state().op_ui.params,
        OpParams::Threading { infeed_plan, .. } if infeed_plan.dips == changed), "the controls edit all draft values");
    assert_eq!(dips(harness.state()), original, "the world waits for Apply");

    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert_eq!(dips(harness.state()), changed, "Apply stores all three chip-break values");
    let state = harness.state_mut();
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(dips(harness.state()), original, "Undo restores all three original values");
}
