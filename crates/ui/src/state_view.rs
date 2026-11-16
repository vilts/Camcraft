//! The view preferences on the State (issue #645, the toggle asked on
//! top of #727): the gizmo-name flag is `camcraft_app::view` preference
//! state - read once at startup from the per-user file, written back
//! the moment the browser checkbox flips it. The flag itself is view
//! state like the camera (never a command, never on the history); what
//! persists is the user's choice, so the next run starts the way this
//! one ended.
//! Provenance: our own wiring over our own app::view; no HSMWorks material.

use std::path::Path;

use camcraft_app::view;

use super::State;

impl State {
    /// Startup: the per-user view file (the real window only - the
    /// headless suites never touch the user's file). A corrupt or
    /// unreadable file starts at the defaults and says so on the
    /// status line.
    pub(crate) fn load_view(&mut self) {
        let Some(path) = view::store_path() else {
            diag_warn!("view settings: no config folder - they last for this session only");
            return;
        };
        self.load_view_from(&path);
    }

    /// Read the view preferences from `path` and keep writing back there.
    pub fn load_view_from(&mut self, path: &Path) {
        let (prefs, note) = view::load(path);
        self.show_gizmo_names = prefs.show_gizmo_names;
        self.view_store = Some(path.to_path_buf());
        if let Some(why) = note {
            self.status = why;
        }
    }

    /// The browser's "Show origin names" checkbox: flip the flag, and
    /// with it take the user's choice out to the file at once - a
    /// failed write is logged (by `view::save`), never shown over the
    /// click; the flag in memory stays right for this session either
    /// way.
    pub(crate) fn set_gizmo_names(&mut self, on: bool) {
        self.show_gizmo_names = on;
        let Some(path) = self.view_store.clone() else { return };
        let prefs = view::ViewPrefs { show_gizmo_names: on };
        let _ = view::save(&path, &prefs);
    }
}
