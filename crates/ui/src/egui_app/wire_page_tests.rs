//! #1148: wire skim and tab controls edit the draft, then Apply and undo.
//! Provenance: our own UI contract over synthetic wire conditions.
//! Gate map (M4.4): covers ui::egui_app::wire_page.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::{Queryable, NodeT}};
use camcraft_app::{model, operation::{OpParams, WireContourOp}};
use crate::state::{CommandKind, State};
use super::super::dialog_tests::page;

fn wire_page() -> Harness<'static, State> {
    let mut st = super::super::dialog_tests::opened(super::super::dialog_tests::face());
    let machine = camcraft_app::machine::parse_mch(include_str!("../../../app/tests/edm/wire-iso-fixture.mch")).expect("fixture");
    st.pending.push_back(CommandKind::EditMachineApply { path: "wire-test.mch".into(), machine: Box::new(machine) });
    st.drain_actions();
    let kind = camcraft_app::operation::KindInfo::ALL.iter().position(|k| k.label == "Wire Contour").expect("wire kind");
    crate::actions::add_operation(&mut st, kind);
    st.drain_actions();
    st.sync_ops_ui();
    st.op_tab = super::super::op_tabs::SEC_PASSES;
    let mut h = page(st);
    h.run();
    assert_eq!(live(&h).condition, "C110", "the fixture starts with a live rough pass");
    h
}

fn draft<'a>(h: &'a Harness<'static, State>) -> &'a WireContourOp {
    let OpParams::WireContour(w) = &h.state().op_ui.params else { panic!("wire draft") };
    w
}

fn live(h: &Harness<'static, State>) -> WireContourOp {
    let st = h.state();
    let name = st.selected_operation().unwrap_or_else(|| panic!("selected operation: {}", st.status));
    let op = model::find_by_name(&st.world, &name).and_then(|e| model::operation_of(&st.world, &e)).expect("operation");
    let OpParams::WireContour(w) = op.params else { panic!("wire operation") };
    w
}

fn press(h: &mut Harness<'static, State>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).click();
    h.run_steps(3);
}

fn choose(h: &mut Harness<'static, State>, row: &str, choice: &str) {
    let r = h.get_by_label(row).rect();
    h.query_all_by_role(Role::ComboBox)
        .find(|n| (n.rect().center().y - r.center().y).abs() < 12.0)
        .expect("condition combo on row").click();
    h.run_steps(3);
    press(h, choice);
}

#[test]
fn wire_skims_add_reorder_remove_and_apply_undo() {
    let mut h = wire_page();
    press(&mut h, "Add skim");
    choose(&mut h, "Skim 1 condition", "S21");
    press(&mut h, "Skim 1 reverse");
    press(&mut h, "Add skim");
    choose(&mut h, "Skim 2 condition", "S31");
    assert_eq!(draft(&h).skims.len(), 2);
    assert!(draft(&h).skims[0].reverse);
    press(&mut h, "Move skim 2 up");
    assert_eq!(draft(&h).skims[0].condition, "S31");
    press(&mut h, "Move skim 1 down");
    assert_eq!(draft(&h).skims[0].condition, "S21");
    press(&mut h, "Remove skim 2");
    assert_eq!(draft(&h).skims.len(), 1);
    assert!(live(&h).skims.is_empty(), "edits wait for Apply");
    press(&mut h, "Apply");
    h.state_mut().drain_actions();
    assert_eq!(live(&h).skims, draft(&h).skims);
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert!(live(&h).skims.is_empty(), "one undo restores the skim list");
}

#[test]
fn wire_tabs_and_capture_expose_all_stated_fields() {
    let mut h = wire_page();
    press(&mut h, "Freed piece captured");
    assert!(draft(&h).capture);
    press(&mut h, "Hold with tabs");
    press(&mut h, "Tab count stated");
    press(&mut h, "Tab width stated");
    for label in ["Tab count", "Tab width", "Tab cut-off condition"] {
        assert!(h.query_by_label(label).is_some(), "{label} is editable");
    }
    choose(&mut h, "Tab cut-off condition", "S21");
    press(&mut h, "Add tab position");
    for label in ["Tab 1 X", "Tab 1 Y", "Remove tab position 1"] {
        assert!(h.query_by_label(label).is_some(), "{label} is editable");
    }
    let tabs = draft(&h).tabs.as_ref().expect("tabs");
    assert!(tabs.count.is_some() && tabs.width.is_some());
    assert_eq!(tabs.cutoff_condition, "S21");
    assert_eq!(tabs.at.len(), 1);
    press(&mut h, "Remove tab position 1");
    assert!(draft(&h).tabs.as_ref().expect("tabs").at.is_empty());
    press(&mut h, "Hold with tabs");
    assert!(draft(&h).tabs.is_none());
}

#[test]
fn wire_skim_apply_shows_each_named_refusal() {
    for (ids, reason) in [
        (vec![""], "skim 1 of the Wire Contour names no EDM condition"),
        (vec!["S31", "S21"], "cut past the finished profile"),
        (vec!["S21", "C110", "S31"], "cut only air"),
        (vec!["W20"], "one threading cuts every pass with one wire"),
    ] {
        let mut h = wire_page();
        for (i, id) in ids.iter().enumerate() {
            press(&mut h, "Add skim");
            if !id.is_empty() {
                choose(&mut h, &format!("Skim {} condition", i + 1), id);
            }
        }
        press(&mut h, "Apply");
        h.state_mut().drain_actions();
        h.run_steps(3);
        assert!(live(&h).skims.is_empty(), "a refused edit changes no live parameters");
        assert!(h.query_by_label_contains(reason).is_some(), "the page names {reason}: {}", h.state().status);
    }
}

#[test]
fn wire_skim_limits_and_order_controls_disable_invalid_actions() {
    let mut h = wire_page();
    if let OpParams::WireContour(w) = &mut h.state_mut().op_ui.params {
        w.skims = vec![camcraft_app::operation::WireSkim::default(); camcraft_app::operation::SKIMS_MAX];
    }
    h.run_steps(3);
    for label in ["Add skim", "Move skim 1 up", "Move skim 16 down"] {
        assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{label} must be disabled");
    }
    assert!(!h.get_by_label("Move skim 1 down").accesskit_node().is_disabled());
    assert!(!h.get_by_label("Move skim 16 up").accesskit_node().is_disabled());
}

#[test]
fn wire_tab_position_limit_disables_add() {
    let mut h = wire_page();
    if let OpParams::WireContour(w) = &mut h.state_mut().op_ui.params {
        w.tabs = Some(camcraft_app::operation::WireTabs { at: vec![[0.0, 0.0]; 1024], ..Default::default() });
    }
    h.run_steps(3);
    assert!(h.get_by_label("Add tab position").accesskit_node().is_disabled());
    assert_eq!(draft(&h).tabs.as_ref().expect("tabs").at.len(), 1024);
}

#[test]
fn wire_unknown_condition_ids_stay_visible_and_unchanged() {
    let mut h = wire_page();
    if let OpParams::WireContour(w) = &mut h.state_mut().op_ui.params {
        w.condition = "missing rough".into();
        w.skims.push(camcraft_app::operation::WireSkim { condition: "missing skim".into(), reverse: false });
        w.tabs = Some(camcraft_app::operation::WireTabs { cutoff_condition: "missing cutoff".into(), ..Default::default() });
    }
    h.run_steps(3);
    for id in ["missing rough", "missing skim", "missing cutoff"] {
        assert!(h.query_all_by_role(Role::ComboBox).any(|n| n.value().as_deref() == Some(&format!("{id} (not stated by the machine)"))), "{id} remains visible");
    }
    assert_eq!(draft(&h).condition, "missing rough");
    assert_eq!(draft(&h).skims[0].condition, "missing skim");
    assert_eq!(draft(&h).tabs.as_ref().expect("tabs").cutoff_condition, "missing cutoff");
}

#[test]
fn wire_sync_pick_button_arms_and_says_the_next_click() {
    let mut h = wire_page();
    h.state_mut().op_tab = super::super::op_tabs::SEC_GEO;
    h.run_steps(3);
    press(&mut h, "4-axis (upper and lower profiles)");
    press(&mut h, "Pairing");
    choose(&mut h, "Pairing", "By sync points");
    assert!(h.query_by_label("Pick sync points in the viewport").is_some(), "the pick button shows");
    press(&mut h, "Pick sync points in the viewport");
    assert!(h.state().wire_sync.armed, "the button arms the pick");
    assert!(h.query_by_label("Stop picking sync points").is_some());
    assert!(h.query_by_label("Next: click the upper point of pair 1 (a vertex of the upper profile)").is_some());
    press(&mut h, "Stop picking sync points");
    assert!(!h.state().wire_sync.armed);
}
