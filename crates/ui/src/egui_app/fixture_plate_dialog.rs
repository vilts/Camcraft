//! The "Add fixture plate…" dialog body (#819), drawn inside the fixture
//! library's modal: pick a preset, edit any figure to match the shop's
//! plate (size, grid pitch and first-hole offset, thread, an optional
//! dowel pattern and counterbores), then lay it centred on the machine
//! table or type where its frame goes. Place queues one undoable
//! `InsertPlate` through the command queue.
//! Provenance: our own layout over our own plate commands.

use egui::{RichText, Ui};

use camcraft_app::plate;
use camcraft_app::units::Unit;

use crate::state::{CommandKind, PlateDraft};

use super::fields::{num, num_plain};
use super::fixture_vise_dialog::ViseAct;

/// The Place button's command for this draft.
#[must_use]
pub(crate) fn place_command(d: &PlateDraft) -> CommandKind {
    CommandKind::InsertFixturePlate {
        setup: (!d.setup.is_empty()).then(|| d.setup.clone()),
        params: d.landed(),
        on_table: d.on_table,
        pos: d.pos,
        yaw_deg: d.yaw_deg,
    }
}

fn grid(ui: &mut Ui, id: &str, u: Unit, rows: Vec<(&str, &mut f64)>) {
    egui::Grid::new(id).num_columns(2).show(ui, |ui| {
        for (label, v) in rows {
            ui.label(label);
            num(ui, u, v);
            ui.end_row();
        }
    });
}

pub(crate) fn body(ui: &mut Ui, u: Unit, d: &mut PlateDraft) -> Option<ViseAct> {
    let mut act = None;
    ui.heading("Add fixture plate");
    ui.label(
        RichText::new(
            "A generic grid plate; edit the figures to match yours. Holes are named by row letter (A, B, ... along +Y) \
             and column number (1, 2, ... along +X); A1 is nearest the plate's origin corner.",
        )
        .small(),
    );
    ui.add_space(4.0);
    ui.label("Preset");
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for (k, row) in plate::PRESETS.iter().enumerate() {
            if ui.selectable_label(d.preset == k, row.label).clicked() {
                picked = Some(k);
            }
        }
    });
    if let Some(k) = picked {
        d.pick(k);
    }
    let p = &mut d.params;
    grid(
        ui,
        "plate-dims",
        u,
        vec![
            ("Length (X)", &mut p.length),
            ("Width (Y)", &mut p.width),
            ("Thickness", &mut p.thickness),
            ("Pitch X", &mut p.pitch_x),
            ("Pitch Y", &mut p.pitch_y),
            ("First hole X", &mut p.offset_x),
            ("First hole Y", &mut p.offset_y),
            ("Thread diameter", &mut p.thread_diameter),
        ],
    );
    ui.horizontal(|ui| {
        ui.label("Thread");
        ui.add(egui::TextEdit::singleline(&mut p.thread).desired_width(80.0));
    });
    ui.checkbox(&mut d.dowels_on, "Dowel holes");
    if d.dowels_on {
        let w = &mut d.dowels;
        grid(
            ui,
            "plate-dowels",
            u,
            vec![
                ("Dowel pitch X", &mut w.pitch_x),
                ("Dowel pitch Y", &mut w.pitch_y),
                ("First dowel X", &mut w.offset_x),
                ("First dowel Y", &mut w.offset_y),
                ("Dowel diameter", &mut w.diameter),
            ],
        );
    }
    ui.checkbox(&mut d.counterbore_on, "Counterbored holes");
    if d.counterbore_on {
        let c = &mut d.counterbore;
        grid(ui, "plate-cbore", u, vec![("Counterbore diameter", &mut c.diameter), ("Counterbore depth", &mut c.depth)]);
    }
    ui.checkbox(&mut d.on_table, "Lay it on the machine table (centred)");
    if !d.on_table {
        egui::Grid::new("plate-place").num_columns(2).show(ui, |ui| {
            for (k, axis) in ["Corner X", "Corner Y", "Top Z"].iter().enumerate() {
                ui.label(*axis);
                num(ui, u, &mut d.pos[k]);
                ui.end_row();
            }
            ui.label("Turn about Z (yaw)");
            num_plain(ui, &mut d.yaw_deg);
            ui.end_row();
        });
    }
    ui.label(RichText::new("The collision check sees the plate as a solid slab (holes drawn solid).").small());
    if let Some(err) = &d.error {
        super::error_text::error_text(ui, ui.visuals().error_fg_color, err);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Place").clicked() {
            act = Some(ViseAct::Place(Box::new(place_command(d))));
        }
        if ui.button("Cancel").clicked() {
            act = Some(ViseAct::Close);
        }
    });
    act
}
