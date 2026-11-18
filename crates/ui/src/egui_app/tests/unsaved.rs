//! #107's frame tests: a Quit (the window X and Ctrl+Q both queue one)
//! or a New over unsaved changes raises the Save / Don't Save / Cancel
//! prompt instead of discarding the document, and each answer does what
//! it says - Cancel and a failed save keep the app open. #144 adds the
//! OS quit (macOS Cmd+Q / app-menu Quit) routed through the same guard.
//! Provenance: our own tests, specced from the egui_kittest vendored
//! sources; no HSMWorks material involved.

use super::*;
use crate::unsaved::UnsavedChoice;

fn temp_project(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-unsaved-{tag}-{}.camproj", std::process::id()))
}

fn quit(h: &mut Harness<'static, ShellUi>) {
    h.state_mut().state.pending.push_back(CommandKind::Quit);
    h.run();
}

#[test]
fn close_over_unsaved_changes_prompts_and_does_not_exit() {
    let mut harness = shell_harness();
    assert!(harness.state().state.history.is_dirty(), "the seed is unsaved work");
    assert_eq!(harness.state().state.window_title(), "*CamCraft - Untitled");

    quit(&mut harness);
    assert!(!harness.state().state.request_quit, "a dirty close must not exit");
    assert!(harness.state().state.unsaved_prompt.is_some(), "the close is held");
    harness.get_by_label("Unsaved changes");

    // Cancel: the prompt closes, the document and the app stay.
    harness.get_by_label("Cancel").click();
    harness.run();
    let st = &harness.state().state;
    assert!(st.unsaved_prompt.is_none() && !st.request_quit, "Cancel keeps the app open");
    assert!(model::find_by_name(&st.world, "Contour1").is_some(), "Cancel kept the document");
    assert!(harness.query_by_label("Unsaved changes").is_none(), "the prompt closed");

    // Don't Save: the held close goes through.
    quit(&mut harness);
    harness.get_by_label("Don't Save").click();
    harness.run();
    assert!(harness.state().state.request_quit, "Don't Save quits");
}

#[test]
fn save_from_the_prompt_saves_then_quits_and_a_failed_save_stays_open() {
    let mut harness = shell_harness();
    let path = temp_project("save");
    // A failed save first: `path` is not created yet, so a file under it
    // cannot be written.
    harness.state_mut().state.project_path = Some(path.join("nested.camproj"));
    quit(&mut harness);
    let prompt_top = harness.get_by_label("Unsaved changes").rect().top();
    harness
        .get_all_by_role_and_label(egui::accesskit::Role::Button, "Save")
        .find(|n| n.rect().top() > prompt_top)
        .expect("Unsaved changes Save button")
        .click();
    harness.run();
    let st = &harness.state().state;
    assert!(!st.request_quit, "a failed save must keep the app open");
    assert!(st.status.starts_with("save failed:"), "status: {}", st.status);
    assert!(st.history.is_dirty(), "nothing was saved");

    harness.state_mut().state.project_path = Some(path.clone());
    quit(&mut harness);
    let prompt_top = harness.get_by_label("Unsaved changes").rect().top();
    harness
        .get_all_by_role_and_label(egui::accesskit::Role::Button, "Save")
        .find(|n| n.rect().top() > prompt_top)
        .expect("Unsaved changes Save button")
        .click();
    harness.run();
    let st = &harness.state().state;
    let written = path.exists();
    let _ = std::fs::remove_file(&path);
    assert!(written, "Save wrote the project");
    assert!(!st.history.is_dirty(), "the saved document is clean");
    assert!(st.request_quit, "after the save the close goes through");
}

#[test]
fn new_over_unsaved_changes_asks_and_a_clean_document_never_does() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::NewDocument);
    harness.run();
    assert!(
        model::find_by_name(&harness.state().state.world, "Contour1").is_some(),
        "New over unsaved work waits for the answer"
    );
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::ResolveUnsaved(UnsavedChoice::DontSave));
    harness.run();
    let st = &harness.state().state;
    assert!(model::find_by_name(&st.world, "Contour1").is_none(), "Don't Save discarded");
    assert_eq!(st.status, "new document");
    assert!(!st.history.is_dirty(), "a new document is clean");
    assert_eq!(st.window_title(), "CamCraft - Untitled");

    // Clean: Quit goes straight through, no prompt.
    quit(&mut harness);
    let st = &harness.state().state;
    assert!(st.unsaved_prompt.is_none() && st.request_quit, "a clean close never asks");
}

#[test]
fn undo_back_to_the_saved_point_clears_the_mark() {
    let mut harness = shell_harness();
    let path = temp_project("undo");
    harness.state_mut().state.pending.push_back(CommandKind::SaveProjectFile(path.clone()));
    harness.run();
    let _ = std::fs::remove_file(&path);
    let saved_title = format!("CamCraft - {}", path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()));
    assert_eq!(harness.state().state.window_title(), saved_title);
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert_eq!(harness.state().state.window_title(), format!("*{saved_title}"));
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run();
    assert_eq!(harness.state().state.window_title(), saved_title);
}

/// #144: an OS quit (macOS Cmd+Q, the app menu's or the Dock's Quit, a
/// logout) is held by the terminate hook and routed as the same guarded
/// Quit, so unsaved work raises the prompt instead of ending the process.
/// The only test that touches the hook's process-wide flag.
#[test]
fn os_quit_request_over_unsaved_changes_prompts_once() {
    use crate::egui_app::os_quit;
    let mut harness = shell_harness();
    assert!(!os_quit::route_pending(&mut harness.state_mut().state), "nothing asked, nothing queued");

    os_quit::request();
    assert!(os_quit::route_pending(&mut harness.state_mut().state), "the request is routed");
    assert!(!os_quit::route_pending(&mut harness.state_mut().state), "a request is taken once");
    harness.run();
    let st = &harness.state().state;
    assert!(!st.request_quit, "a dirty OS quit must not exit");
    assert!(st.unsaved_prompt.is_some(), "the quit is held for the answer");
    harness.get_by_label("Unsaved changes");

    harness.get_by_label("Don't Save").click();
    harness.run();
    assert!(harness.state().state.request_quit, "Don't Save quits");
}
