//! The theme and colour-vision preferences on the State (issue #877):
//! read once at startup from the per-user `camcraft_app::prefs` file,
//! written back on every pick. They are the person's, not the
//! document's: a pick is not on the history (Ctrl+Z never undoes it),
//! it does not mark the document changed, and neither New nor Open
//! touches it. Each pick is still validated before it is set - a missing
//! or corrupt theme file and a colour-vision mode that fails its gate on
//! the live theme are refused by name on the status line.
//! Provenance: our own wiring over our own `app::prefs`, shaped like
//! `state_recent.rs` (#662); no HSMWorks material.

use std::path::{Path, PathBuf};

use camcraft_app::commands::{check_theme, color_vision_of, color_vision_set, theme_of, theme_set};
use camcraft_app::prefs::{self, ViewPrefs};
use camcraft_app::schema::{ColorVision, ThemeRef};

use super::{ErrorScope, State, theme_label};
use crate::egui_app::{colorvision, theme, theme_files};

impl State {
    /// Startup: the per-user preferences file (the real window only - the
    /// headless suites never touch the user's file).
    pub(crate) fn load_prefs(&mut self) {
        let Some(path) = prefs::store_path() else {
            diag_warn!("preferences: no config folder - the theme and colour vision last for this session only");
            return;
        };
        self.load_prefs_from(path);
    }

    /// Read the preferences from `path`, wear them, and keep writing back
    /// there. A stored colour-vision mode is gated against the theme it
    /// lands on, as a pick is; one that fails stays Off, named.
    pub fn load_prefs_from(&mut self, path: PathBuf) {
        let (stored, note) = prefs::load(&path);
        self.prefs_store = Some(path);
        self.import_tool_holders = !stored.skip_tool_holders;
        theme_set(&self.world, &stored.theme);
        let tokens = theme_files::resolve(&mut self.theme_cache, &stored.theme)
            .map_or_else(|_| theme::current(), |(t, _)| t);
        let mut notes: Vec<String> = note.into_iter().collect();
        match colorvision::gate(stored.color_vision, &tokens) {
            Some(why) => notes.push(format!("colour vision {} not restored: {why}", stored.color_vision.label())),
            None => color_vision_set(&self.world, stored.color_vision),
        }
        if !notes.is_empty() {
            self.status = notes.join("; ");
        }
    }

    /// Where the preferences are written back, if anywhere.
    #[must_use]
    pub fn prefs_path(&self) -> Option<&Path> {
        self.prefs_store.as_deref()
    }

    pub(crate) fn persist_prefs(&self) {
        let Some(path) = &self.prefs_store else { return };
        let now = ViewPrefs {
            theme: theme_of(&self.world),
            color_vision: color_vision_of(&self.world),
            skip_tool_holders: !self.import_tool_holders,
        };
        // The refusal is logged where it happens (app::prefs::save); the
        // choice in force stays right for this session either way.
        let _ = prefs::save(path, &now);
    }

    /// Choose the theme. The cheap existence law (`check_theme`) and the
    /// full parse - which the app cannot name (layering: the JSON reader
    /// lives on the surface side) - both run HERE, so a corrupt theme is a
    /// visible refusal at the pick and is never set or stored. The look
    /// itself lands on the next `sync_theme`.
    pub(super) fn select_theme(&mut self, next: ThemeRef) {
        if let ThemeRef::File(path) = &next
            && let Err(why) = &*self.theme_cache.get(Path::new(path))
        {
            self.refuse(ErrorScope::Setup, format!("theme not selected: {why}"));
            return;
        }
        if let Err(why) = check_theme(&next) {
            self.refuse(ErrorScope::Setup, format!("theme not selected: {why}"));
            return;
        }
        if theme_of(&self.world) != next {
            theme_set(&self.world, &next);
            self.persist_prefs();
        }
        self.status = format!("theme: {}", theme_label(&self.world));
    }

    /// Choose the colour-vision remap, `select_theme`'s twin: the surface
    /// owns the palette and the gate (layering - app holds only the
    /// reference), so the gate runs HERE against the live table and a
    /// failing mode is a visible refusal at the pick.
    pub(super) fn select_color_vision(&mut self, next: ColorVision) {
        if let Some(why) = colorvision::gate(next, &theme::current()) {
            self.refuse(ErrorScope::Setup, format!("colour vision not selected: {why}"));
            return;
        }
        if color_vision_of(&self.world) != next {
            color_vision_set(&self.world, next);
            self.persist_prefs();
        }
        self.status = format!("colour vision: {}", next.label());
    }
}
