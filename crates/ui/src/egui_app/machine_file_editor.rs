//! The Machines pane's file editor (issue #383): one machine file as a
//! property grid in collapsible groups, each field's key in its tooltip
//! and its refusals under it; Save / Save As with their own
//! confirmation; the save blocked while any refusal stands.
//!
//! Provenance: the workflow is specced from the issue (new from
//! template, new blank, edit, Save / Save As to the user machines
//! folder, validation blocks the save); every word and widget is ours.
//! The draft, the grid, the rules and the write live in the app layer
//! (`camcraft_app::machine::{draft, fields, store, validate}`): this
//! file draws what they answer and calls them on a click.
//!
//! Editing a machine FILE is a save with its own confirmation, not a
//! document edit: nothing here queues a command or touches the history
//! (#287 rules). The editor's state is view state like the pane's. It
//! is not the Setup pane's machine window (`machine_editor`), which
//! edits the MOUNTED machine through the undoable `EditMachineApply`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Label, RichText, TextEdit, Ui};

use camcraft_app::machine::draft::MachineDraft;
use camcraft_app::machine::fields;
use camcraft_app::machine::store::SaveError;

use super::layout::{group_open, prop_grid};
use super::theme::Tokens;

/// The confirmation step a save is waiting on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prompt {
    /// Save over the file the draft came from.
    Save(PathBuf),
    /// Pick a file name in the user machines folder.
    SaveAs,
    /// Save As found the name taken: replace it?
    Replace(String),
    /// Close with unsaved edits?
    Discard,
}

/// One machine open in the editor.
pub struct MachineFileEditor {
    pub draft: MachineDraft,
    /// The typed text per field pointer, so a half-typed number (`1.`)
    /// stays as typed while the file holds what it reads as.
    pub buffers: HashMap<String, String>,
    /// The Save As file name being typed.
    pub save_as_name: String,
    /// The new coolant's name being typed.
    pub new_coolant: String,
    pub prompt: Option<Prompt>,
    /// The last save's outcome: (it worked, the words).
    pub status: Option<(bool, String)>,
}

/// What a frame of the editor asks of the pane.
#[derive(Default)]
pub struct EditorOut {
    /// A file was written: re-read the catalogue and select it.
    pub saved: Option<PathBuf>,
    /// The editor closed.
    pub close: bool,
}

impl MachineFileEditor {
    #[must_use]
    pub fn new(draft: MachineDraft) -> Self {
        let save_as_name = draft.suggested_name();
        Self { draft, buffers: HashMap::new(), save_as_name, new_coolant: String::new(), prompt: None, status: None }
    }
}

/// The verdict line for the draft.
pub(crate) fn verdict(n: usize) -> String {
    match n {
        0 => "Validation: passes".to_owned(),
        1 => "Validation: 1 refusal - save blocked".to_owned(),
        n => format!("Validation: {n} refusals - save blocked"),
    }
}

fn saved_words(p: &Path) -> String {
    format!("Saved {}", p.display())
}

/// Run one save and turn its answer into the editor's state.
fn finish(ed: &mut MachineFileEditor, out: &mut EditorOut, r: Result<PathBuf, SaveError>) {
    ed.prompt = None;
    match r {
        Ok(p) => {
            ed.status = Some((true, saved_words(&p)));
            out.saved = Some(p);
        }
        Err(SaveError::Exists(p)) => {
            ed.prompt = Some(Prompt::Replace(p.display().to_string()));
        }
        Err(e) => ed.status = Some((false, e.to_string())),
    }
}

fn prompt(ui: &mut Ui, ed: &mut MachineFileEditor, dir: Option<&Path>, out: &mut EditorOut, pal: &Tokens) {
    let Some(p) = ed.prompt.clone() else { return };
    ui.group(|ui| match p {
        Prompt::Save(path) => {
            ui.add(Label::new(format!("Write {}?", path.display())).wrap());
            ui.horizontal(|ui| {
                if ui.button("Write file").clicked() {
                    let r = ed.draft.save();
                    finish(ed, out, r);
                }
                if ui.button("Cancel").clicked() {
                    ed.prompt = None;
                }
            });
        }
        Prompt::SaveAs => {
            let where_ = dir.map_or_else(|| "no user machines folder".to_owned(), |d| d.display().to_string());
            ui.add(Label::new(RichText::new(format!("Save to {where_}")).color(pal.text_dim)).wrap());
            ui.horizontal(|ui| {
                let l = ui.label("File name");
                ui.add(TextEdit::singleline(&mut ed.save_as_name).desired_width(160.0))
                    .labelled_by(l.id)
                    .on_hover_text("A plain name; .mch is added");
            });
            ui.horizontal(|ui| {
                if ui.button("Write file").clicked() {
                    let name = ed.save_as_name.clone();
                    let r = ed.draft.save_as(dir, &name, false);
                    finish(ed, out, r);
                }
                if ui.button("Cancel").clicked() {
                    ed.prompt = None;
                }
            });
        }
        Prompt::Replace(path) => {
            ui.add(Label::new(format!("{path} exists. Replace it?")).wrap());
            ui.horizontal(|ui| {
                if ui.button("Replace file").clicked() {
                    let name = ed.save_as_name.clone();
                    let r = ed.draft.save_as(dir, &name, true);
                    finish(ed, out, r);
                }
                if ui.button("Cancel").clicked() {
                    ed.prompt = Some(Prompt::SaveAs);
                }
            });
        }
        Prompt::Discard => {
            ui.label("Discard the unsaved edits?");
            ui.horizontal(|ui| {
                if ui.button("Discard edits").clicked() {
                    out.close = true;
                }
                if ui.button("Keep editing").clicked() {
                    ed.prompt = None;
                }
            });
        }
    });
}

/// Draw the editor. `dir` is the user machines folder Save As writes to.
pub(crate) fn draw(ui: &mut Ui, ed: &mut MachineFileEditor, dir: Option<&Path>, pal: &Tokens, error: egui::Color32) -> EditorOut {
    let mut out = EditorOut::default();
    let refusals = ed.draft.refusals();
    let groups = ed.draft.groups();

    let title = if ed.draft.model().is_empty() { "(no model)".to_owned() } else { ed.draft.model() };
    ui.add(Label::new(RichText::new(format!("Editing {title}{}", if ed.draft.dirty { " *" } else { "" })).strong()).wrap());
    let at = match (&ed.draft.path, ed.draft.read_only) {
        (_, true) => "Shipped machine - read-only; Save As writes your own copy".to_owned(),
        (Some(p), false) => p.display().to_string(),
        (None, false) => "Not saved yet".to_owned(),
    };
    ui.add(Label::new(RichText::new(at).small().color(pal.text_dim)).wrap());

    ui.horizontal_wrapped(|ui| {
        let blocked = !refusals.is_empty();
        let save = ui
            .add_enabled(!blocked, egui::Button::new("Save machine"))
            .on_hover_text("Write this machine file (asks first)")
            .on_disabled_hover_text("Fix every refusal first - a refused file is never written");
        if save.clicked() {
            ed.prompt = Some(match (&ed.draft.path, ed.draft.read_only) {
                (Some(p), false) => Prompt::Save(p.clone()),
                _ => Prompt::SaveAs,
            });
        }
        let save_as = ui
            .add_enabled(!blocked, egui::Button::new("Save machine as..."))
            .on_hover_text("Write a copy to your machines folder")
            .on_disabled_hover_text("Fix every refusal first - a refused file is never written");
        if save_as.clicked() {
            ed.prompt = Some(Prompt::SaveAs);
        }
        if ui.button("Close editor").on_hover_text("Leave the editor").clicked() {
            if ed.draft.dirty {
                ed.prompt = Some(Prompt::Discard);
            } else {
                out.close = true;
            }
        }
    });
    prompt(ui, ed, dir, &mut out, pal);
    if let Some((ok, words)) = &ed.status {
        let ink = if *ok { pal.accent } else { error };
        ui.add(Label::new(RichText::new(words.as_str()).color(ink)).wrap());
    }

    let ink = if refusals.is_empty() { pal.accent } else { error };
    ui.label(RichText::new(verdict(refusals.len())).color(ink).strong());
    for r in fields::unplaced(&groups, &refusals) {
        super::error_text::error_text(ui, error, &r.to_string());
    }
    ui.add_space(4.0);

    let mut edits: Vec<(String, fields::FieldKind, String)> = Vec::new();
    let mut add_coolant = false;
    for g in &groups {
        group_open(ui, &g.title, None, true, |ui| {
            if g.fields.is_empty() {
                ui.label(RichText::new("none stated").color(pal.text_dim));
            }
            prop_grid(ui, &format!("machine-edit-{}", g.title), |ui| {
                for f in &g.fields {
                    let tip = format!("Key: {}", f.key);
                    let l = ui.label(&f.label).on_hover_text(&tip);
                    let buf = ed.buffers.entry(f.pointer.clone()).or_insert_with(|| f.value.clone());
                    let r = ui
                        .add(TextEdit::singleline(buf).desired_width(140.0).hint_text("not stated"))
                        .labelled_by(l.id)
                        .on_hover_text(&tip);
                    if r.changed() {
                        edits.push((f.pointer.clone(), f.kind, buf.clone()));
                    }
                    ui.end_row();
                    for refusal in f.refusals(&refusals) {
                        ui.label("");
                        super::error_text::error_text(ui, error, &refusal.to_string());
                        ui.end_row();
                    }
                }
            });
            if g.title == "Coolant codes" {
                ui.horizontal(|ui| {
                    let l = ui.label("New coolant");
                    ui.add(TextEdit::singleline(&mut ed.new_coolant).desired_width(100.0)).labelled_by(l.id);
                    if ui.button("Add coolant").on_hover_text("Add a named coolant to the tool station").clicked() {
                        add_coolant = true;
                    }
                });
            }
        });
    }
    for (pointer, kind, text) in edits {
        if let Err(e) = ed.draft.set(&pointer, kind, &text) {
            ed.status = Some((false, e));
        }
    }
    if add_coolant {
        let name = std::mem::take(&mut ed.new_coolant);
        if let Err(e) = ed.draft.add_coolant(&name) {
            ed.status = Some((false, e));
        }
    }
    out
}
