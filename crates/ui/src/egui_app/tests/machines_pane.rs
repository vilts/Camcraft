//! The Machines pane's frame tests (issue #287): the View door opens it
//! beside the properties pane, it lists the shipped library, selecting a
//! machine shows its properties and its validation - a clean pass, or
//! each refusal by name - and none of it touches the command queue (the
//! pane is view state) except "Use for this setup" (#384), which queues
//! the undoable machine assignment and is disabled, with its reason
//! shown, for a refused machine.
//! Provenance: our own tests, specced from the issue and the egui_dock /
//! egui_kittest vendored sources; no HSMWorks material involved.
//!
//! Gate map (M4.4): covers ui::egui_app::machines_pane, ui::egui_app::layout, ui::state, app::machine::library, app::machine::validate, app::commands::load_machine, ui::state_setup_mount.

use super::super::layout;
use super::super::machines_pane::{row_label, verdict, USE_FOR_SETUP};
use super::*;

#[path = "machines_pane/assign.rs"]
mod assign;

fn blank_harness() -> Harness<'static, ShellUi> {
    harness_with_user_dir(None)
}

/// A blank shell whose Machines pane lists `dir` as the user machines
/// folder (`None`: no user folder) - never the tester's real one.
pub(super) fn harness_with_user_dir(dir: Option<std::path::PathBuf>) -> Harness<'static, ShellUi> {
    let mut sh = ShellUi::blank();
    sh.state.machines_pane.user_dir = Some(dir);
    Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, sh)
}

pub(super) fn click(h: &mut Harness<'static, ShellUi>, at: egui::Pos2) {
    set_pos(h, at);
    button_at(h, egui::PointerButton::Primary, at, true);
    button_at(h, egui::PointerButton::Primary, at, false);
    h.run_steps(2);
}

/// Open the Machines pane the way a user does: the ribbon's View door,
/// then its "Machines" row.
pub(super) fn open_machines(h: &mut Harness<'static, ShellUi>) {
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Panels").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Machines").click();
    h.run_steps(2);
}

pub(super) fn assert_no_commands(h: &Harness<'static, ShellUi>) {
    let st = &h.state().state;
    assert!(st.pending.is_empty(), "the pane queued a command: {:?}", st.pending);
    assert!(st.history.is_empty(), "the pane landed in the history");
}

#[test]
fn the_view_door_opens_the_machines_pane_beside_properties() {
    let mut h = blank_harness();
    h.run();
    assert!(!layout::is_open(&h.state().dock, &Tab::Machines), "closed by default");
    open_machines(&mut h);
    let dock = &h.state().dock;
    let m = dock.find_tab(&Tab::Machines).expect("the pane opened");
    let p = dock.find_tab(&Tab::Properties).expect("properties still open");
    assert_eq!(m.node_path(), p.node_path(), "a tab in the properties pane's leaf");
    assert!(h.state().state.panels_open.machines);
    // The pane read the catalogue: the whole shipped library, in order.
    let names: Vec<String> = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .expect("the catalogue was read")
        .iter()
        .map(|e| e.name.clone())
        .collect();
    let want: Vec<String> = camcraft_app::posting::MACHINES.iter().map(|(n, _)| (*n).to_owned()).collect();
    assert_eq!(names, want);
    // The same door closes it.
    open_machines(&mut h);
    assert!(!layout::is_open(&h.state().dock, &Tab::Machines));
    assert_no_commands(&h);
}

#[test]
fn selecting_a_shipped_machine_shows_its_sheet_and_a_clean_validation() {
    let mut h = blank_harness();
    h.run();
    open_machines(&mut h);
    let lathe = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .and_then(|l| l.iter().find(|e| e.name == "Lathe").cloned())
        .expect("the library lists the lathe");
    h.get_by_label(&row_label(&lathe)).click();
    h.run_steps(2);
    let sel = h.state().state.machines_pane.selected;
    let picked = h.state().state.machines_pane.list.as_ref().and_then(|l| sel.and_then(|i| l.get(i)));
    assert_eq!(picked.map(|e| e.name.as_str()), Some("Lathe"));
    h.get_by_label("Validation: passes");
    h.get_by_label("Axes");
    h.get_by_label("Spindle");
    assert_no_commands(&h);
}

/// #786: the Spindle group states rigid tapping in words - the 770M's
/// "no" says what it does to a program.
#[test]
fn the_sheet_states_rigid_tapping_in_words() {
    let mut h = blank_harness();
    h.run();
    open_machines(&mut h);
    let m = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .and_then(|l| l.iter().find(|e| e.name == "Tormach 770M").cloned())
        .expect("the library lists the 770M");
    // #3233: the list can push the row past the pane's clip edge, where a
    // click lands on nothing. Scroll it into view first.
    assert!(h.query_by_label(&row_label(&m)).is_some(), "the pane shows the 770M row");
    h.get_by_label(&row_label(&m)).scroll_to_me();
    h.run();
    h.get_by_label(&row_label(&m)).click();
    h.run_steps(2);
    h.get_by_label("Rigid tapping");
    h.get_by_label("no - tap cycles are refused");
    assert_no_commands(&h);
}

#[test]
fn a_refused_machine_file_shows_each_refusal_by_name() {
    let text = r#"{
      "general": { "model": "BACKWARDS MILL", "kind": "mill" },
      "kinematics": { "default": { "units": { "length": "mm" }, "parts": [
        { "name": "X", "type": "linear", "travel": { "min": 10.0, "max": 5.0 } },
        { "name": "Y", "type": "linear" }, { "name": "Z", "type": "linear" }
      ] } }
    }"#;
    let path = std::env::temp_dir().join(format!("camcraft_ui_machines_pane_{}.mch", std::process::id()));
    std::fs::write(&path, text).expect("write the fixture file");
    let mut h = blank_harness();
    let m = camcraft_app::machine::parse_mch(text).expect("the reader takes it (it drops the travel)");
    camcraft_app::machine::set(&h.state().state.world, &path.display().to_string(), Some(&m));
    h.run();
    open_machines(&mut h);
    let bad = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .and_then(|l| l.last().cloned())
        .expect("the document's file is listed last");
    assert_eq!(bad.name, "BACKWARDS MILL");
    let label = row_label(&bad);
    // The document's row is last, below the list's fold: scroll it in,
    // let the scroll settle, then press it with the real pointer.
    h.get_by_label(&label).scroll_to_me();
    h.run_steps(10);
    let at = h.get_by_label(&label).rect().center();
    click(&mut h, at);
    assert_eq!(verdict(&bad), "Validation: 1 refusal");
    h.get_by_label("Validation: 1 refusal");
    h.get_by_label("X travel: min 10 must be below max 5");
    assert_no_commands(&h);
    let _ = std::fs::remove_file(&path);
}

fn ctrl_z(h: &mut Harness<'static, ShellUi>) {
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
}

fn button_disabled(h: &Harness<'static, ShellUi>) -> bool {
    use egui_kittest::kittest::NodeT;
    h.get_by_label(USE_FOR_SETUP).accesskit_node().is_disabled()
}

#[test]
fn use_for_this_setup_assigns_the_selected_machine_and_undo_restores_it() {
    let mut h = blank_harness();
    h.run();
    let before = camcraft_app::machine::of(&h.state().state.world);
    let lathe_path = camcraft_app::posting::builtin_path("Lathe");
    assert_ne!(before.0, lathe_path, "the fixture must start on another machine");
    open_machines(&mut h);
    let lathe = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .and_then(|l| l.iter().find(|e| e.name == "Lathe").cloned())
        .expect("the library lists the lathe");
    h.get_by_label(&row_label(&lathe)).click();
    h.run_steps(2);
    assert!(!button_disabled(&h), "a valid, unmounted machine can be assigned");
    h.get_by_label(USE_FOR_SETUP).click();
    h.run_steps(3);
    let st = &h.state().state;
    assert!(st.pending.is_empty(), "the queued command was drained: {:?}", st.pending);
    assert_eq!(camcraft_app::machine::of(&st.world).0, lathe_path, "status: {}", st.status);
    assert_eq!(camcraft_app::machine::kind_of(&st.world), camcraft_app::machine::Kind::Lathe);
    assert!(!st.history.is_empty(), "the assignment is in the history");
    // Now it is the setup's machine: the button says so and stays shut.
    assert!(button_disabled(&h), "the mounted machine cannot be assigned again");
    h.get_by_label("Cannot assign: it is already this setup's machine");
    // Ctrl+Z puts the previous machine back exactly.
    ctrl_z(&mut h);
    assert_eq!(camcraft_app::machine::of(&h.state().state.world), before, "undo restores the machine");
}

#[test]
fn a_refused_machine_cannot_be_assigned_and_names_why() {
    let text = r#"{
      "general": { "model": "BACKWARDS MILL", "kind": "mill" },
      "kinematics": { "default": { "units": { "length": "mm" }, "parts": [
        { "name": "X", "type": "linear", "travel": { "min": 10.0, "max": 5.0 } },
        { "name": "Y", "type": "linear" }, { "name": "Z", "type": "linear" }
      ] } }
    }"#;
    let path = std::env::temp_dir().join(format!("camcraft_ui_machines_assign_{}.mch", std::process::id()));
    std::fs::write(&path, text).expect("write the fixture file");
    let mut h = blank_harness();
    h.run();
    let before = camcraft_app::machine::of(&h.state().state.world);
    open_machines(&mut h);
    // The refused file is in the catalogue but NOT mounted, so the only
    // thing keeping the button shut is its validation.
    let bad = camcraft_app::machine::library::MachineEntry::from_file(&path);
    assert!(!bad.is_valid());
    {
        let p = &mut h.state_mut().state.machines_pane;
        let list = p.list.as_mut().expect("the catalogue was read");
        list.push(bad.clone());
        p.selected = Some(list.len() - 1);
    }
    h.run_steps(2);
    assert!(button_disabled(&h), "a refused machine's button is disabled");
    h.get_by_label("Cannot assign: fails validation - X travel: min 10 must be below max 5");
    // Pressing the disabled button does nothing.
    let at = h.get_by_label(USE_FOR_SETUP).rect().center();
    click(&mut h, at);
    assert_no_commands(&h);
    assert_eq!(camcraft_app::machine::of(&h.state().state.world), before);
    // Even queued directly, the command door refuses it by name.
    h.state_mut()
        .state
        .pending
        .push_back(CommandKind::UseCatalogueMachine(bad.source.clone()));
    h.run_steps(2);
    let st = &h.state().state;
    assert!(st.history.is_empty(), "a refused machine never lands in the history");
    assert!(st.status.contains("X travel: min 10 must be below max 5"), "status: {}", st.status);
    assert_eq!(camcraft_app::machine::of(&st.world), before);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn use_for_this_setup_replaces_an_own_machine_and_leaves_the_document_alone() {
    let mut h = blank_harness();
    h.run();
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    }
    h.run_steps(2);
    let s = camcraft_app::setup::active(&h.state().state.world).expect("a setup is active");
    h.state_mut().state.pending.push_back(CommandKind::SetupOwnMachine { setup: s.clone(), own: true });
    h.run_steps(2);
    let world = &h.state().state.world;
    assert!(camcraft_app::setup_mount::own_machine(world, Some(&s)).is_some(), "the setup owns its machine");
    let doc_before = camcraft_app::machine::document_of(world);
    let own_before = camcraft_app::machine::of(world);
    let lathe_path = camcraft_app::posting::builtin_path("Lathe");
    assert_ne!(own_before.0, lathe_path);
    open_machines(&mut h);
    let lathe = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .and_then(|l| l.iter().find(|e| e.name == "Lathe").cloned())
        .expect("the library lists the lathe");
    h.get_by_label(&row_label(&lathe)).click();
    h.run_steps(2);
    h.get_by_label(USE_FOR_SETUP).click();
    h.run_steps(3);
    let st = &h.state().state;
    assert_eq!(camcraft_app::machine::of(&st.world).0, lathe_path, "status: {}", st.status);
    let own = camcraft_app::setup_mount::own_machine(&st.world, Some(&s)).map(|d| d.path);
    assert_eq!(own.as_deref(), Some(lathe_path.as_str()), "the setup's own machine took the pick");
    assert_eq!(camcraft_app::machine::document_of(&st.world), doc_before, "the document's machine is untouched");
    assert!(button_disabled(&h));
    ctrl_z(&mut h);
    let world = &h.state().state.world;
    assert_eq!(camcraft_app::machine::of(world), own_before, "undo restores the setup's machine");
    assert_eq!(camcraft_app::machine::document_of(world), doc_before);
}

/// #1119: the Machines pane's kind filter combo carries an accessible name.
#[test]
fn the_machines_pane_names_every_input() {
    let mut h = blank_harness();
    h.run();
    open_machines(&mut h);
    let (bad, seen) = super::super::fixture_plate_tests::unnamed_inputs(&h);
    assert!(seen >= 1, "only {seen} inputs found in the machines pane");
    assert!(bad.is_empty(), "unnamed inputs in machines pane: {bad:?}");
}

/// #2030: every machine kind has a filter entry, including both EDM kinds.
#[test]
fn the_kind_filter_offers_every_machine_kind_and_filters_its_rows() {
    use camcraft_app::machine::Kind;
    use egui::accesskit::Role;
    let kinds = [
        Kind::Mill, Kind::Lathe, Kind::MillTurn, Kind::Plasma,
        Kind::Waterjet, Kind::WireEdm, Kind::SinkerEdm,
    ];
    let mut h = blank_harness();
    h.run();
    open_machines(&mut h);
    h.run_steps(3);
    let rows: Vec<_> = h.state().state.machines_pane.list.as_ref()
        .expect("the catalogue was read").iter()
        .map(|e| (e.kind, row_label(e))).collect();
    let mut current = "All";
    for kind in kinds {
        let at = h.get_by(|n| n.role() == Role::ComboBox
            && n.value().as_deref() == Some(current)).rect().center();
        click(&mut h, at);
        h.run_steps(10);
        for entry in kinds {
            h.get_by_label(entry.word());
        }
        h.get_by_label(kind.word()).click();
        h.run_steps(3);
        for (row_kind, label) in &rows {
            assert_eq!(h.query_by_label(label).is_some(), *row_kind == Some(kind),
                "{label} under the {} filter", kind.word());
        }
        current = kind.word();
    }
    assert_no_commands(&h);
}
