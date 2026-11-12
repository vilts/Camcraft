//! The Machines pane (issue #287, first slice): a dockable, read-only
//! browser of every machine definition the app can name - the shipped
//! library and the document's own `.mch` file - with each machine's
//! grouped properties and its validation, refusal by refusal.
//!
//! Provenance: the workflow is specced from the issue (list, inspect,
//! validate machine configurations); every word and widget here is
//! ours. The catalogue, the sheet and the rules live in the app layer
//! (`camcraft_app::machine::library` / `validate`): this pane only draws
//! what they answer - it reads no file itself.
//!
//! View state only: which row is selected and the cached catalogue are
//! ephemeral like the dock (never a command, never saved). #383 adds
//! the file editor (`machine_file_editor`): New blank, New from
//! template and Edit open a machine FILE for editing, saved with its
//! own confirmation - still no command, no history. The one document
//! change the pane makes is "Use for this setup" (#384): it only queues
//! `CommandKind::UseCatalogueMachine`, routed like every machine verb
//! (#343): the undoable `SetSetupMachine` when the active setup owns its
//! machine, else `LoadMachine`; a refused or already-mounted machine
//! leaves the button disabled with the reason beside it.

use std::path::PathBuf;

use egui::{Label, RichText, ScrollArea, Ui};

use camcraft_app::machine::draft::MachineDraft;
use camcraft_app::machine::library::{self, MachineEntry, MachineSource};
use camcraft_app::machine::{store, Kind};

use super::machine_file_editor::{self as file_editor, MachineFileEditor};

use super::colorvision::text_inks;
use super::layout::{group_open, prop_grid};
use super::theme::Tokens;
use crate::state::{CommandKind, State};

/// The pane's view state.
#[derive(Default)]
pub struct MachinesPane {
    /// The catalogue as last read; `None` = read it on the next draw.
    pub list: Option<Vec<MachineEntry>>,
    /// The document's machine path the catalogue was read for - a new
    /// mount re-reads it (the document's file is one of the entries).
    pub listed_for: String,
    /// The selected row, an index into `list`.
    pub selected: Option<usize>,
    /// The user machines folder: `None` = not resolved yet (resolved
    /// once, `store::user_dir`); `Some(None)` = there is none. Tests
    /// point it at a temp folder.
    pub user_dir: Option<Option<PathBuf>>,
    /// The machine file open in the editor, if any.
    pub editor: Option<MachineFileEditor>,
    /// The kind filter (#816): `None` = "All", else an index into
    /// `KINDS`. Filters the list and picks the kind New blank starts;
    /// under "All" New blank is disabled.
    pub kind_filter: Option<usize>,
}

/// The kinds New blank offers, in the combo's order.
const KINDS: [Kind; 4] = [Kind::Mill, Kind::Lathe, Kind::MillTurn, Kind::Plasma];

/// The dropdown's first entry: no filter, New blank shut.
pub(crate) const ALL_KINDS: &str = "All";

/// Does the entry show under the kind filter? A machine whose file
/// states no kind shows only under "All".
pub(crate) fn matches_filter(e: &MachineEntry, want: Option<Kind>) -> bool {
    want.is_none_or(|k| e.kind == Some(k))
}

/// A list row's words: name, then kind, axes and units - unique per
/// entry in practice, and what a test finds the row by.
pub(crate) fn row_label(e: &MachineEntry) -> String {
    let kind = e.kind.map_or("?", |k| k.word());
    let mut axes: String = e.axes.concat();
    axes.push_str(&e.rotaries.concat());
    if axes.is_empty() {
        axes.push('?');
    }
    let mark = if e.is_valid() { "" } else { "  [refused]" };
    format!("{}  ({kind}, {axes}, {}){mark}", e.name, e.units)
}

/// The validation heading for an entry.
pub(crate) fn verdict(e: &MachineEntry) -> String {
    match e.refusals.len() {
        0 => "Validation: passes".to_owned(),
        1 => "Validation: 1 refusal".to_owned(),
        n => format!("Validation: {n} refusals"),
    }
}

/// Make sure the catalogue is current: read once, and again when the
/// document mounts a different machine.
fn refresh(state: &mut State) {
    let (path, _) = camcraft_app::machine::of(&state.world);
    let p = &mut state.machines_pane;
    let dir = p.user_dir.get_or_insert_with(store::user_dir).clone();
    if p.list.is_none() || p.listed_for != path {
        let list = library::enumerate_with(&state.world, dir.as_deref());
        if p.selected.is_some_and(|i| i >= list.len()) {
            p.selected = None;
        }
        p.list = Some(list);
        p.listed_for = path;
    }
}

/// The pane's assign button label (a test finds it by these words).
pub(crate) const USE_FOR_SETUP: &str = "Use for this setup";

pub(crate) fn draw(ui: &mut Ui, state: &mut State, pal: &Tokens) {
    refresh(state);
    let inks = text_inks(ui, &state.world);
    // Why the selected machine can't be assigned (read before the pane
    // borrow below takes `state`).
    let block = state
        .machines_pane
        .selected
        .and_then(|i| state.machines_pane.list.as_ref().and_then(|l| l.get(i)))
        .map(|e| e.assign_block(&state.world));
    let mut assign = None;
    let p = &mut state.machines_pane;
    let Some(list) = p.list.as_ref() else { return };
    let mut reread = false;
    let want = p.kind_filter.and_then(|i| KINDS.get(i)).copied();
    let shown = list.iter().filter(|e| matches_filter(e, want)).count();
    ui.horizontal(|ui| {
        let count = if want.is_some() {
            format!("{shown} of {} machines", list.len())
        } else {
            format!("{} machines", list.len())
        };
        ui.label(RichText::new(count).color(pal.text_dim));
        if ui
            .button("Refresh")
            .on_hover_text("Read the machine files again")
            .clicked()
        {
            reread = true;
        }
    });
    let picked = p.selected.and_then(|i| list.get(i)).map(|e| e.source.clone());
    let mut open: Option<Result<MachineDraft, String>> = None;
    if p.editor.is_none() {
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("machines-new-kind")
                .selected_text(want.map_or(ALL_KINDS, |k| k.word()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut p.kind_filter, None, ALL_KINDS);
                    for (i, k) in KINDS.iter().enumerate() {
                        ui.selectable_value(&mut p.kind_filter, Some(i), k.word());
                    }
                });
            // A blank file needs a kind: shut under "All", accent-filled
            // once a kind is picked.
            let mut blank = egui::Button::new("New blank");
            if want.is_some() {
                blank = blank.fill(pal.accent);
            }
            if ui
                .add_enabled(want.is_some(), blank)
                .on_hover_text("A new machine file with the keys its kind requires")
                .on_disabled_hover_text("Pick a kind first")
                .clicked()
                && let Some(kind) = want
            {
                open = Some(Ok(MachineDraft::blank(kind)));
            }
            let has = picked.is_some();
            if ui
                .add_enabled(has, egui::Button::new("New from template"))
                .on_hover_text("A new machine file copied from the selected machine")
                .on_disabled_hover_text("Select a machine to copy")
                .clicked()
                && let Some(src) = &picked
            {
                open = Some(MachineDraft::from_template(src).map_err(|r| r.to_string()));
            }
            if ui
                .add_enabled(has, egui::Button::new("Edit machine"))
                .on_hover_text("Edit the selected machine file (a shipped one saves as a copy)")
                .on_disabled_hover_text("Select a machine to edit")
                .clicked()
                && let Some(src) = &picked
            {
                open = Some(MachineDraft::open(src).map_err(|r| r.to_string()));
            }
        });
    }
    match open {
        Some(Ok(d)) => p.editor = Some(MachineFileEditor::new(d)),
        Some(Err(e)) => {
            super::error_text::error_text(ui, inks.error, &format!("Cannot open: {e}"));
        }
        None => {}
    }
    ui.separator();
    let mut chose = None;
    ScrollArea::vertical()
        .id_salt("machines-list")
        .max_height((ui.available_height() * 0.4).max(80.0))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if shown == 0 {
                ui.label(RichText::new("No machines of this kind.").color(pal.text_dim));
            }
            for (i, e) in list.iter().enumerate() {
                if !matches_filter(e, want) {
                    continue;
                }
                let r = ui
                    .selectable_label(p.selected == Some(i), row_label(e))
                    .on_hover_text(e.source.label());
                if r.clicked() {
                    chose = Some(i);
                }
            }
        });
    if let Some(i) = chose {
        p.selected = Some(i);
    }
    // A selection the filter hides is dropped, so no hidden row stays
    // the target of Edit / Use for this setup.
    if p.selected
        .and_then(|i| list.get(i))
        .is_some_and(|e| !matches_filter(e, want))
    {
        p.selected = None;
    }
    ui.separator();
    if let Some(ed) = p.editor.as_mut() {
        let dir = p.user_dir.clone().flatten();
        let out = ScrollArea::vertical()
            .id_salt("machines-editor")
            .show(ui, |ui| file_editor::draw(ui, ed, dir.as_deref(), pal, inks.error))
            .inner;
        if out.close {
            p.editor = None;
        }
        if let Some(saved) = out.saved {
            // Re-read the catalogue and select the file just written.
            let world_list = library::enumerate_with(&state.world, dir.as_deref());
            let p = &mut state.machines_pane;
            p.selected = world_list
                .iter()
                .position(|e| matches!(&e.source, MachineSource::File(f) if *f == saved));
            p.list = Some(world_list);
        } else if reread {
            p.list = None;
        }
        return;
    }
    let Some(e) = p.selected.and_then(|i| list.get(i)) else {
        ui.label(RichText::new("Select a machine to see its properties.").color(pal.text_dim));
        if reread {
            p.list = None;
        }
        return;
    };
    ScrollArea::vertical().id_salt("machines-detail").show(ui, |ui| {
        ui.add(Label::new(RichText::new(&e.name).strong()).wrap());
        ui.add(Label::new(RichText::new(e.source.label()).small().color(pal.text_dim)).wrap());
        let ink = if e.is_valid() { pal.accent } else { inks.error };
        ui.label(RichText::new(verdict(e)).color(ink).strong());
        for r in &e.refusals {
            super::error_text::error_text(ui, inks.error, &r.to_string());
        }
        let why = block.clone().flatten();
        let r = ui.add_enabled(why.is_none(), egui::Button::new(USE_FOR_SETUP));
        match &why {
            None => {
                if r.on_hover_text("Make this the setup's machine (Ctrl+Z undoes it)").clicked() {
                    assign = Some(e.source.clone());
                }
            }
            Some(why) => {
                r.on_disabled_hover_text(format!("Cannot assign: {why}"));
                ui.add(Label::new(RichText::new(format!("Cannot assign: {why}")).color(pal.text_dim)).wrap());
            }
        }
        ui.add_space(4.0);
        for g in &e.groups {
            group_open(ui, &g.title, None, true, |ui| {
                prop_grid(ui, &format!("machine-{}", g.title), |ui| {
                    for (label, value) in &g.rows {
                        ui.label(label);
                        ui.add(Label::new(value).wrap());
                        ui.end_row();
                    }
                });
            });
        }
    });
    if reread {
        p.list = None;
    }
    if let Some(source) = assign {
        state.pending.push_back(CommandKind::UseCatalogueMachine(source));
    }
}
