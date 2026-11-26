//! Provenance: structural regressions from issue #3776 and our own egui harness.
//! Gate map (M4.4): covers ui::egui_app::machines_pane.

use super::*;

fn lathe(h: &Harness<'static, ShellUi>) -> camcraft_app::machine::library::MachineEntry {
    h.state().state.machines_pane.list.as_ref()
        .and_then(|list| list.iter().find(|entry| entry.name == "Lathe").cloned())
        .expect("the catalogue lists Lathe")
}

fn double_click(h: &mut Harness<'static, ShellUi>, label: &str) {
    // Age the preceding menu/selection click so egui does not count a triple-click.
    h.run_steps(40);
    let at = h.get_by_label(label).rect().center();
    h.input_mut().events.push(Event::PointerMoved(at));
    h.run_steps(1);
    for pressed in [true, false, true, false] {
        h.input_mut().events.push(Event::PointerButton {
            pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    assert!(h.ctx.input(|input| input.pointer.button_double_clicked(egui::PointerButton::Primary)),
        "the pointer stream contains a double-click");
    h.run_steps(3);
}

fn gesture_harness() -> Harness<'static, ShellUi> {
    let mut sh = ShellUi::blank();
    sh.state.machines_pane.user_dir = Some(None);
    Harness::builder().with_size(Vec2::new(1280.0, 720.0))
        .with_step_dt(1.0 / 60.0).build_ui_state(frame, sh)
}

#[test]
fn selected_machine_names_the_unapplied_selection() {
    let mut h = blank_harness();
    h.run();
    open_machines(&mut h);
    let entry = lathe(&h);
    h.get_by_label(&row_label(&entry)).click();
    h.run_steps(2);
    h.get_by_label("Selected: Lathe - not yet this setup's machine");
    assert_no_commands(&h);
    h.get_by_label(USE_FOR_SETUP).click();
    h.run_steps(3);
    assert!(h.query_by_label("Selected: Lathe - not yet this setup's machine").is_none());
}

#[test]
fn assign_button_works_after_details_scroll_to_bottom_and_undo_restores() {
    let mut h = blank_harness();
    h.run();
    let before = camcraft_app::machine::of(&h.state().state.world);
    open_machines(&mut h);
    let entry = lathe(&h);
    h.get_by_label(&row_label(&entry)).click();
    h.run_steps(2);
    let detail = h.get_by_label("Validation: passes").rect().center();
    h.hover_at(detail);
    h.run_steps(2);
    h.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -10_000.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(30);
    // Real pointer input cannot activate a button clipped by the detail scroll area.
    let at = h.get_by_label(USE_FOR_SETUP).rect().center();
    click(&mut h, at);
    assert_eq!(camcraft_app::machine::of(&h.state().state.world).0, entry.source.mount_path());
    assert!(button_disabled(&h));
    ctrl_z(&mut h);
    assert_eq!(camcraft_app::machine::of(&h.state().state.world), before);
}

#[test]
fn double_click_assigns_the_clicked_machine_and_undo_restores() {
    let mut h = gesture_harness();
    h.run();
    let before = camcraft_app::machine::of(&h.state().state.world);
    open_machines(&mut h);
    let entry = lathe(&h);
    assert!(h.state().state.machines_pane.selected.is_none());
    double_click(&mut h, &row_label(&entry));
    assert_eq!(camcraft_app::machine::of(&h.state().state.world).0, entry.source.mount_path());
    assert_eq!(h.state().state.history.done_len(), 1, "one undoable assignment");
    ctrl_z(&mut h);
    assert_eq!(camcraft_app::machine::of(&h.state().state.world), before);
    assert_eq!(h.state().state.history.done_len(), 0, "one undo restores the initial history");
}

#[test]
fn double_click_uses_the_clicked_rows_guard_and_skips_the_mounted_machine() {
    let mut h = gesture_harness();
    h.state_mut().state.pending.push_back(CommandKind::UseCatalogueMachine(
        camcraft_app::machine::library::MachineSource::Builtin("Lathe")));
    h.run();
    h.state_mut().state.history.clear();
    open_machines(&mut h);
    let mounted = lathe(&h);
    let before_status = h.state().state.status.clone();
    double_click(&mut h, &row_label(&mounted));
    assert_no_commands(&h);
    assert_eq!(h.state().state.status, before_status, "already mounted does not queue an assignment");
    h.get_by_label("Cannot assign: it is already this setup's machine");
    let entry = h.state().state.machines_pane.list.as_ref().expect("catalogue")
        .iter().find(|entry| entry.name != "Lathe" && entry.is_valid()).cloned()
        .expect("another valid machine");
    h.get_by_label(&row_label(&entry)).scroll_to_me();
    h.run_steps(10);
    double_click(&mut h, &row_label(&entry));
    assert_eq!(camcraft_app::machine::of(&h.state().state.world).0, entry.source.mount_path());
    ctrl_z(&mut h);
    assert_eq!(camcraft_app::machine::of(&h.state().state.world).0, mounted.source.mount_path());
}

#[test]
fn double_click_refused_machine_does_not_queue_an_assignment() {
    let path = std::env::temp_dir().join(format!("camcraft_3776_refused_{}.mch", std::process::id()));
    std::fs::write(&path, "{}").expect("write refused machine");
    let mut h = gesture_harness();
    h.run();
    let before = camcraft_app::machine::of(&h.state().state.world);
    open_machines(&mut h);
    let valid = lathe(&h);
    h.get_by_label(&row_label(&valid)).click();
    h.run_steps(2);
    let bad = camcraft_app::machine::library::MachineEntry::from_file(&path);
    let reason = bad.assign_block(&h.state().state.world).expect("refused machine");
    h.state_mut().state.machines_pane.list.as_mut().expect("catalogue").push(bad.clone());
    h.run_steps(2);
    h.get_by_label(&row_label(&bad)).scroll_to_me();
    h.run_steps(10);
    let status = h.state().state.status.clone();
    double_click(&mut h, &row_label(&bad));
    assert_no_commands(&h);
    assert_eq!(h.state().state.status, status, "a refused row never queues an assignment");
    assert_eq!(camcraft_app::machine::of(&h.state().state.world), before);
    assert!(button_disabled(&h));
    h.get_by_label(&format!("Cannot assign: {reason}"));
    std::fs::remove_file(path).expect("remove refused machine");
}
