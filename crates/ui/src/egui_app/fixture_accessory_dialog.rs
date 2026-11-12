//! The "Add accessory…" dialog body (#789), drawn inside the fixture
//! library's modal: pick a kind (parallels, 1-2-3 block, step block,
//! strap clamp, soft jaw), edit its figures, then type where its frame
//! goes - or, for parallels, fit them under the stock in one of the
//! setup's built-in vises. Place queues one undoable `InsertAccessory`.
//! Provenance: our own layout over our own accessory commands.

use egui::{RichText, Ui};

use camcraft_app::accessory::{self, Accessory};
use camcraft_app::units::Unit;

use crate::state::{AccessoryDraft, CommandKind};

use super::fields::{num, num_plain};

/// What the body's buttons asked for.
pub(crate) enum AccessoryAct {
    Close,
    Place(Box<CommandKind>),
}

/// The Place button's command for this draft.
#[must_use]
pub(crate) fn place_command(d: &AccessoryDraft) -> CommandKind {
    CommandKind::InsertAccessory {
        setup: (!d.setup.is_empty()).then(|| d.setup.clone()),
        params: d.params.clone(),
        placement: d.placement(),
    }
}

/// The label parallels' height takes while a vise fit sets it (#812).
pub(crate) const FIT_HEIGHT_LABEL: &str = "Height (set by the fit)";

/// What the fit does to the vise, said before Place (#812): the stock
/// stays put and the vise drops under it.
pub(crate) const FIT_SINKS_HINT: &str = "The fit keeps the stock where it is and drops the vise by the parallels' height: \
     with the stock resting on the vise floor, the vise ends below the table. Raise the stock first if the vise must stay on the table.";

/// The figure fields of `a` (and a step block's step count). With
/// `fitting`, parallels' height is the fit's: shown, labelled so and
/// not editable (#812).
pub(crate) fn dims_grid(ui: &mut Ui, u: Unit, id: &str, a: &mut Accessory, fitting: bool) {
    egui::Grid::new(id).num_columns(2).show(ui, |ui| {
        if let Some(steps) = a.steps_mut() {
            ui.label("Steps");
            ui.add(egui::DragValue::new(steps).range(1..=accessory::MAX_STEPS));
            ui.end_row();
        }
        for d in a.dims_mut() {
            if fitting && d.key == "height" {
                ui.add_enabled(false, egui::Label::new(FIT_HEIGHT_LABEL));
                ui.add_enabled_ui(false, |ui| num(ui, u, d.value));
            } else {
                ui.label(d.label);
                num(ui, u, d.value);
            }
            ui.end_row();
        }
    });
}

pub(crate) fn body(ui: &mut Ui, u: Unit, d: &mut AccessoryDraft) -> Option<AccessoryAct> {
    let mut act = None;
    ui.heading("Add accessory");
    ui.label(RichText::new("Generic workholding built from these figures; edit them to match yours.").small());
    ui.add_space(4.0);
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for kind in accessory::KINDS {
            if ui.selectable_label(d.params.kind() == kind, accessory::label_of(kind)).clicked() {
                picked = Some(kind);
            }
        }
    });
    if let Some(k) = picked {
        d.pick(k);
    }
    let parallels = matches!(d.params, Accessory::Parallels { .. });
    let fitting = parallels && d.in_vise.is_some();
    if parallels && !d.vises.is_empty() {
        let mut on = d.in_vise.is_some();
        if ui.checkbox(&mut on, "Under the stock in a vise (height set to fit)").changed() {
            d.in_vise = if on { d.vises.first().cloned() } else { None };
        }
        if let Some(cur) = d.in_vise.clone() {
            ui.horizontal_wrapped(|ui| {
                for v in &d.vises {
                    if ui.selectable_label(&cur == v, v.as_str()).clicked() {
                        d.in_vise = Some(v.clone());
                    }
                }
            });
            egui::Grid::new("accessory-fit").num_columns(2).show(ui, |ui| {
                ui.label("Stock top over the jaws");
                num(ui, u, &mut d.above);
                ui.end_row();
            });
        }
    }
    dims_grid(ui, u, "accessory-dims", &mut d.params, fitting);
    if fitting {
        ui.label(RichText::new(FIT_SINKS_HINT).small());
    } else {
        egui::Grid::new("accessory-place").num_columns(2).show(ui, |ui| {
            for (k, axis) in ["Origin X", "Origin Y", "Origin Z"].iter().enumerate() {
                ui.label(*axis);
                num(ui, u, &mut d.pos[k]);
                ui.end_row();
            }
            ui.label("Turn about Z (yaw)");
            num_plain(ui, &mut d.yaw_deg);
            ui.end_row();
        });
    }
    if let Some(err) = &d.error {
        super::error_text::error_text(ui, ui.visuals().error_fg_color, err);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Place").clicked() {
            act = Some(AccessoryAct::Place(Box::new(place_command(d))));
        }
        if ui.button("Cancel").clicked() {
            act = Some(AccessoryAct::Close);
        }
    });
    act
}
