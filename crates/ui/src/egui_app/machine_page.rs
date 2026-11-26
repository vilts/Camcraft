//! The setup's Machine page, laid out like the WCS page (#1440, after
//! #739): titled groups - which machine the setup uses, the machine file
//! (library pick plus the overview grid, or the no-machine message) and
//! the load / edit / clear actions as equal buttons filling the pane's
//! width. Setup furniture read live off the world singleton - no mirror to
//! go stale. The overview is exactly what the file stated; a blank is
//! "not stated", never an invented ceiling. The library combo picks a
//! shipped machine BY NAME (`LoadBuiltinMachine`) or one of the user's
//! own files (`UseCatalogueMachine`, #2358), "Load other" reaches
//! any `.mch` through the file law (`LoadMachineDialog`), "Edit" opens the
//! draft editor (`EditMachineDialog`) and "Clear" drops it
//! (`ClearMachine`); the own / document choice is `SetupOwnMachine`. All
//! of them are the undoable commands the page used before; no command or
//! project field is new.
//!
//! Provenance: our own layout over the `layout` form helpers; the facts
//! come from `Machine::rows`; no vendor material.

use egui::{RichText, Ui, Vec2};

use camcraft_app::machine::library::{MachineEntry, MachineSource};
use camcraft_app::{machine, posting};

use crate::state::{CommandKind, State};

use super::layout::{group, prop_grid};
use super::setup_pages::{ownership, Mount};
use super::wcs_page::{cell_w, even_row};

/// A button row's height (the WCS page's).
const BUTTON_H: f32 = 24.0;
/// #2105: the page's own vertical rhythm. The shell's item spacing leaves
/// the action row below the visible bottom of a 720 px window: the page
/// draws three titled groups, two radios, two help lines and eight
/// overview rows in a 274 px pane, so its rows sit one px closer.
const ROW_GAP: f32 = 2.0;
/// #2105: the row gap of the machine overview readout (its own, not the
/// form grid's: eight rows in a short pane).
const OVERVIEW_GAP: f32 = 2.0;
/// The group titles (the structural twin reads them).
pub(super) const GROUPS: [&str; 3] = ["Which machine", "Machine file", "Change machine"];
/// The one line under "Machine file" saying what the group does.
pub(super) const FILE_HELP: &str = "Pick a library machine, or load a .mch file.";
/// What the page says with no machine loaded.
pub(super) const NO_MACHINE: &str = "No machine loaded - the checker states, it cannot judge.";

/// The AccessKit name of the page's Refresh button (#2380).
pub(super) const REFRESH_MACHINES: &str = "Refresh machine list";

/// The page body for setup `ctx` (`None` - the implicit context).
pub(super) fn machine_page(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    // #2105: the page draws its rows at its own rhythm (see ROW_GAP). The
    // shell default leaves the "Change machine" row under the visible
    // bottom of a 720 px window: `pages.rs` takes 40 px of the leaf for
    // the separator and the Close footer, so the page body must fit above
    // it. The rhythm is local to this page: the `ui` here is the page's
    // own, rebuilt with the scroll area in each frame.
    ui.spacing_mut().item_spacing.y = ROW_GAP;
    group(ui, GROUPS[0], None, |ui| ownership(ui, state, ctx, Mount::Machine));
    let (path, m) = machine::of(&state.world);
    let names: Vec<&str> = posting::MACHINES.iter().map(|(n, _)| *n).collect();
    group(ui, GROUPS[1], None, |ui| {
        ui.label(RichText::new(FILE_HELP).small().weak());
        library_row(ui, state, &path);
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
                super::machine_grid::machine_grid(ui, "machine_overview", m, state.units(), &[("Source", source)], OVERVIEW_GAP);
            }
            None => {
                ui.label(NO_MACHINE);
            }
        }
    });
    group(ui, GROUPS[2], None, |ui| actions(ui, state, m.is_some()));
}

/// A library entry's words (#2358): a shipped machine by its name; a
/// file by its name and file name, so it cannot pass for a shipped one,
/// and a refused file with its first refusal.
pub(crate) fn library_label(e: &MachineEntry) -> String {
    let MachineSource::File(p) = &e.source else { return e.name.clone() };
    let file = p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
    match e.refusals.first() {
        None => format!("{} ({file})", e.name),
        Some(first) => format!("{} ({file}) - refused: {first}", e.name),
    }
}

/// Library | combo on one row (the page's label column). The list is
/// the Machines pane's catalogue (#2358): the user's own files first -
/// a user who made a machine most likely wants it - then the shipped
/// library. A refused file is listed but cannot be picked.
fn library_row(ui: &mut Ui, state: &mut State, path: &str) {
    super::machines_pane::refresh(state);
    let list: &[MachineEntry] = state.machines_pane.list.as_deref().unwrap_or(&[]);
    let (mine, shipped): (Vec<&MachineEntry>, Vec<&MachineEntry>) =
        list.iter().partition(|e| matches!(e.source, MachineSource::File(_)));
    let current = list.iter().find(|e| !path.is_empty() && e.source.mount_path() == path);
    let text = current.map_or_else(|| "other".to_owned(), library_label);
    let mut pick: Option<MachineSource> = None;
    let mut reread = false;
    // #2105: the Refresh button shares the Library row instead of taking a
    // row of its own. One row on this page is worth ~28 px: with the taller
    // ribbon of #2027 the action row sat 8 px under the fold of a 720 px
    // window. Refresh belongs on the combo's row anyway - it re-reads the
    // same folder the combo lists. The row lays out right to left: the
    // button first, so the combo (which fills the remaining width) cannot
    // push the button out of the cell.
    prop_grid(ui, "machine_library_grid", |ui| {
        ui.label("Library");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let refresh = ui.button("Refresh").on_hover_text("Read the machine folder again");
            if super::fields::named(refresh, REFRESH_MACHINES).clicked() {
                // #2380: read the shipped folder again, then list the
                // catalogue again (its shipped part is that scan).
                posting::rescan();
                reread = true;
            }
            let cb = egui::ComboBox::from_id_salt("machine_library").selected_text(text).show_ui(ui, |ui| {
                for (i, group) in [&mine, &shipped].into_iter().enumerate() {
                    if i == 1 && !mine.is_empty() && !shipped.is_empty() {
                        ui.separator();
                    }
                    for e in group {
                        let on = current.is_some_and(|c| c.source == e.source);
                        let r = ui.add_enabled(e.is_valid(), egui::Button::selectable(on, library_label(e)));
                        if r.clicked() && !on {
                            pick = Some(e.source.clone());
                        }
                    }
                }
            });
            // Opening the list reads the folder again: a file saved outside
            // the app shows up without a remount. Keep (never overwrite)
            // the `true` the Refresh button just set.
            reread |= cb.response.clicked();
            super::fields::named(cb.response, "Machine library");
        });
        ui.end_row();
    });
    if reread {
        state.machines_pane.list = None;
    }
    match pick {
        Some(MachineSource::Builtin(name)) => state.pending.push_back(CommandKind::LoadBuiltinMachine(name.to_owned())),
        Some(source) => state.pending.push_back(CommandKind::UseCatalogueMachine(source)),
        None => {}
    }
}

/// Load other / Edit / Clear as equal buttons; with no machine, the one
/// load button. #1431: a refused `.mch` load slot greys with its reason;
/// since #2950 the browser's picker loads one, so nothing is refused, and
/// the browser adds a folder button under the row.
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
    // #2950: a page has no machine or post folder; the browser picks one.
    if state.browser_build {
        let size = Vec2::new(cell_w(ui, avail, 1), BUTTON_H);
        if crate::browser_files::button_sized(ui, None, LOAD_FOLDER, FOLDER_HELP, size) {
            state.pending.push_back(CommandKind::LoadSetupFolderDialog);
        }
    }
}

/// The browser's folder button (#2950).
pub(crate) const LOAD_FOLDER: &str = "Load folder (.mch, .cps)";
const FOLDER_HELP: &str = "Add every machine and post file of one folder to this session's lists";
