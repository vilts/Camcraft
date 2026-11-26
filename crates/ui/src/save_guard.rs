//! The overwrite guard of the window's Save (#3608): a save to the file that
//! the window opened or last saved asks first when that file on disk
//! changed after the window opened or saved it (a second window, a
//! `git pull`, a colleague on a shared folder, a sync client). Nothing is
//! written before the answer: Save As, Cancel or Overwrite. The stamp test
//! is the app layer's (`project::DiskStamp`); this module holds the stamp,
//! the held save and the answer. Ephemeral UI state - never on the history.
//!
//! Provenance: our own design, after the usual desktop-editor question for
//! a file that changed outside the editor; no HSMWorks material involved.

use std::path::{Path, PathBuf};

use camcraft_app::project::DiskStamp;

use crate::state::{CommandKind, State};

/// The prompt's three answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteChoice {
    /// Replace the file on disk with the document.
    Overwrite,
    /// Ask for another file (Save As); the file on disk stays.
    SaveAs,
    /// Write nothing; the file on disk stays.
    Cancel,
}

/// What the window knows of the file that it opened or last saved.
#[derive(Debug, Default)]
pub struct SaveGuard {
    /// The disk stamp of that file after the window opened or saved it, or
    /// after an agent's save over it (`mcp_host`).
    pub(crate) stamp: Option<DiskStamp>,
    /// A save held for the prompt: the file that it would replace. While
    /// Some, the shell shows the Overwrite / Save As / Cancel prompt.
    pub(crate) held: Option<PathBuf>,
    /// The user chose Overwrite: the next save of this file goes through,
    /// once.
    confirmed: Option<PathBuf>,
}

/// The file name a prompt or a status line names.
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl State {
    /// `true`: save the document to `path` now. `false`: `path` is the file
    /// that the window opened or last saved, and the file on disk changed
    /// after that; the save is held for the prompt and nothing is written.
    pub(crate) fn may_save_over(&mut self, path: &Path) -> bool {
        let confirmed = self.save_guard.confirmed.take();
        let changed = self
            .save_guard
            .stamp
            .as_ref()
            .is_some_and(|s| s.changed(path));
        if !changed || confirmed.as_deref() == Some(path) {
            return true;
        }
        self.status = format!(
            "{}: the file on disk changed after this window opened or saved it - nothing was written",
            file_name(path)
        );
        self.save_guard.held = Some(path.to_path_buf());
        false
    }

    /// Resolve the prompt. Each answer goes through the command queue, so
    /// the Save As dialog runs in `State::run`, never mid-frame.
    pub(crate) fn resolve_overwrite(&mut self, choice: OverwriteChoice) {
        let Some(path) = self.save_guard.held.take() else {
            return;
        };
        match choice {
            OverwriteChoice::Overwrite => {
                self.save_guard.confirmed = Some(path.clone());
                self.pending.push_back(CommandKind::SaveProjectFile(path));
            }
            OverwriteChoice::SaveAs => self.pending.push_back(CommandKind::SaveProjectAsDialog),
            OverwriteChoice::Cancel => {
                self.status = format!(
                    "save cancelled - nothing was written to {}",
                    file_name(&path)
                );
            }
        }
    }
}
