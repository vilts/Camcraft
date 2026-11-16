//! #535: the wire EDM through the real shell - the machine editor's
//! wire section (keys, codes, condition table, "not stated" where
//! absent, a refusal by name, one undo step) and the Wire Contour's
//! page (no Tool row, quick-add naming the machine's first condition,
//! a side edit that is one undo step). Structural only: no golden PNG.
//! Provenance: our own tests over the synthetic fixture (invented
//! codes, no vendor manual); specced from the issue's "Done means".
//!
//! Gate map (M4.4): covers ui::egui_app::machine_edm, ui::egui_app::machine_editor, ui::egui_app::op_page, ui::actions, app::machine::holders.

use egui::accesskit::Role;
use egui_kittest::kittest::NodeT;

use super::*;
use crate::egui_app::machine_editor::MachineEditor;

const FIXTURE: &str = include_str!("../../../../app/tests/edm/wire-iso-fixture.mch");

fn wire_machine() -> camcraft_app::machine::Machine {
    camcraft_app::machine::parse_mch(FIXTURE).expect("the fixture parses")
}

fn mount(h: &mut Harness<'static, ShellUi>, m: camcraft_app::machine::Machine) {
    h.state_mut().state.pending.push_back(CommandKind::EditMachineApply { path: "wire-test.mch".to_owned(), machine: Box::new(m) });
    h.run();
}

fn type_into(h: &mut Harness<'static, ShellUi>, field: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, field).scroll_to_me();
    h.run_steps(4);
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.input_mut().events.push(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run();
    h.get_by_role_and_label(Role::TextInput, field).type_text(text);
    h.run_steps(3);
}

fn press(h: &mut Harness<'static, ShellUi>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(4);
    h.get_by_label(label).click();
    h.run_steps(3);
}

/// Click the enabled button of that label (the op pane's own Apply
/// stands dark beside the machine window's).
fn press_live(h: &mut Harness<'static, ShellUi>, label: &str) {
    h.query_all_by_label(label).find(|n| !n.accesskit_node().is_disabled()).expect("a live button").scroll_to_me();
    h.run_steps(4);
    h.query_all_by_label(label).find(|n| !n.accesskit_node().is_disabled()).expect("a live button").click();
    h.run_steps(3);
}

fn mounted(h: &Harness<'static, ShellUi>) -> camcraft_app::machine::Machine {
    camcraft_app::machine::of(&h.state().state.world).1.expect("a machine is mounted")
}

#[test]
fn the_machine_editor_states_the_wire_section_refuses_by_name_and_undoes_in_one_step() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.run();
    let mut bare = wire_machine();
    bare.edm = None;
    bare.edm_codes.clear();
    mount(&mut h, bare.clone());
    h.state_mut().state.machine_editor = Some(MachineEditor { path: "wire-test.mch".to_owned(), machine: bare.clone() });
    h.run_steps(3);
    // Nothing is stated: every wire value wears "not stated"; no spindle.
    assert!(h.query_all_by_label("not stated").count() >= 8, "absent values say so");
    assert!(h.query_by_label("Spindle (rpm)").is_none(), "a wire EDM has no spindle");
    press(&mut h, "add condition");
    type_into(&mut h, "Condition 1 id", "C110");
    // A condition code without {id}: refused by name, nothing mounts.
    press(&mut h, "code condition");
    type_into(&mut h, "Code condition", "FX-COND");
    press_live(&mut h, "Apply");
    let status = h.state().state.status.clone();
    assert!(status.contains("edm.codes.condition") && status.contains("{id}"), "refused by name: {status}");
    assert_eq!(mounted(&h).edm, None, "a refused edit mounts nothing");
    assert!(h.state().state.machine_editor.is_some(), "the draft stays for the user to fix");
    // With {id} it applies, as one command.
    type_into(&mut h, "Code condition", "FX-COND {id}");
    press_live(&mut h, "Apply");
    let m = mounted(&h);
    assert_eq!(m.edm_codes.get("condition").map(String::as_str), Some("FX-COND {id}"));
    assert_eq!(m.edm.as_ref().map(|e| e.conditions.len()), Some(1));
    assert_eq!(m.edm.as_ref().map(|e| e.conditions[0].id.as_str()), Some("C110"));
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run();
    let m = mounted(&h);
    assert_eq!((m.edm, m.edm_codes.is_empty()), (None, true), "one undo takes the whole edit back");
}

#[test]
fn a_wire_contour_quick_adds_with_the_first_condition_and_its_page_has_no_tool_row() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.run();
    mount(&mut h, wire_machine());
    let kind = camcraft_app::operation::KindInfo::ALL.iter().position(|k| k.label == "Wire Contour").expect("the kind exists");
    crate::actions::add_operation(&mut h.state_mut().state, kind);
    h.run_steps(3);
    let name = h.state().state.selected_operation().expect("the op is created and selected");
    let op = camcraft_app::model::find_by_name(&h.state().state.world, &name)
        .and_then(|e| camcraft_app::model::operation_of(&h.state().state.world, &e))
        .expect("the operation exists");
    let camcraft_app::operation::OpParams::WireContour(w) = &op.params else { panic!("a wire contour") };
    assert_eq!(w.condition, "C110", "quick-add names the machine's first condition");
    assert!(op.tool.is_empty(), "the op mounts no library tool");
    assert!(h.query_by_label("Tool").is_none(), "no Tool row on a tool-less kind");
    // The side is one undo step.
    let side0 = w.side;
    assert!(h.query_by_label("Flood").is_none(), "a wire EDM has no coolant row");
    h.query_all_by_role(Role::ComboBox)
        .find(|n| n.accesskit_node().value().as_deref().is_some_and(|v| v.starts_with("Die")))
        .expect("the side combo")
        .click();
    h.run_steps(3);
    press(&mut h, "Punch - the wire runs outside the profile");
    press(&mut h, "Apply");
    let side = |h: &Harness<'static, ShellUi>| {
        let e = camcraft_app::model::find_by_name(&h.state().state.world, &name).expect("op");
        match camcraft_app::model::operation_of(&h.state().state.world, &e).map(|o| o.params) {
            Some(camcraft_app::operation::OpParams::WireContour(w)) => Some(w.side),
            _ => None,
        }
    };
    assert_ne!(side(&h), Some(side0), "Apply changed the side");
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run();
    assert_eq!(side(&h), Some(side0), "one undo restores it");
}

fn editor_harness(machine: camcraft_app::machine::Machine, path: &str) -> Harness<'static, crate::state::State> {
    let mut state = crate::state::State::new();
    state.pending.push_back(CommandKind::EditMachineApply { path: path.to_owned(), machine: Box::new(machine.clone()) });
    state.drain_actions();
    state.machine_editor = Some(MachineEditor { path: path.to_owned(), machine });
    let mut h = Harness::builder()
        .with_size(Vec2::new(900.0, 1600.0))
        .build_ui_state(|ui, st: &mut crate::state::State| super::super::machine_editor::window(ui.ctx(), st), state);
    h.run_steps(3);
    h
}

/// #1119: every input in the Wire EDM machine editor carries an accessible name.
#[test]
fn the_wire_machine_editor_names_every_input() {
    let mut m = wire_machine();
    if let Some(edm) = m.edm.as_mut() {
        edm.lower_guide_z.get_or_insert(0.0);
        edm.upper_guide_z.get_or_insert(50.0);
        edm.max_taper_deg.get_or_insert(10.0);
        if edm.wire_diameters.is_empty() {
            edm.wire_diameters.push(0.25);
        }
        if let Some(c) = edm.conditions.first_mut() {
            c.gap.get_or_insert(0.02);
            c.wire_diameter.get_or_insert(0.25);
        }
    }
    let h = editor_harness(m, "wire-test.mch");
    let (bad, seen) = super::super::fixture_plate_tests::unnamed_inputs(&h);
    assert!(seen >= 10, "only {seen} inputs found in wire machine editor");
    assert!(bad.is_empty(), "unnamed inputs in wire machine editor: {bad:?}");
}

/// #1119: every input in the mill/lathe machine editor carries an accessible name.
#[test]
fn the_mill_machine_editor_names_every_input() {
    let mut state = crate::state::State::new();
    state.pending.push_back(CommandKind::LoadBuiltinMachine("Tormach 770MX".into()));
    state.drain_actions();
    let mut m = camcraft_app::machine::of(&state.world).1.expect("770MX mounted");
    m.spindle_min.get_or_insert(100.0);
    m.spindle_max.get_or_insert(10_000.0);
    m.spindle_power.get_or_insert(1.5);
    m.spindle_d.get_or_insert(60.0);
    m.spindle_len.get_or_insert(90.0);
    m.max_tool_length.get_or_insert(150.0);
    m.max_tool_diameter.get_or_insert(50.0);
    m.tool_count.get_or_insert(10);
    if let Some(a) = m.axes.first_mut() {
        a.rapid.get_or_insert(5000.0);
        a.feed.get_or_insert(2000.0);
        a.travel.get_or_insert((-100.0, 100.0));
        a.resolution.get_or_insert(0.001);
    }
    m.coolants.push(camcraft_app::machine::Coolant {
        name: "ShopAir".to_owned(),
        m_code: "M7".to_owned(),
    });
    let h = editor_harness(m, "mill-test.mch");
    let (bad, seen) = super::super::fixture_plate_tests::unnamed_inputs(&h);
    assert!(seen >= 20, "only {seen} inputs found in mill machine editor");
    assert!(bad.is_empty(), "unnamed inputs in mill machine editor: {bad:?}");
}
