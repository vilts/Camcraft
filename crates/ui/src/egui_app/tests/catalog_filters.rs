//! PR #2029: catalog filters are temporary view state and compose together.
//! Provenance: our own tests of the requested catalog controls.
//! Gate map: covers ui::egui_app::tool_library_dialog, ui::state.

use egui::accesskit::Role;
use super::*;

fn catalog() -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1600.0, 1600.0))
        .build_ui_state(frame, ShellUi::blank());
    h.run_steps(2);
    h.get_by_label("View").click();
    h.run_steps(2);
    h.get_by_label("Tool library…").click();
    h.run_steps(3);
    h
}

fn choose_type(h: &mut Harness<'static, ShellUi>, name: &str) {
    h.get_by_role_and_label(Role::ComboBox, "Tool type").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, name).click();
    h.run_steps(3);
}

fn search(h: &mut Harness<'static, ShellUi>, name: &str) {
    h.get_by_role_and_label(Role::TextInput, "Name filter").click();
    h.run();
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: Key::A, physical_key: None, pressed: true, repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run();
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::NONE));
    h.get_by_role_and_label(Role::TextInput, "Name filter").type_text(name);
    h.run_steps(3);
}

fn has(h: &Harness<'static, ShellUi>, name: &str) -> bool {
    h.query_by_label_contains(&format!("{name}  Ø")).is_some()
}

#[test]
fn catalog_type_filter_and_all_types_reset() {
    let mut h = catalog();
    assert!(has(&h, "EM-0600-4FL") && has(&h, "BN-0600"));
    choose_type(&mut h, "ball nose");
    assert!(has(&h, "BN-0600") && has(&h, "BN-1000"));
    assert!(!has(&h, "EM-0600-4FL") && !has(&h, "DR-0600"));
    choose_type(&mut h, "All types");
    assert!(has(&h, "EM-0600-4FL") && has(&h, "DR-0600"));
}

#[test]
fn catalog_name_filter_ignores_case_and_matches_substrings() {
    let mut h = catalog();
    search(&mut h, "bN-06");
    assert!(has(&h, "BN-0600"));
    assert!(!has(&h, "BN-1000") && !has(&h, "EM-0600-4FL"));
}

#[test]
fn catalog_filters_apply_together_without_changing_the_document_or_history() {
    let mut h = catalog();
    let before = camcraft_app::project::save_text(&h.state().state.world).unwrap();
    search(&mut h, "0600");
    assert!(has(&h, "BN-0600") && has(&h, "EM-0600-4FL"));
    choose_type(&mut h, "ball nose");
    assert!(has(&h, "BN-0600"));
    assert!(!has(&h, "BN-1000") && !has(&h, "EM-0600-4FL"));
    assert_eq!(camcraft_app::project::save_text(&h.state().state.world).unwrap(), before);
    assert!(h.state().state.pending.is_empty());
    assert!(h.state().state.history.is_empty());
    assert!(!h.state().state.history.is_dirty());
}

#[test]
fn catalog_no_match_shows_message_and_preserves_the_catalog() {
    let mut h = catalog();
    search(&mut h, "no such tool");
    assert!(h.query_by_label("No tools match the filters.").is_some());
    assert!(h.query_by_label("Add").is_none());
    search(&mut h, "0600");
    assert!(h.query_by_label("No tools match the filters.").is_none());
    assert!(has(&h, "BN-0600") && has(&h, "EM-0600-4FL"));
}
