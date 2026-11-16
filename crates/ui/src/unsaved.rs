//! The unsaved-changes guard (#107): New, Open and Quit over a document
//! with unsaved changes are held back until the user answers Save /
//! Don't Save / Cancel. The flag itself belongs to the app layer
//! (`History::is_dirty` - the history position against the last save or
//! load); this module only holds the action and resolves the answer.
//!
//! Provenance: our own design, specced from common desktop-editor
//! save-on-close behavior; no HSMWorks material involved.

use camcraft_app::recent::RecentKind;

use crate::state::{CommandKind, State, save_project_dialog};

/// The prompt's three answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsavedChoice {
    /// Save (asking for a path if the document has none), then go on.
    Save,
    /// Discard the unsaved changes and go on.
    DontSave,
    /// Keep the document open; the held action is dropped.
    Cancel,
}

/// The user-facing actions that would throw the current document away.
/// The `*File` arms stay unguarded: they are the scripted/headless paths.
fn discards_document(action: &CommandKind) -> bool {
    match action {
        CommandKind::NewDocument | CommandKind::OpenProjectDialog | CommandKind::Quit => true,
        // #662: a recent project replaces the document like Open does.
        // A missing one is refused by name before anything is asked.
        CommandKind::OpenRecent(RecentKind::Project, path) => path.is_file(),
        _ => false,
    }
}

/// What the prompt says the held action is about to do.
#[must_use]
pub fn held_verb(action: &CommandKind) -> &'static str {
    match action {
        CommandKind::NewDocument => "starting a new document",
        CommandKind::OpenProjectDialog | CommandKind::OpenRecent(..) => "opening another project",
        _ => "closing",
    }
}

impl State {
    /// Let `action` through, or hold it for the prompt when it would
    /// discard unsaved changes. A second request while the prompt is up
    /// keeps the first one (a double click on the window X asks once).
    pub(crate) fn hold_if_unsaved(&mut self, action: CommandKind) -> Option<CommandKind> {
        if self.discard_confirmed || !discards_document(&action) || !self.history.is_dirty() {
            return Some(action);
        }
        if self.unsaved_prompt.is_none() {
            self.unsaved_prompt = Some(action);
        }
        self.status = String::from("unsaved changes");
        None
    }

    /// Resolve the prompt. A save that fails or is cancelled keeps the
    /// document (and the app) open; the status line says why.
    pub(crate) fn resolve_unsaved(&mut self, choice: UnsavedChoice) {
        let Some(action) = self.unsaved_prompt.take() else {
            return;
        };
        match choice {
            UnsavedChoice::Cancel => {
                self.status = String::from("cancelled - the document stays open");
            }
            UnsavedChoice::DontSave => self.run_confirmed(action),
            UnsavedChoice::Save if self.browser_build => {
                // #1431: the browser's Save is a download.
                if self.download_project(false) {
                    self.run_confirmed(action);
                } else {
                    self.status.push_str(" - the document stays open");
                }
            }
            UnsavedChoice::Save => {
                let Some(path) = self.project_path.clone().or_else(|| save_project_dialog(None))
                else {
                    self.status = String::from("save cancelled - the document stays open");
                    return;
                };
                self.save_project_path(path);
                if self.history.is_dirty() {
                    // save_project_path left "save failed: ..." on the
                    // status line; nothing is discarded.
                    self.status.push_str(" - the document stays open");
                    return;
                }
                self.run_confirmed(action);
            }
        }
    }

    /// Run a held action past the guard, once.
    fn run_confirmed(&mut self, action: CommandKind) {
        self.discard_confirmed = true;
        self.run(action);
        self.discard_confirmed = false;
    }
}
