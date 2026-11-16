//! #1256: Escape closes the topmost modal dialog without applying its draft
//! (the same path as its Cancel), one dialog per press. The fixture library
//! dialogs and the unsaved-changes prompt are egui `Modal`s; egui consumes
//! Escape for the topmost one. Driven by real key events in a kittest
//! harness; the harness drains the command queue at the start of each frame,
//! as `shell_web` does, so a Cancel queued by Escape lands on the next frame.
//! No pixels (the snapshot lane is release).
//! Provenance: our own tests over our own shell; no vendor material.

use std::path::PathBuf;

use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::Harness;

use crate::state::{CommandKind, FixtureLibDialog, State};

use super::fixture_library_dialog;
use super::unsaved_prompt;

/// Press and release Escape with no modifiers, as the shell receives it.
fn press_escape(h: &mut Harness<'static, State>) {
    h.input_mut().events.push(Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
}

/// The two dialogs in the order `shell` draws them: the unsaved prompt last,
/// so it is the topmost modal.
fn harness(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(900.0, 900.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            fixture_library_dialog::window(ui.ctx(), st);
            unsaved_prompt::window(ui.ctx(), st);
        },
        state,
    )
}

/// A state whose fixture library lives in its own temp folder, so "no
/// library file" is a real check.
fn fresh_state(tag: &str) -> State {
    let mut state = State::new();
    let dir = std::env::temp_dir().join(format!("camcraft-ui-escape-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    state.fixture_lib_dir = Some(dir);
    state
}

fn library_file_written(state: &State) -> bool {
    state.fixture_library_path().is_ok_and(|p| p.exists())
}

/// Put a pending, unapplied edit in the open fixture Save draft.
const PENDING_NOTE: &str = "pending edit, must not apply";

fn set_draft_note(h: &mut Harness<'static, State>) {
    if let Some(FixtureLibDialog::Save(draft)) = h.state_mut().fixture_lib.as_mut() {
        draft.note = PENDING_NOTE.to_string();
    }
}

fn draft_note(state: &State) -> Option<&str> {
    match state.fixture_lib.as_ref() {
        Some(FixtureLibDialog::Save(draft)) => Some(draft.note.as_str()),
        _ => None,
    }
}

#[test]
fn escape_closes_only_the_topmost_dialog_per_press() {
    let mut state = fresh_state("topmost");
    state.pending.push_back(CommandKind::FixtureLibrarySaveDialog(String::new()));
    state.drain_actions();
    state.unsaved_prompt = Some(CommandKind::NewDocument);
    let open_doc = PathBuf::from("open-document.camcraft");
    state.project_path = Some(open_doc.clone());

    let mut h = harness(state);
    h.run_steps(2);
    set_draft_note(&mut h);
    press_escape(&mut h);
    assert!(h.state().unsaved_prompt.is_none(), "the topmost prompt closes");
    assert_eq!(h.state().status, "cancelled - the document stays open");
    assert_eq!(h.state().project_path.as_deref(), Some(open_doc.as_path()), "the same document is still open");
    assert_eq!(draft_note(h.state()), Some(PENDING_NOTE), "the fixture draft is unchanged by the first press");
    assert!(matches!(h.state().fixture_lib, Some(FixtureLibDialog::Save(_))), "one press closes one dialog");

    press_escape(&mut h);
    assert!(h.state().fixture_lib.is_none(), "the second press closes the dialog below");
    assert!(!library_file_written(h.state()), "Escape applies nothing: no library file");
}

#[test]
fn escape_closes_the_fixture_dialog_without_applying() {
    let mut state = fresh_state("fixture");
    state.pending.push_back(CommandKind::FixtureLibrarySaveDialog(String::new()));
    state.drain_actions();
    let mut h = harness(state);
    h.run_steps(2);
    set_draft_note(&mut h);
    assert_eq!(draft_note(h.state()), Some(PENDING_NOTE), "the edit is in the draft before Escape");
    press_escape(&mut h);

    assert!(h.state().fixture_lib.is_none(), "Escape closes the dialog and its draft with it");
    assert!(!library_file_written(h.state()), "Escape applies nothing: no library file");
    assert_eq!(h.state().status, "ready");
}

#[test]
fn escape_with_nothing_open_does_nothing() {
    let mut h = harness(fresh_state("none"));
    h.run_steps(2);
    press_escape(&mut h);

    assert!(h.state().fixture_lib.is_none());
    assert!(h.state().unsaved_prompt.is_none());
    assert!(!library_file_written(h.state()));
    assert_eq!(h.state().status, "ready");
}
