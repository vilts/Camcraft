//! The recent-files drop-downs (issue #662) under the ribbon's Open
//! (projects) and Model (models) buttons. With a recent list on hand
//! the button opens its list - the files newest first by name (full
//! path on hover; a gone file greyed with "(missing)" and refused by
//! name), "Clear recent", and "Browse…" for today's file dialog. With
//! no recents the button opens the dialog, unless installed examples
//! provide an "Open example" submenu. A pick queues `OpenRecent` / `ClearRecent`
//! / the dialog command, so the unsaved-changes prompt and the import
//! worker are Open's and Model's own; the door itself is view state.
//! Provenance: our own code, the hand-drawn dropdown law of the Theme
//! and View doors (theme_drop.rs, layout/mod.rs); no HSMWorks material.

use std::path::{Path, PathBuf};

use camcraft_app::recent::{RecentEntry, RecentKind};
use egui::{Rect, Sense, Ui, Vec2};

use super::panes::{Cell, Cmd};
use super::theme::Tokens;
use crate::actions;
use crate::state::{CommandKind, State};

/// The bottom rows' captions (also their AccessKit labels).
pub(super) const CLEAR: &str = "Clear recent";
pub(super) const BROWSE: &str = "Browse…";

/// One row of a recent-files list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RecentRow {
    /// A remembered file.
    Entry(RecentEntry),
    /// Empty this list.
    Clear,
    /// Today's file dialog.
    Browse,
    /// Installed projects beside the executable.
    Examples,
}

impl RecentRow {
    /// What the row reads (and its AccessKit label).
    pub(super) fn label(&self) -> String {
        match self {
            RecentRow::Entry(e) if e.missing => format!("{} (missing)", e.name),
            RecentRow::Entry(e) => e.name.clone(),
            RecentRow::Clear => CLEAR.to_owned(),
            RecentRow::Browse => BROWSE.to_owned(),
            RecentRow::Examples => "Open example".to_owned(),
        }
    }

    fn live(&self) -> bool {
        !matches!(self, RecentRow::Entry(e) if e.missing)
    }
}

/// Which list a ribbon cell carries.
pub(super) fn kind_of(c: Cmd) -> Option<RecentKind> {
    match c {
        Cmd::Project => Some(RecentKind::Project),
        Cmd::Model => Some(RecentKind::Model),
        _ => None,
    }
}

/// The cell a list hangs under.
fn cmd_of(kind: RecentKind) -> Cmd {
    match kind {
        RecentKind::Project => Cmd::Project,
        RecentKind::Model => Cmd::Model,
    }
}

/// The dialog the Browse row (and an empty list's button) opens.
fn browse(state: &mut State, kind: RecentKind) {
    match kind {
        RecentKind::Project => actions::open_project(state),
        RecentKind::Model => actions::open_model_dialog(state),
    }
}

/// The Open / Model button: toggle its list for recents or examples, else
/// the dialog as before. Opening a list closes every other door.
pub(super) fn press(state: &mut State, kind: RecentKind) {
    if state.recent.list(kind).is_empty() && !has_examples(state, kind) {
        state.ribbon_recent_open = None;
        browse(state, kind);
        return;
    }
    if state.ribbon_recent_open == Some(kind) {
        state.ribbon_recent_open = None;
        return;
    }
    state.ribbon_recent_open = Some(kind);
    state.ribbon_more = None;
    state.ribbon_mode_open = false;
    state.ribbon_view_open = false;
    state.ribbon_theme_open = false;
}

/// The list's rows, pure geometry in the strip's global frame (the
/// Theme door's law): hung under the button, pulled left so the list
/// never runs off the strip's right edge. The tests click these rects.
pub(super) fn rows_of(
    state: &State,
    kind: RecentKind,
    button: Rect,
    strip: Rect,
    pal: &Tokens,
) -> Vec<(RecentRow, Rect)> {
    let mut rows: Vec<RecentRow> = state.recent_entries(kind).into_iter().map(RecentRow::Entry).collect();
    rows.push(RecentRow::Clear);
    rows.push(RecentRow::Browse);
    if has_examples(state, kind) {
        rows.push(RecentRow::Examples);
    }
    let g = pal.dropdown();
    let w = g.fit_width(g.recent_width, strip.width());
    let left = g.fit_left(button.left(), strip, w);
    rows.into_iter()
        .enumerate()
        .map(|(k, r)| {
            let min = egui::Pos2::new(left, button.bottom() + g.gap + k as f32 * pal.row_height);
            (r, Rect::from_min_size(min, Vec2::new(w, pal.row_height)))
        })
        .collect()
}

/// Draw the open list, if any, under its button in `cells`. A list
/// whose button or entries are gone closes itself.
pub(super) fn draw(ui: &mut Ui, state: &mut State, pal: &Tokens, id: egui::Id, cells: &[Cell], strip: Rect) {
    let Some(kind) = state.ribbon_recent_open else { return };
    let button = cells.iter().find(|c| c.cmd == cmd_of(kind)).map(|c| c.rect);
    match button {
        Some(button) if !state.recent.list(kind).is_empty() || has_examples(state, kind) => {
            let rows = rows_of(state, kind, button, strip, pal);
            show(ui, state, kind, pal, id, button, &rows);
        }
        _ => state.ribbon_recent_open = None,
    }
}

/// The open list: a foreground Area of rows. A press on a row acts and
/// closes the list; an outside press closes it.
fn show(
    ui: &mut Ui,
    state: &mut State,
    kind: RecentKind,
    pal: &Tokens,
    id: egui::Id,
    button: Rect,
    rows: &[(RecentRow, Rect)],
) {
    let Some(top) = rows.first().map(|(_, r)| r.min) else {
        state.ribbon_recent_open = None;
        return;
    };
    let g = pal.dropdown();
    let w = rows.first().map_or(g.recent_width, |(_, r)| r.width());
    let panel = Rect::from_min_size(top, Vec2::new(w, rows.len() as f32 * pal.row_height));
    let mut chose: Option<RecentRow> = None;
    let mut example_pick = None;
    let mut example_open = false;
    egui::Area::new(id)
        .fixed_pos(panel.min)
        .order(egui::Order::Foreground)
        .movable(false)
        .show(ui.ctx(), |ui| {
            let p = ui.painter().clone();
            p.rect_filled(panel, pal.radius_menu, pal.ink);
            p.rect_stroke(panel, pal.radius_menu, egui::Stroke::new(1.0, pal.border), egui::StrokeKind::Outside);
            for (k, (row, r)) in rows.iter().enumerate() {
                if matches!(row, RecentRow::Examples) {
                    let result = ui.scope_builder(egui::UiBuilder::new().max_rect(*r), |ui| {
                        ui.menu_button("Open example", |ui| {
                            let paths = example_directory(state)
                                .map(|dir| example_projects(&dir))
                                .unwrap_or_default();
                            if paths.is_empty() {
                                ui.add_enabled(false, egui::Button::new("No example projects"));
                            }
                            for path in paths {
                                let label = path.file_name().unwrap_or_default().to_string_lossy();
                                if ui.button(label).clicked() {
                                    example_pick = Some(path);
                                    ui.close();
                                }
                            }
                        })
                    });
                    example_open = result.inner.inner.is_some();
                    continue;
                }
                let resp = ui.interact(*r, id.with(("recent-row", k)), Sense::click());
                let (label, live) = (row.label(), row.live());
                super::painted::menu_item(&resp, &label, live, None);
                if resp.hovered() && live {
                    p.rect_filled(*r, 0.0, pal.field_hover);
                }
                if matches!(row, RecentRow::Clear) {
                    p.hline(r.x_range(), r.top(), egui::Stroke::new(1.0, pal.border));
                }
                p.with_clip_rect(r.shrink2(Vec2::new(g.clip_inset_x, 0.0))).text(
                    r.left_center() + Vec2::new(g.pad_x, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &label,
                    egui::FontId::proportional(pal.font_row),
                    if live { pal.text } else { pal.text_faint },
                );
                let clicked = resp.clicked();
                if let RecentRow::Entry(e) = row {
                    resp.on_hover_text(e.path.display().to_string());
                }
                if clicked {
                    chose = Some(row.clone());
                }
            }
        });
    if let Some(path) = example_pick {
        state.ribbon_recent_open = None;
        state.pending.push_back(CommandKind::OpenRecent(RecentKind::Project, path));
        return;
    }
    match chose {
        Some(row) => {
            state.ribbon_recent_open = None;
            match row {
                // Refused by name, never a crash: the file is gone.
                RecentRow::Entry(e) if e.missing => state.status = e.refusal(kind),
                RecentRow::Entry(e) => state.pending.push_back(CommandKind::OpenRecent(kind, e.path)),
                RecentRow::Clear => state.pending.push_back(CommandKind::ClearRecent(kind)),
                RecentRow::Browse => browse(state, kind),
                RecentRow::Examples => {},
            }
        }
        None => {
            let outside = ui.input(|i| {
                i.pointer.button_pressed(egui::PointerButton::Primary)
                    && i.pointer.interact_pos().is_none_or(|p| {
                        !button.expand(g.gap).contains(p) && !rows.iter().any(|(_, r)| r.contains(p))
                    })
            });
            if outside && !example_open {
                state.ribbon_recent_open = None;
            }
        }
    }
}

fn has_examples(state: &State, kind: RecentKind) -> bool {
    kind == RecentKind::Project && example_directory(state).is_some()
}

/// Installed packages keep examples beside the executable, independent of CWD.
fn example_directory(state: &State) -> Option<PathBuf> {
    if state.browser_build {
        return None;
    }
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            diag_warn!("Open example: cannot locate the executable: {err}");
            return None;
        }
    };
    let directory = executable.parent()?.join("examples");
    // Keep packaged-directory fixtures local to their test thread.
    #[cfg(test)]
    let directory = example_tests::directory_override().unwrap_or(directory);
    directory.is_dir().then_some(directory)
}

/// List direct project files only; keep package order stable across platforms.
fn example_projects(directory: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(err) => {
            diag_warn!("Open example: cannot read {}: {err}", directory.display());
            return Vec::new();
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        match entry.and_then(|entry| Ok((entry.path(), entry.file_type()?))) {
            Ok((path, kind)) if kind.is_file() && path.extension().is_some_and(|ext| ext == "camproj") => paths.push(path),
            Ok(_) => {},
            Err(err) => diag_warn!("Open example: cannot read an entry in {}: {err}", directory.display()),
        }
    }
    paths.sort();
    paths
}

#[cfg(test)]
#[path = "../../tests/open_examples/mod.rs"]
mod example_tests;
