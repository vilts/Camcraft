//! A project open read-only on the State (#3609): `camcraft-ui --read-only
//! job.camproj` opens the project through the window's own open
//! (`open_project_path`) and marks it read-only. Save then opens the Save
//! As dialog, as the editor for shipped machines does. A save over the
//! opened file is refused on the status line by
//! `camcraft_app::project::refuse_read_only_save`, the rule that MCP and
//! scripts use too. A save to a different file ends read-only. Changes
//! stay allowed: they are commands on the history like any other.
//! Provenance: our own design and code. The behaviour matches SolidCraft's
//! `--read-only` (SolidCraft #20830); no SolidCraft code or text is used.

use std::path::{Path, PathBuf};

use super::{State, save_project_dialog};

impl State {
    /// Open `path` as File > Open does, then mark it read-only when it
    /// opened. A refused open keeps the old document and says why.
    pub(crate) fn open_project_read_only(&mut self, path: PathBuf) {
        self.open_project_path(path.clone());
        if self.project_path.as_deref() == Some(path.as_path()) {
            self.project_read_only = true;
            self.status.push_str(" (read-only; Save As writes your own copy)");
        }
    }

    /// The file that Save writes: the project's own file, or a file from
    /// the Save As dialog when the project has no file or is open
    /// read-only. `None` when the dialog is cancelled.
    pub(crate) fn save_target(&mut self) -> Option<PathBuf> {
        match self.project_path.clone() {
            Some(own) if self.project_read_only => {
                self.status = format!("{} is open read-only; Save As writes your own copy", own.display());
                save_project_dialog(Some(&own))
            }
            Some(own) => Some(own),
            None => save_project_dialog(None),
        }
    }

    /// Refuse a save over the file that the project is open read-only
    /// from, on the status line. `true`: refused, and nothing was written.
    pub(crate) fn refuse_read_only_save(&mut self, path: &Path) -> bool {
        let opened = self.project_path.as_deref();
        match camcraft_app::project::refuse_read_only_save(opened, self.project_read_only, path) {
            Ok(()) => false,
            Err(why) => {
                self.status = format!("save refused: {why} (File > Save As)");
                true
            }
        }
    }
}
