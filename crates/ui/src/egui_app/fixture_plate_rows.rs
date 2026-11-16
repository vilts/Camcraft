//! A fixture row's plate lines (#819): a plate shows its grid and what
//! stands on it; an imported fixture offers "Detect plate grid"; any
//! fixture, while a plate exists, gets the grid-location picker - pick
//! the plate, a row letter and a column number (or a dowel pair) and a
//! turn, then "Place on grid" queues one `PlaceOnPlate`. A fixture on a
//! plate says where it stands.
//! Provenance: our own layout over our own plate commands.

use egui::Ui;

use camcraft_app::plate::{self, mount};
use camcraft_app::{fixture, model, units};

use crate::state::{CommandKind, State};

use super::fields::{named, num_plain_by};

/// The plate lines of fixture `name`'s row.
pub(super) fn plate_rows(ui: &mut Ui, state: &mut State, name: &str) {
    let Some(def) = model::find_by_name(&state.world, name).and_then(|e| fixture::def_of(&state.world, &e)) else {
        return;
    };
    let u = state.units();
    if let Some(p) = &def.plate.plate {
        let g = &p.holes;
        let mut line = format!(
            "Fixture plate: {}, pitch {} x {}",
            g.extent(),
            units::len(g.pitch_x, u, 3),
            units::len(g.pitch_y, u, 3)
        );
        if g.stagger.is_some() {
            line.push_str(", staggered");
        }
        if !g.missing.is_empty() {
            line.push_str(&format!(", {} gaps", g.missing.len()));
        }
        ui.label(line);
        if let Some(d) = &p.dowels {
            ui.label(format!("Dowels: {}, pitch {}", d.extent(), units::len(d.pitch_x, u, 3)));
        }
        let on = mount::riders(&state.world, name);
        if !on.is_empty() {
            ui.label(format!("Carries: {}", on.join(", ")));
        }
        super::fixture_modvise_rows::pair_rows(ui, state, name, p);
        if p.params.is_none() && ui.small_button("Not a plate").on_hover_text("Forget the detected grid").clicked() {
            state.pending.push_back(CommandKind::DetectPlateGrid { name: name.to_owned(), clear: true });
        }
    } else if def.vise.is_none()
        && def.mod_jaw.is_none()
        && ui
            .small_button("Detect plate grid")
            .on_hover_text("Use this fixture as a grid plate: its holes (on its own Z) must form a regular grid")
            .clicked()
    {
        state.pending.push_back(CommandKind::DetectPlateGrid { name: name.to_owned(), clear: false });
    }
    super::fixture_modvise_rows::block_line(ui, state, name);
    if let Some(m) = &def.plate.mount {
        ui.label(format!("On plate {} at {}", m.plate, m.at.label()));
    }
    place_picker(ui, state, name);
}

/// The grid-location picker, when some other fixture is a plate.
fn place_picker(ui: &mut Ui, state: &mut State, name: &str) {
    let plates = state.plate_names(name);
    if plates.is_empty() {
        return;
    }
    let mut draft = state.plate_place_draft(name).clone();
    if !plates.contains(&draft.plate) {
        draft.plate = plates[0].clone();
    }
    let def = model::find_by_name(&state.world, &draft.plate)
        .and_then(|e| fixture::def_of(&state.world, &e))
        .and_then(|d| d.plate.plate);
    let Some(def) = def else { return };
    let mut go = false;
    egui::CollapsingHeader::new("Place on plate…").id_salt(("plate-place", name)).show(ui, |ui| {
        let pick = egui::ComboBox::from_id_salt(("plate-pick", name)).selected_text(draft.plate.clone()).show_ui(ui, |ui| {
            for p in &plates {
                ui.selectable_value(&mut draft.plate, p.clone(), p);
            }
        });
        named(pick.response, "Plate to place on");
        ui.horizontal(|ui| {
            ui.radio_value(&mut draft.dowels, false, "Hole");
            if def.dowels.is_some() {
                ui.radio_value(&mut draft.dowels, true, "Dowel pair");
            }
        });
        if draft.dowels {
            ui.horizontal(|ui| {
                let l = ui.label("Dowels");
                ui.add(egui::TextEdit::singleline(&mut draft.dowel_a).desired_width(40.0)).labelled_by(l.id);
                ui.add(egui::TextEdit::singleline(&mut draft.dowel_b).desired_width(40.0)).labelled_by(l.id);
            });
        } else {
            let g = &def.holes;
            draft.row = draft.row.min(g.rows.saturating_sub(1));
            draft.col = draft.col.min(g.cols.saturating_sub(1));
            ui.horizontal(|ui| {
                let l = ui.label("Row");
                egui::ComboBox::from_id_salt(("plate-row", name)).selected_text(plate::row_letters(draft.row)).show_ui(ui, |ui| {
                    for r in 0..g.rows {
                        ui.selectable_value(&mut draft.row, r, plate::row_letters(r));
                    }
                })
                .response
                .labelled_by(l.id);
                let l = ui.label("Column");
                egui::ComboBox::from_id_salt(("plate-col", name)).selected_text((draft.col + 1).to_string()).show_ui(ui, |ui| {
                    for c in 0..g.cols {
                        ui.selectable_value(&mut draft.col, c, (c + 1).to_string());
                    }
                })
                .response
                .labelled_by(l.id);
            });
        }
        ui.horizontal(|ui| {
            let l = ui.label("Turn (deg)");
            num_plain_by(ui, &mut draft.yaw_deg, l.id);
        });
        go = ui.button("Place on grid").clicked();
    });
    if go {
        state.pending.push_back(CommandKind::PlaceOnPlate {
            fixture: name.to_owned(),
            plate: draft.plate.clone(),
            at: draft.at(),
            yaw_deg: draft.yaw_deg,
        });
    }
    *state.plate_place_draft(name) = draft;
}
