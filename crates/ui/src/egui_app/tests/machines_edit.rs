//! The Machines pane's file editor through the real shell (issue #383):
//! a machine made from a template has its travels and coolant and
//! live-tool codes typed in and is saved to the user machines folder;
//! a refusal sits on its field and blocks the save; a shipped machine's
//! Save becomes Save As; a hand-written file keeps its unknown keys.
//! None of it touches the command queue or the history - editing a
//! machine FILE is a save with its own confirmation, not a document edit.
//! Provenance: our own tests, specced from the issue's "Done means" and
//! the egui_kittest vendored sources; no HSMWorks material involved.
//! Structural only: no golden PNG.
//!
//! Gate map (M4.4): covers ui::egui_app::machines_pane, ui::egui_app::machine_file_editor, app::machine::draft, app::machine::fields, app::machine::store.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;

use super::machines_pane::{assert_no_commands, harness_with_user_dir, open_machines};
use super::*;
use crate::egui_app::machine_file_editor::Prompt;

/// A fresh, empty user machines folder for one test.
fn user_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft_ui_machines_edit_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    d
}

fn select_row(h: &mut Harness<'static, ShellUi>, name: &str) {
    let label = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .and_then(|l| l.iter().find(|e| e.name == name))
        .map(super::super::machines_pane::row_label)
        .unwrap_or_else(|| panic!("the catalogue lists {name}"));
    h.get_by_label(&label).scroll_to_me();
    h.run_steps(10);
    h.get_by_label(&label).click();
    h.run_steps(2);
}

fn press(h: &mut Harness<'static, ShellUi>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(4);
    h.get_by_label(label).click();
    h.run_steps(3);
}

/// Replace a grid field's text the way a user does: click it, select
/// all, type.
fn type_into(h: &mut Harness<'static, ShellUi>, field: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, field).scroll_to_me();
    h.run_steps(6);
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run();
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::NONE));
    h.get_by_role_and_label(Role::TextInput, field).type_text(text);
    h.run_steps(3);
}

fn draft_at(h: &Harness<'static, ShellUi>, pointer: &str) -> Option<serde_json::Value> {
    h.state().state.machines_pane.editor.as_ref().and_then(|e| e.draft.value.pointer(pointer).cloned())
}

fn prompt(h: &Harness<'static, ShellUi>) -> Option<Prompt> {
    h.state().state.machines_pane.editor.as_ref().and_then(|e| e.prompt.clone())
}

fn read_json(p: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(p).expect("read back")).expect("json")
}

#[test]
fn a_template_machine_takes_new_travels_and_codes_and_saves_to_the_user_folder() {
    let dir = user_dir("template");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    select_row(&mut h, "Lathe");
    press(&mut h, "New from template");
    let model = h.state().state.machines_pane.editor.as_ref().map(|e| e.draft.model());
    assert_eq!(model.as_deref(), Some("TURNING CENTER 2-AXIS copy"));
    // Travels: the lathe's X turret is the first part under the bed.
    type_into(&mut h, "X travel max", "350");
    let x = "/kinematics/default/parts/0/parts/0";
    assert_eq!(draft_at(&h, &format!("{x}/travel/max")).and_then(|v| v.as_f64()), Some(350.0));
    type_into(&mut h, "Z travel min", "-700");
    assert_eq!(draft_at(&h, &format!("{x}/parts/0/travel/min")).and_then(|v| v.as_f64()), Some(-700.0));
    // Coolant code.
    type_into(&mut h, "Coolant Flood code", "M7");
    // Live-tool codes: one alone is refused on its field and blocks the save.
    type_into(&mut h, "Live tool C on", "M154");
    h.get_by_label("Validation: 1 refusal - save blocked");
    assert!(h.query_all_by_label_contains("live_tool: states 1 of the 4 words").count() >= 1);
    press(&mut h, "Save machine");
    assert_eq!(prompt(&h), None, "a blocked save asks nothing");
    for (f, c) in [("Live tool C off", "M155"), ("Live tool spindle on", "M133"), ("Live tool spindle off", "M135")] {
        type_into(&mut h, f, c);
    }
    h.get_by_label("Validation: passes");
    // Save on a never-saved copy is Save As, named from the model.
    press(&mut h, "Save machine");
    assert_eq!(prompt(&h), Some(Prompt::SaveAs));
    press(&mut h, "Write file");
    let path = dir.join("turning-center-2-axis-copy.mch");
    assert!(path.is_file(), "the machine was written to the user folder");
    let v = read_json(&path);
    assert_eq!(v.pointer(&format!("{x}/travel/max")).and_then(|v| v.as_f64()), Some(350.0));
    assert_eq!(v.pointer(&format!("{x}/parts/0/travel/min")).and_then(|v| v.as_f64()), Some(-700.0));
    let ts = "/kinematics/default/parts/0/parts/1/tool_station";
    assert_eq!(v.pointer(&format!("{ts}/coolants/0/mCode")).and_then(|v| v.as_str()), Some("M7"));
    assert_eq!(v.pointer(&format!("{ts}/live_tool/spindle_off")).and_then(|v| v.as_str()), Some("M135"));
    // The catalogue re-read lists it, selected; the editor says so.
    let st = &h.state().state.machines_pane;
    let sel = st.selected.and_then(|i| st.list.as_ref().and_then(|l| l.get(i)));
    assert_eq!(sel.map(|e| e.name.as_str()), Some("TURNING CENTER 2-AXIS copy"));
    assert!(sel.is_some_and(|e| e.is_valid()));
    h.get_by_label(&format!("Saved {}", path.display()));
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_backwards_travel_is_refused_on_its_field_and_nothing_is_written() {
    let dir = user_dir("refused");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    h.state_mut().state.machines_pane.kind_filter = Some(0); // New blank needs a kind (#816)
    h.run_steps(2);
    press(&mut h, "New blank");
    type_into(&mut h, "X travel min", "100");
    type_into(&mut h, "X travel max", "50");
    h.get_by_label("Validation: 1 refusal - save blocked");
    // Shown under both ends of the stroke.
    assert_eq!(h.query_all_by_label("X travel: min 100 must be below max 50").count(), 2);
    press(&mut h, "Save machine");
    press(&mut h, "Save machine as...");
    assert_eq!(prompt(&h), None, "both saves are blocked");
    assert_eq!(std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0), 0, "nothing written");
    // Fixing the field unblocks it.
    type_into(&mut h, "X travel max", "150");
    h.get_by_label("Validation: passes");
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// GUI-test finding on PR #478: a bare `M` in an M-code field passed
/// validation. It is refused under its field and blocks both saves.
#[test]
fn a_bare_m_code_is_refused_on_its_field_and_blocks_the_save() {
    let dir = user_dir("bare_m");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    select_row(&mut h, "Lathe");
    press(&mut h, "New from template");
    for (f, c) in [("Live tool C on", "M154"), ("Live tool C off", "M155"), ("Live tool spindle on", "M133"), ("Live tool spindle off", "M135")] {
        type_into(&mut h, f, c);
    }
    h.get_by_label("Validation: passes");
    type_into(&mut h, "Live tool C on", "M");
    h.get_by_label("Validation: 1 refusal - save blocked");
    h.get_by_label(r#"live_tool.c_axis_on: code "M" is not an M word like M8 (no number after M)"#);
    type_into(&mut h, "Coolant Flood code", "M");
    h.get_by_label("Validation: 2 refusals - save blocked");
    h.get_by_label(r#"coolant Flood: code "M" is not an M word like M8 (no number after M)"#);
    press(&mut h, "Save machine");
    press(&mut h, "Save machine as...");
    assert_eq!(prompt(&h), None, "both saves are blocked");
    assert_eq!(std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0), 0, "nothing written");
    type_into(&mut h, "Live tool C on", "M08");
    type_into(&mut h, "Coolant Flood code", "M7");
    h.get_by_label("Validation: passes");
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn editing_a_shipped_machine_saves_as_a_copy() {
    let dir = user_dir("shipped");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    select_row(&mut h, "VMC 3-axis");
    press(&mut h, "Edit machine");
    assert!(h.state().state.machines_pane.editor.as_ref().is_some_and(|e| e.draft.read_only));
    h.get_by_label("Shipped machine - read-only; Save As writes your own copy");
    press(&mut h, "Save machine");
    assert_eq!(prompt(&h), Some(Prompt::SaveAs), "Save on a shipped machine is Save As");
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hand_written_user_file_keeps_its_unknown_keys_through_an_edit_and_save() {
    let dir = user_dir("unknown");
    let path = dir.join("shop.mch");
    let text = r#"{
      "general": { "model": "SHOP MILL", "kind": "mill" },
      "kinematics": { "default": { "units": { "length": "mm" }, "parts": [
        { "name": "X", "type": "linear", "travel": { "min": -100.0, "max": 100.0 } },
        { "name": "Y", "type": "linear" }, { "name": "Z", "type": "linear" }
      ] } },
      "x_shop_notes": { "probe": "OMP40" }
    }"#;
    std::fs::write(&path, text).expect("write");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    select_row(&mut h, "SHOP MILL");
    press(&mut h, "Edit machine");
    type_into(&mut h, "X travel max", "120");
    press(&mut h, "Save machine");
    assert_eq!(prompt(&h), Some(Prompt::Save(path.clone())), "Save over its own file asks first");
    assert_eq!(read_json(&path).pointer("/kinematics/default/parts/0/travel/max").and_then(|v| v.as_f64()), Some(100.0));
    press(&mut h, "Write file");
    let v = read_json(&path);
    assert_eq!(v.pointer("/kinematics/default/parts/0/travel/max").and_then(|v| v.as_f64()), Some(120.0));
    assert_eq!(v.pointer("/x_shop_notes/probe").and_then(|v| v.as_str()), Some("OMP40"), "unknown key kept");
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Row labels of the catalogue entries `keep` accepts.
fn rows_where(
    h: &Harness<'static, ShellUi>,
    keep: impl Fn(&camcraft_app::machine::library::MachineEntry) -> bool,
) -> Vec<String> {
    let l = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .expect("the catalogue was read");
    l.iter()
        .filter(|e| keep(e))
        .map(super::super::machines_pane::row_label)
        .collect()
}

fn present(h: &Harness<'static, ShellUi>, label: &str) -> bool {
    h.query_by_label(label).is_some()
}

fn new_blank_disabled(h: &Harness<'static, ShellUi>) -> bool {
    use egui_kittest::kittest::NodeT;
    h.get_by_label("New blank").accesskit_node().is_disabled()
}

/// Open the kind dropdown (a ComboBox showing `now`) and pick `to`,
/// with real pointer events like a user.
fn pick_kind(h: &mut Harness<'static, ShellUi>, now: &str, to: &str) {
    let at = h
        .get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(now))
        .rect()
        .center();
    super::machines_pane::click(h, at);
    h.run_steps(10);
    let at = h.get_by_label(to).rect().center();
    super::machines_pane::click(h, at);
    h.run_steps(3);
}

/// Issue #816: the kind dropdown starts on "All" (every row, New blank
/// shut), a kind filters the list and opens New blank, "All" restores.
#[test]
fn the_kind_dropdown_filters_the_list_and_gates_new_blank() {
    use camcraft_app::machine::Kind;
    let mut h = harness_with_user_dir(None);
    h.run();
    open_machines(&mut h);
    h.run_steps(3);
    let all = rows_where(&h, |_| true);
    let lathes = rows_where(&h, |e| e.kind == Some(Kind::Lathe));
    let others = rows_where(&h, |e| e.kind != Some(Kind::Lathe));
    assert!(
        !lathes.is_empty() && !others.is_empty(),
        "the library has lathes and non-lathes"
    );
    // "All" by default: every row, New blank disabled.
    assert!(h.state().state.machines_pane.kind_filter.is_none());
    for r in &all {
        h.get_by_label(r);
    }
    assert!(new_blank_disabled(&h), "New blank is shut under All");
    // Pick Lathe through the dropdown.
    pick_kind(&mut h, "All", "lathe");
    for r in &lathes {
        assert!(present(&h, r), "lathe row {r} stays");
    }
    for r in &others {
        assert!(!present(&h, r), "row {r} is filtered out");
    }
    assert!(
        !new_blank_disabled(&h),
        "New blank opens once a kind is picked"
    );
    // Back to All restores the full list and shuts New blank.
    pick_kind(&mut h, "lathe", "All");
    for r in &all {
        h.get_by_label(r);
    }
    assert!(new_blank_disabled(&h));
    assert_no_commands(&h);
}

/// Issue #869: a blank mill's seeded Flood is removed, an Air M7 added
/// with its code, and the saved file reads back with Air M7 alone - the
/// added coolant is never dropped. A coolant added without its code is
/// refused by name and adds nothing.
#[test]
fn a_blank_mill_swaps_its_flood_for_air_m7_and_keeps_it_through_save_and_reload() {
    let dir = user_dir("air");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    pick_kind(&mut h, "All", "mill");
    press(&mut h, "New blank");
    let list = "/kinematics/default/parts/0/parts/3/tool_station/coolants";
    assert_eq!(draft_at(&h, &format!("{list}/0/name")).and_then(|v| v.as_str().map(str::to_owned)).as_deref(), Some("Flood"));
    // A name alone is refused by name.
    type_into(&mut h, "New coolant", "Air");
    press(&mut h, "Add coolant");
    h.get_by_label("coolant Air: enter its M code (M7 air or mist, M8 flood) before adding it");
    assert_eq!(draft_at(&h, list).and_then(|v| v.as_array().map(Vec::len)), Some(1), "nothing added");
    type_into(&mut h, "New coolant code", "M7");
    press(&mut h, "Add coolant");
    press(&mut h, "Remove Coolant Flood");
    h.get_by_label("Validation: passes");
    press(&mut h, "Save machine");
    assert_eq!(prompt(&h), Some(Prompt::SaveAs));
    press(&mut h, "Write file");
    let path = dir.join("new-mill.mch");
    let text = std::fs::read_to_string(&path).expect("the machine was written");
    let m = camcraft_app::machine::parse_mch(&text).expect("the saved machine reads back");
    let coolants: Vec<(String, String)> = m.coolants.iter().map(|c| (c.name.clone(), c.m_code.clone())).collect();
    assert_eq!(coolants, [("Air".to_owned(), "M7".to_owned())], "{text}");
    assert_eq!(
        camcraft_app::operation::CoolantMode::machine_default(&m.coolants),
        camcraft_app::operation::CoolantMode::Air,
        "an Air-only machine starts its ops on Air"
    );
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}
