//! Pins for #3608: the window's Save over the project file that it opened or
//! last saved asks first when that file on disk changed after the open or
//! the last save, and writes nothing before the answer. Overwrite replaces the file,
//! Cancel keeps it, and Save As goes to the Save As dialog. The window's own
//! saves, a save to another file and a file deleted since the open never
//! ask. A held Save from the unsaved-changes prompt keeps the document and
//! the app open. A new document drops a held Save.
//!
//! Gate map (M4.4): covers ui::save_guard, ui::state, ui::unsaved.
//! Provenance: our own tests.

use std::path::{Path, PathBuf};

use camcraft_app::project;

use crate::file_pick::{PickFor, PickedFile};
use crate::save_guard::OverwriteChoice;
use crate::state::{CommandKind, State};
use crate::unsaved::UnsavedChoice;

/// What another program writes over the project: a different size, so the
/// stamp differs on every file system, whatever its clock resolution.
const FOREIGN: &str = "{\"written by\": \"another program\"}";

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-overwrite-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read the project file")
}

/// A change to the document (a new model, under a name not used before), so
/// that it has unsaved changes.
fn edit(state: &mut State) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    let cube = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/cube.stl");
    let bytes = std::fs::read(cube).expect("cube.stl");
    let name = format!("edit{}.stl", NEXT.fetch_add(1, Ordering::Relaxed));
    state.run(CommandKind::OpenPicked(PickedFile {
        purpose: PickFor::Model,
        name,
        bytes: Ok(bytes),
    }));
    assert!(state.history.is_dirty(), "{}", state.status);
}

/// A project file on disk, opened in a new window.
fn opened(path: &Path) -> State {
    project::save(&State::new().world, path).expect("a project on disk");
    let mut state = State::new();
    state.run(CommandKind::OpenProjectFile(path.to_path_buf()));
    assert_eq!(
        state.project_path.as_deref(),
        Some(path),
        "{}",
        state.status
    );
    state
}

/// As [`opened`], with an edit.
fn opened_and_edited(path: &Path) -> State {
    let mut state = opened(path);
    edit(&mut state);
    state
}

/// Run the queued commands (the prompt's answer), as the next frame does.
/// Never with a Save As queued: its dialog is a native window.
fn drain(state: &mut State) {
    while let Some(action) = state.pending.pop_front() {
        state.run(action);
    }
}

#[test]
fn a_save_over_a_changed_file_asks_and_overwrite_replaces_it() {
    let d = dir("overwrite");
    let path = d.join("job.camproj");
    let mut state = opened_and_edited(&path);
    std::fs::write(&path, FOREIGN).expect("another program writes the file");

    state.run(CommandKind::SaveProjectDialog);
    assert_eq!(
        state.save_guard.held.as_deref(),
        Some(path.as_path()),
        "{}",
        state.status
    );
    assert!(
        state
            .status
            .contains("the file on disk changed after this window opened or saved it"),
        "{}",
        state.status
    );
    assert_eq!(
        read(&path),
        FOREIGN,
        "nothing was written before the answer"
    );
    assert!(state.history.is_dirty(), "the changes are still unsaved");

    crate::actions::resolve_overwrite(&mut state, OverwriteChoice::Overwrite);
    drain(&mut state);
    assert!(state.save_guard.held.is_none(), "{}", state.status);
    assert_eq!(read(&path), project::save_text(&state.world).expect("text"));
    assert!(!state.history.is_dirty(), "{}", state.status);

    // Our own save is the new stamp: the next Save does not ask.
    edit(&mut state);
    state.run(CommandKind::SaveProjectDialog);
    assert!(state.save_guard.held.is_none(), "{}", state.status);
    assert!(!state.history.is_dirty(), "{}", state.status);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn cancel_keeps_the_file_and_save_as_goes_to_the_save_as_dialog() {
    let d = dir("cancel");
    let path = d.join("job.camproj");
    let mut state = opened_and_edited(&path);
    std::fs::write(&path, FOREIGN).expect("another program writes the file");

    state.run(CommandKind::SaveProjectDialog);
    crate::actions::resolve_overwrite(&mut state, OverwriteChoice::Cancel);
    assert!(state.save_guard.held.is_none());
    assert!(state.pending.is_empty(), "Cancel queues nothing");
    assert!(
        state.status.starts_with("save cancelled"),
        "{}",
        state.status
    );
    assert_eq!(read(&path), FOREIGN);
    assert!(state.history.is_dirty());

    // The file is still changed, so the next Save asks again.
    state.run(CommandKind::SaveProjectDialog);
    assert_eq!(
        state.save_guard.held.as_deref(),
        Some(path.as_path()),
        "{}",
        state.status
    );
    crate::actions::resolve_overwrite(&mut state, OverwriteChoice::SaveAs);
    assert!(state.save_guard.held.is_none());
    assert_eq!(state.pending.len(), 1, "{:?}", state.pending);
    assert!(
        matches!(
            state.pending.front(),
            Some(CommandKind::SaveProjectAsDialog)
        ),
        "{:?}",
        state.pending
    );
    assert_eq!(read(&path), FOREIGN);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_window_s_own_saves_never_ask() {
    let d = dir("own");
    let path = d.join("job.camproj");
    let mut state = State::new();
    edit(&mut state);
    // What Save As runs with the picked path: a new file.
    state.run(CommandKind::SaveProjectFile(path.clone()));
    assert!(!state.history.is_dirty(), "{}", state.status);
    // Two saves with no edit between them, then one after an edit.
    for _ in 0..2 {
        state.run(CommandKind::SaveProjectDialog);
        assert!(state.save_guard.held.is_none(), "{}", state.status);
    }
    edit(&mut state);
    state.run(CommandKind::SaveProjectDialog);
    assert!(state.save_guard.held.is_none(), "{}", state.status);
    assert!(!state.history.is_dirty(), "{}", state.status);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_save_to_another_file_or_over_a_deleted_file_does_not_ask() {
    let d = dir("other");
    let path = d.join("job.camproj");
    let copy = d.join("copy.camproj");
    let mut state = opened_and_edited(&path);
    std::fs::write(&path, FOREIGN).expect("another program writes the file");
    state.run(CommandKind::SaveProjectFile(copy.clone()));
    assert!(state.save_guard.held.is_none(), "{}", state.status);
    assert_eq!(state.project_path.as_deref(), Some(copy.as_path()));
    assert_eq!(read(&path), FOREIGN, "the changed file is kept");

    edit(&mut state);
    std::fs::remove_file(&copy).expect("another program deletes the file");
    state.run(CommandKind::SaveProjectDialog);
    assert!(state.save_guard.held.is_none(), "{}", state.status);
    assert!(copy.is_file(), "the save puts the file back");
    assert!(!state.history.is_dirty(), "{}", state.status);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_held_save_from_the_unsaved_prompt_keeps_the_document_open() {
    let d = dir("quit");
    let path = d.join("job.camproj");
    let mut state = opened_and_edited(&path);
    std::fs::write(&path, FOREIGN).expect("another program writes the file");

    state.run(CommandKind::Quit);
    assert!(state.unsaved_prompt.is_some(), "{}", state.status);
    state.run(CommandKind::ResolveUnsaved(UnsavedChoice::Save));
    assert!(!state.request_quit, "the app stays open");
    assert!(state.unsaved_prompt.is_none());
    assert_eq!(
        state.save_guard.held.as_deref(),
        Some(path.as_path()),
        "{}",
        state.status
    );
    assert!(
        state.status.ends_with("the document stays open"),
        "{}",
        state.status
    );
    assert_eq!(read(&path), FOREIGN, "nothing was written");
    assert!(state.history.is_dirty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_new_document_drops_a_held_save() {
    let d = dir("new");
    let path = d.join("job.camproj");
    // A clean document: Save still writes, and New needs no unsaved prompt.
    let mut state = opened(&path);
    std::fs::write(&path, FOREIGN).expect("another program writes the file");
    state.run(CommandKind::SaveProjectDialog);
    assert_eq!(
        state.save_guard.held.as_deref(),
        Some(path.as_path()),
        "{}",
        state.status
    );

    // Ctrl+N while the prompt is up: the blank document never goes to the old file.
    state.run(CommandKind::NewDocument);
    assert!(state.save_guard.held.is_none(), "{}", state.status);
    crate::actions::resolve_overwrite(&mut state, OverwriteChoice::Overwrite);
    drain(&mut state);
    assert_eq!(read(&path), FOREIGN, "nothing was written");
    let _ = std::fs::remove_dir_all(&d);
}
