//! The "Add accessory…" dialog body (#789), drawn inside the fixture
//! library's modal: pick a kind (parallels, 1-2-3 block, step block,
//! strap clamp, soft jaw, rotary subplate, a lathe chuck), edit its
//! figures, then type where its frame goes - or, for parallels, fit
//! them under the stock in one of the setup's built-in vises, or, for
//! a lathe chuck, fit it to the stock (#1468). Place queues one
//! undoable `InsertAccessory`.
//! Provenance: our own layout over our own accessory commands.

use egui::{RichText, Ui};

use camcraft_app::accessory::{self, Accessory};
use camcraft_app::units::Unit;

use crate::state::{AccessoryDraft, CommandKind};

use super::fields::{num_by, num_plain_by};

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

/// What the fit does to the vise, said before Place (#812): the stock
/// stays put and the vise drops under it.
pub(crate) const FIT_SINKS_HINT: &str = "The fit keeps the stock where it is and drops the vise by the parallels' height: \
     with the stock resting on the vise floor, the vise ends below the table. Raise the stock first if the vise must stay on the table.";

/// The checkbox that fits a lathe chuck to the stock (#1468).
pub(crate) const FIT_STOCK_LABEL: &str = "Fit to the stock";

/// What the chuck fit does, said before Place.
pub(crate) const FIT_STOCK_HINT: &str = "The chuck closes on the setup's round stock (standing on a vertical axis): \
     its face at the stock's underside - stepped jaws take the stock on their step, a collet takes it its nose length deep.";

/// Why an inside grip has no stock fit.
pub(crate) const FIT_STOCK_INSIDE_HINT: &str =
    "An inside grip opens into the work's bore, which the stock does not give: set the grip diameter and place the chuck by position.";

/// The label of the figure a fit sets: `label` with the fit named
/// (parallels' "Height (set by the fit)", #812).
#[must_use]
pub(crate) fn fit_label(label: &str) -> String {
    format!("{label} (set by the fit)")
}

/// The figure fields of `a` (a step block's step count and a jaw
/// chuck's grip direction too). `fitted` is the key of the figure a
/// fit sets (parallels' height, #812; a chuck's grip diameter or collet
/// bore, #1468): shown, labelled so and not editable.
pub(crate) fn dims_grid(ui: &mut Ui, u: Unit, id: &str, a: &mut Accessory, fitted: Option<&str>) {
    egui::Grid::new(id).num_columns(2).show(ui, |ui| {
        if let Some(grip) = a.grip_mut() {
            ui.label("Grip");
            ui.horizontal(|ui| {
                for key in accessory::chuck::Grip::KEYS {
                    let Some(g) = accessory::chuck::Grip::of(key) else { continue };
                    if ui.selectable_label(*grip == g, g.label()).clicked() {
                        *grip = g;
                    }
                }
            });
            ui.end_row();
        }
        if let Some(steps) = a.steps_mut() {
            let l = ui.label("Steps");
            ui.add(egui::DragValue::new(steps).range(1..=accessory::MAX_STEPS)).labelled_by(l.id);
            ui.end_row();
        }
        for d in a.dims_mut() {
            if fitted == Some(d.key) {
                let l = ui.add_enabled(false, egui::Label::new(fit_label(d.label)));
                ui.add_enabled_ui(false, |ui| num_by(ui, u, d.value, l.id));
            } else {
                let l = ui.label(d.label);
                num_by(ui, u, d.value, l.id);
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
                let l = ui.label("Stock top over the jaws");
                num_by(ui, u, &mut d.above, l.id);
                ui.end_row();
            });
        }
    }
    let chuck = accessory::chuck::is_chuck(&d.params);
    if chuck {
        let inside = d.params.grip() == Some(accessory::chuck::Grip::Inside);
        if inside {
            d.on_stock = false;
        }
        ui.add_enabled(!inside, egui::Checkbox::new(&mut d.on_stock, FIT_STOCK_LABEL));
        if inside {
            ui.label(RichText::new(FIT_STOCK_INSIDE_HINT).small());
        }
    }
    let on_stock = d.fits_stock();
    let fitted = if fitting { Some("height") } else { on_stock.then(|| accessory::chuck::fitted_dim(&d.params)).flatten() };
    dims_grid(ui, u, "accessory-dims", &mut d.params, fitted);
    if fitting {
        ui.label(RichText::new(FIT_SINKS_HINT).small());
    } else if on_stock {
        ui.label(RichText::new(FIT_STOCK_HINT).small());
    } else {
        egui::Grid::new("accessory-place").num_columns(2).show(ui, |ui| {
            for (k, axis) in ["Origin X", "Origin Y", "Origin Z"].iter().enumerate() {
                let l = ui.label(*axis);
                num_by(ui, u, &mut d.pos[k], l.id);
                ui.end_row();
            }
            let l = ui.label("Turn about Z (yaw)");
            num_plain_by(ui, &mut d.yaw_deg, l.id);
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
