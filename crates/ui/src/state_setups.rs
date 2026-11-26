//! The saved panel setups on the State (#2339): read once at startup from
//! the per-user `camcraft_app::prefs` setups file, written back on every
//! save, load or delete. They are the person's, not the document's: a
//! saved arrangement is not on the history (Ctrl+Z never undoes it), it
//! does not mark the document changed, and neither New nor Open touches
//! it. The dock itself is the shell's; this module only keeps the store
//! and the file. A corrupt or older file is an empty store with the
//! refusal on the status line - never a crash.
//! Provenance: our own wiring over our own `app::prefs`, shaped like
//! `state_prefs.rs` (#877); no HSMWorks material.

use std::path::{Path, PathBuf};

use camcraft_app::prefs;

use super::State;

impl State {
    /// Startup: the per-user panel-setup file (the real window only - the
    /// headless suites read a named temp file instead).
    pub(crate) fn load_panel_setups(&mut self) {
        let Some(path) = prefs::setups_path() else {
            diag_warn!("panel setups: no config folder - they last for this session only");
            return;
        };
        self.load_panel_setups_from(path);
    }

    /// Read the saved arrangements from `path` and keep writing back there.
    pub fn load_panel_setups_from(&mut self, path: PathBuf) {
        let (setups, note) = prefs::load_setups(&path);
        self.panel_setups_store = Some(path);
        self.panel_setups = setups;
        if let Some(note) = note {
            self.status = note;
        }
    }

    /// Where the saved arrangements are written back, if anywhere.
    #[must_use]
    pub fn panel_setups_path(&self) -> Option<&Path> {
        self.panel_setups_store.as_deref()
    }

    /// `Err` names a write that failed; the arrangement is in memory
    /// either way, so the session keeps working.
    fn persist_panel_setups(&self) -> Result<(), String> {
        let Some(path) = &self.panel_setups_store else {
            return Ok(());
        };
        prefs::save_setups(path, &self.panel_setups)
    }

    /// Save one arrangement under `name` (already validated by
    /// `prefs::setup_name`) and remember it as the last one used.
    pub(crate) fn save_panel_setup(
        &mut self,
        name: &str,
        dock: serde_json::Value,
    ) -> Result<(), String> {
        self.panel_setups.upsert(name, dock);
        self.persist_panel_setups()
    }

    /// Remember the arrangement just loaded, so the next start can show it.
    pub(crate) fn remember_setup(&mut self, name: &str) {
        self.panel_setups.last = Some(name.to_owned());
        if let Err(why) = self.persist_panel_setups() {
            self.status = format!("panel setup choice not written: {why}");
        }
    }

    /// Forget one saved arrangement.
    pub(crate) fn delete_panel_setup(&mut self, name: &str) -> bool {
        let removed = self.panel_setups.remove(name);
        if removed && let Err(why) = self.persist_panel_setups() {
            self.status = format!("panel setup not written: {why}");
        }
        removed
    }

    /// The auto-load choice: the person's, written at once.
    pub(crate) fn set_setup_auto_load(&mut self, on: bool) {
        if self.panel_setups.auto_load == on {
            return;
        }
        self.panel_setups.auto_load = on;
        self.status = if on {
            String::from("panel setups: the last one loads at the next start")
        } else {
            String::from("panel setups: the default layout loads at the next start")
        };
        if let Err(why) = self.persist_panel_setups() {
            self.status = format!("panel setup choice not written: {why}");
        }
    }
}
