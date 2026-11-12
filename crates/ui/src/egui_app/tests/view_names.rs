//! #645: the "Show origin names" switch the review asked for on top of
//! #727. The browser pane's checkbox flips the flag through the State
//! method that takes the user's choice out to the per-user view file at
//! once; the flip is view state like the camera - no command on the
//! queue, nothing on the undo history. The choice survives a restart;
//! a corrupt file starts with names ON, the #727 default, and says so.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (M4.4): covers ui::egui_app::browser_pane (the name switch),
//! ui::state (state_view), app::view through the State handoff.

use std::path::PathBuf;

use camcraft_app::view::{self, ViewPrefs, FILE_NAME};

use super::*;

/// A fresh folder for one test, canonical (macOS temp is a symlink).
fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-view-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    std::fs::canonicalize(&d).expect("canonical folder")
}

#[test]
fn the_names_checkbox_flips_view_state_not_a_command_and_persists() {
    let file = temp_dir("toggle").join(FILE_NAME);
    let mut h = shell_harness();
    h.state_mut().state.view_store = Some(file.clone());
    assert!(h.state().state.show_gizmo_names, "on by default, the #727 decision");
    h.get_by_label("Show origin names").click();
    h.run();
    assert!(!h.state().state.show_gizmo_names, "the checkbox flips the flag");
    assert!(h.state().state.pending.is_empty(), "the flip is view state, never a command");
    let (prefs, note) = view::load(&file);
    assert!(!prefs.show_gizmo_names, "the choice goes out to the file at once");
    assert_eq!(note, None, "and a good file says nothing");
    h.get_by_label("Show origin names").click();
    h.run();
    assert!(h.state().state.show_gizmo_names, "off again flips back on");
    assert!(view::load(&file).0.show_gizmo_names, "and the file follows");
}

#[test]
fn the_choice_survives_a_restart_and_a_corrupt_file_starts_with_names_on() {
    let d = temp_dir("restart");
    let file = d.join(FILE_NAME);
    view::save(&file, &ViewPrefs { show_gizmo_names: false }).expect("write an off choice");
    let mut second = State::new();
    second.load_view_from(&file);
    assert!(!second.show_gizmo_names, "the next run starts the way the last left it");
    std::fs::write(&file, "{ not json").expect("corrupt the file");
    let mut third = State::new();
    third.load_view_from(&file);
    assert!(third.show_gizmo_names, "a corrupt file is the #727 default: names on");
    assert!(third.status.contains("defaults"), "and says so: {}", third.status);
}
