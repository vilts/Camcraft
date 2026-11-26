//! The view preferences on the State (issue #645, the toggle asked on
//! top of #727): the gizmo-name flag is `camcraft_app::view` preference
//! state - read once at startup from the per-user file, written back
//! the moment the browser checkbox flips it. The flag itself is view
//! state like the camera (never a command, never on the history); what
//! persists is the user's choice, so the next run starts the way this
//! one ended.
//! #2958: the line width and the anti-aliasing are kept the same way;
//! every write takes all the view settings, so one never resets another.
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
        self.display = crate::state::DisplayMode::from_key(&prefs.display);
        self.show_grid = prefs.show_grid;
        self.ground_shadow = prefs.ground_shadow;
        self.ambient_occlusion = prefs.ambient_occlusion;
        self.line_width = prefs.line_width;
        self.antialias = prefs.antialias;
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
        self.persist_view();
    }

    /// Write every view setting back to the per-user file (#2818: the
    /// display mode and the grid switch; #2958: the line width and the
    /// anti-aliasing). The headless suites have no file and write nothing.
    /// A failed write is logged by `view::save`; the session stays right.
    pub(crate) fn persist_view(&self) {
        let Some(path) = &self.view_store else { return };
        let prefs = view::ViewPrefs {
            show_gizmo_names: self.show_gizmo_names,
            display: self.display.key().to_owned(),
            show_grid: self.show_grid,
            ground_shadow: self.ground_shadow,
            ambient_occlusion: self.ambient_occlusion,
            line_width: self.line_width,
            antialias: self.antialias,
        };
        let _ = view::save(path, &prefs);
    }

    /// The shading switches of the Display menu (#2834), as the draw list
    /// and the scene read them.
    #[must_use]
    pub fn effects(&self) -> crate::draw_list::Effects {
        crate::draw_list::Effects { ground_shadow: self.ground_shadow, ambient_occlusion: self.ambient_occlusion }
    }

    /// #2958: choose the overlay line width; kept at once.
    pub(crate) fn select_line_width(&mut self, next: view::LineWidth) {
        self.line_width = next;
        self.persist_view();
        self.status = format!("line width: {} pt", next.word());
    }

    /// #2958: choose the anti-aliasing. A sample count that this graphics
    /// device does not offer is refused by name and never kept.
    pub(crate) fn select_antialias(&mut self, next: view::Antialias) {
        if !self.samples_offered.contains(&next.samples()) {
            self.refuse(
                super::ErrorScope::Setup,
                format!("anti-aliasing {} not selected: this graphics device does not offer it", next.word()),
            );
            return;
        }
        self.antialias = next;
        self.persist_view();
        self.status = format!("anti-aliasing: {}", next.word());
    }

    /// #2177: add `line` to the status line once - a refusal that every
    /// document edit asks again is not said a second time while the
    /// status line still shows it.
    pub(crate) fn say_once(&mut self, line: &str) {
        if !self.status.contains(line) {
            self.status += &format!(" | {line}");
        }
    }
}
