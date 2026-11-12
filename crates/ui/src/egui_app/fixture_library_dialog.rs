//! The fixture library dialogs (#339 slice 6), opened from a setup's
//! Fixtures node menu: "Save fixture to library" (check the setup's
//! fixtures, pick the datum, add a note; a name already in the library
//! asks before it is replaced) and "Insert from library" (pick an entry,
//! type the datum's origin and rotation, Insert - one undoable
//! `InsertFixture` through the queue; Delete removes the entry from the
//! library file after a confirmation). A modal like the unsaved prompt;
//! Escape or the backdrop closes it. "Add built-in vise…" (#788) shares
//! the modal; its body is `fixture_vise_dialog`. So does "Add
//! accessory…" (#789, `fixture_accessory_dialog`).
//! Provenance: our own layout over our own library format; no vendor
//! fixture material consulted.

use egui::{Id, Modal, RichText, ScrollArea, TextEdit};

use camcraft_app::fixture_library::DatumChoice;

use crate::state::{CommandKind, FixtureLibDialog, InsertFixtureDraft, State};

use super::fields::{num, num_plain};
use super::fixture_accessory_dialog::AccessoryAct;
use super::fixture_vise_dialog::ViseAct;

/// What a click asked for; applied after the dialog is drawn.
enum Act {
    Close,
    Save { replace: bool },
    Insert(Box<CommandKind>),
    Delete(String),
    /// Close the dialog and open the Stock page of this setup.
    OpenStock(String),
}

/// The Insert button's command for this draft (`None`: nothing picked).
#[must_use]
pub(crate) fn insert_command(d: &InsertFixtureDraft) -> Option<CommandKind> {
    let entry = d.selected.clone()?;
    Some(CommandKind::InsertFixtureFromLibrary {
        entry,
        setup: (!d.setup.is_empty()).then(|| d.setup.clone()),
        pos: d.pos,
        rpy_deg: d.rpy_deg,
    })
}

fn setup_label(setup: &str) -> &str {
    if setup.is_empty() { "the setup" } else { setup }
}

pub(crate) fn window(ctx: &egui::Context, state: &mut State) {
    let u = state.units();
    let path = state.fixture_library_path().map(|p| p.display().to_string()).unwrap_or_else(|e| e);
    let Some(dialog) = state.fixture_lib.as_mut() else {
        return;
    };
    if let FixtureLibDialog::Vise(d) = dialog {
        let setup = (!d.setup.is_empty()).then_some(d.setup.as_str());
        d.fit = Some(camcraft_app::vise_fit::preview(&state.world, &d.params, setup));
    }
    let mut act: Option<Act> = None;
    let response = Modal::new(Id::new("fixture-library")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        match dialog {
            FixtureLibDialog::Save(d) => {
                ui.heading("Save fixture to library");
                ui.label(RichText::new(&path).small());
                ui.add_space(4.0);
                if d.fixtures.is_empty() {
                    ui.label(format!("{} has no fixtures to save.", setup_label(&d.setup)));
                }
                for (name, on) in &mut d.fixtures {
                    ui.checkbox(on, name.as_str());
                }
                ui.label("Placed by");
                ui.radio_value(&mut d.datum, DatumChoice::ModelOrigin, "Model origin");
                ui.radio_value(&mut d.datum, DatumChoice::BottomCentre, "Bottom centre");
                ui.label("Note");
                ui.add(TextEdit::singleline(&mut d.note).desired_width(320.0));
                if let Some(err) = &d.error {
                    super::error_text::error_text(ui, ui.visuals().error_fg_color, err);
                }
                ui.add_space(8.0);
                if let Some(taken) = &d.confirm_replace {
                    ui.label(format!("Replace {} in the library?", taken.join(", ")));
                    ui.horizontal(|ui| {
                        if ui.button("Replace").clicked() {
                            act = Some(Act::Save { replace: true });
                        }
                        if ui.button("Cancel").clicked() {
                            act = Some(Act::Close);
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            act = Some(Act::Save { replace: false });
                        }
                        if ui.button("Cancel").clicked() {
                            act = Some(Act::Close);
                        }
                    });
                }
            }
            FixtureLibDialog::Plate(d) => {
                act = super::fixture_plate_dialog::body(ui, u, d).map(|a| match a {
                    ViseAct::Close => Act::Close,
                    ViseAct::Place(cmd) => Act::Insert(cmd),
                    ViseAct::OpenStock => Act::Close,
                });
            }
            FixtureLibDialog::Vise(d) => {
                act = super::fixture_vise_dialog::body(ui, u, d).map(|a| match a {
                    ViseAct::Close => Act::Close,
                    ViseAct::Place(cmd) => Act::Insert(cmd),
                    ViseAct::OpenStock => Act::OpenStock(d.setup.clone()),
                });
            }
            FixtureLibDialog::Accessory(d) => {
                act = super::fixture_accessory_dialog::body(ui, u, d).map(|a| match a {
                    AccessoryAct::Close => Act::Close,
                    AccessoryAct::Place(cmd) => Act::Insert(cmd),
                });
            }
            FixtureLibDialog::Insert(d) => {
                ui.heading("Insert from library");
                ui.label(RichText::new(&path).small());
                ui.add_space(4.0);
                if d.rows.is_empty() && d.error.is_none() {
                    ui.label("The library is empty - save a fixture to it first.");
                }
                ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    for r in &d.rows {
                        let text = match &r.note {
                            Some(n) => format!("{} - {} facets - {n}", r.name, r.facets),
                            None => format!("{} - {} facets", r.name, r.facets),
                        };
                        if ui.selectable_label(d.selected.as_ref() == Some(&r.name), text).clicked() {
                            d.selected = Some(r.name.clone());
                            d.confirm_delete = None;
                        }
                    }
                });
                ui.label(format!("Datum at (into {})", setup_label(&d.setup)));
                egui::Grid::new("fixture-lib-datum").num_columns(2).show(ui, |ui| {
                    for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
                        ui.label(*axis);
                        num(ui, u, &mut d.pos[k]);
                        ui.end_row();
                    }
                    for (k, axis) in ["Tilt about X (roll)", "Tilt about Y (pitch)", "Turn about Z (yaw)"].iter().enumerate() {
                        ui.label(*axis);
                        num_plain(ui, &mut d.rpy_deg[k]);
                        ui.end_row();
                    }
                });
                if let Some(err) = &d.error {
                    super::error_text::error_text(ui, ui.visuals().error_fg_color, err);
                }
                ui.add_space(8.0);
                if let Some(name) = d.confirm_delete.clone() {
                    ui.label(format!("Delete '{name}' from the library? This edits the file and cannot be undone."));
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            act = Some(Act::Delete(name.clone()));
                        }
                        if ui.button("Keep").clicked() {
                            d.confirm_delete = None;
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        let cmd = insert_command(d);
                        if ui.add_enabled(cmd.is_some(), egui::Button::new("Insert")).clicked()
                            && let Some(cmd) = cmd
                        {
                            act = Some(Act::Insert(Box::new(cmd)));
                        }
                        if ui.add_enabled(d.selected.is_some(), egui::Button::new("Delete from library…")).clicked() {
                            d.confirm_delete = d.selected.clone();
                        }
                        if ui.button("Close").clicked() {
                            act = Some(Act::Close);
                        }
                    });
                }
            }
        }
    });
    if act.is_none() && response.should_close() {
        act = Some(Act::Close);
    }
    match act {
        Some(Act::Close) => state.fixture_lib = None,
        Some(Act::Save { replace }) => state.save_fixtures_to_library(replace),
        Some(Act::Insert(cmd)) => state.pending.push_back(*cmd),
        Some(Act::Delete(name)) => state.delete_library_entry(&name),
        Some(Act::OpenStock(setup)) => {
            // The same path as the browser's Stock row.
            state.fixture_lib = None;
            super::browser_pane::click(state, super::browser_tree::Leaf::Stock(setup));
        }
        None => {}
    }
}
