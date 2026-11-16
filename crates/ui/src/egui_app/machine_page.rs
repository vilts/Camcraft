//! The setup's Machine page, laid out like the WCS page (#1440, after
//! #739): titled groups - which machine the setup uses, the machine file
//! (library pick plus the overview grid, or the no-machine message) and
//! the load / edit / clear actions as equal buttons filling the pane's
//! width. Setup furniture read live off the world singleton - no mirror to
//! go stale. The overview is exactly what the file stated; a blank is
//! "not stated", never an invented ceiling. The library combo picks a
//! shipped machine BY NAME (`LoadBuiltinMachine`), "Load other" reaches
//! any `.mch` through the file law (`LoadMachineDialog`), "Edit" opens the
//! draft editor (`EditMachineDialog`) and "Clear" drops it
//! (`ClearMachine`); the own / document choice is `SetupOwnMachine`. All
//! of them are the undoable commands the page used before; no command or
//! project field is new.
//!
//! Provenance: our own layout over the `layout` form helpers; the facts
//! come from `Machine::rows`; no vendor material.

use egui::{RichText, Ui, Vec2};

use camcraft_app::{machine, posting};

use crate::state::{CommandKind, State};

use super::layout::{group, prop_grid};
use super::setup_pages::{ownership, Mount};
use super::wcs_page::{cell_w, even_row};

/// A button row's height (the WCS page's).
const BUTTON_H: f32 = 24.0;
/// The group titles (the structural twin reads them).
pub(super) const GROUPS: [&str; 3] = ["Which machine", "Machine file", "Change machine"];
/// The one line under "Machine file" saying what the group does.
pub(super) const FILE_HELP: &str = "Pick a library machine, or load a .mch file.";
/// What the page says with no machine loaded.
pub(super) const NO_MACHINE: &str = "No machine loaded - the checker states, it cannot judge.";

/// The page body for setup `ctx` (`None` - the implicit context).
pub(super) fn machine_page(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    group(ui, GROUPS[0], None, |ui| ownership(ui, state, ctx, Mount::Machine));
    let (path, m) = machine::of(&state.world);
    let names: Vec<&str> = posting::MACHINES.iter().map(|(n, _)| *n).collect();
    group(ui, GROUPS[1], None, |ui| {
        ui.label(RichText::new(FILE_HELP).small().weak());
        library_row(ui, state, &path, &names);
        match &m {
            Some(m) => {
                let source = if path.is_empty() {
                    "not stated".to_owned()
                } else {
                    match names.iter().find(|n| path == posting::builtin_path(n)) {
                        Some(n) => format!("Built-in: {n}"),
                        None => path.clone(),
                    }
                };
                super::machine_grid::machine_grid(ui, "machine_overview", m, state.units(), &[("Source", source)]);
            }
            None => {
                ui.label(NO_MACHINE);
            }
        }
    });
    group(ui, GROUPS[2], None, |ui| actions(ui, state, m.is_some()));
}

/// Library | combo on one row (the page's label column).
fn library_row(ui: &mut Ui, state: &mut State, path: &str, names: &[&str]) {
    let sel: Option<usize> = names.iter().position(|n| path == posting::builtin_path(n));
    let text = sel.and_then(|i| names.get(i)).map_or("other", |n| *n).to_owned();
    let mut pick: Option<String> = None;
    prop_grid(ui, "machine_library_grid", |ui| {
        ui.label("Library");
        let cb = egui::ComboBox::from_id_salt("machine_library").selected_text(text).show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                if ui.selectable_label(sel == Some(i), *n).clicked() {
                    pick = Some((*n).to_owned());
                }
            }
        });
        super::fields::named(cb.response, "Machine library");
        ui.end_row();
    });
    if let Some(name) = pick {
        state.pending.push_back(CommandKind::LoadBuiltinMachine(name));
    }
}

/// Load other / Edit / Clear as equal buttons; with no machine, the one
/// load button. #1431: in the browser build the `.mch` load slot greys
/// with its refusal reason (a page has no file picker yet); the rows
/// lay out exactly as on the desktop while nothing is refused.
fn actions(ui: &mut Ui, state: &mut State, has_machine: bool) {
    let avail = ui.available_width();
    let why = state.browser_refusal_of(&CommandKind::LoadMachineDialog);
    if has_machine {
        let size = Vec2::new(cell_w(ui, avail, 3), BUTTON_H);
        let clicked = match why {
            None => even_row(ui, &["Load other", "Edit", "Clear"], size),
            Some(why) => {
                let mut hit = None;
                ui.horizontal(|ui| {
                    if crate::browser_files::button_sized(ui, Some(why), "Load other", "", size) {
                        hit = Some(0);
                    }
                    for (i, label) in ["Edit", "Clear"].iter().enumerate() {
                        if ui.add_sized(size, egui::Button::new(*label)).clicked() {
                            hit = Some(i + 1);
                        }
                    }
                });
                hit
            }
        };
        match clicked {
            Some(0) => state.pending.push_back(CommandKind::LoadMachineDialog),
            Some(1) => state.pending.push_back(CommandKind::EditMachineDialog),
            Some(_) => state.pending.push_back(CommandKind::ClearMachine),
            None => {}
        }
    } else {
        let size = Vec2::new(cell_w(ui, avail, 1), BUTTON_H);
        let load = match why {
            None => even_row(ui, &["Load machine (.mch)"], size).is_some(),
            Some(why) => crate::browser_files::button_sized(ui, Some(why), "Load machine (.mch)", "", size),
        };
        if load {
            state.pending.push_back(CommandKind::LoadMachineDialog);
        }
    }
}
