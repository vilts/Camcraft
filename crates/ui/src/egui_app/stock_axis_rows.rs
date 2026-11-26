//! The Stock page's axis rows (#2576): which way the stock's own Z (a
//! cylinder's axis, a box's height) runs - along Z, X or Y, or a custom
//! turn by three angles - and, before any simulation, a plain note when
//! the simulation cannot cut the drafted stock yet. Drafts only: Apply
//! sends the turn through `SetStock`.
//!
//! Provenance: our own layout over the `layout` form helpers; the axis
//! choice is plain shop practice (a bar laid along X or Y); no vendor
//! material.

use egui::{RichText, Ui};

use camcraft_app::geometry::stock_axis::StockAxis;

use crate::state::StockUi;

use super::fields::num_plain_named;
use super::layout::prop_grid;

/// The axis row's hover text.
const AXIS_HINT: &str = "The direction of the cylinder's axis, or of the box's height. \
     Along X and Along Y lay the stock on its side.";
/// What the size and position fields mean once the stock is turned.
const TURNED_HINT: &str = "Height runs along the chosen axis. \
     X center, Y center and Z bottom give the center of the stock's base face.";

/// The custom angles, in the order of `StockUi::angles_deg`.
const ANGLE_LABELS: [&str; 3] = ["Turn about Z (yaw)", "Tilt about Y (pitch)", "Tilt about X (roll)"];
/// The hover text of each custom angle, in the same order.
const ANGLE_HINTS: [&str; 3] = [
    "Degrees to turn the stock about the machine's Z axis. Applied last.",
    "Degrees to tilt the stock about the Y axis. Applied after the roll.",
    "Degrees to tilt the stock about the X axis. Applied first.",
];

pub(super) fn axis_rows(ui: &mut Ui, s: &mut StockUi) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Stock axis").on_hover_text(AXIS_HINT);
        for axis in StockAxis::ALL {
            ui.radio_value(&mut s.axis, axis, axis.label()).on_hover_text(AXIS_HINT);
        }
    });
    if s.axis == StockAxis::Custom {
        prop_grid(ui, "stock_axis_grid", |ui| {
            for ((label, hint), v) in ANGLE_LABELS.iter().zip(ANGLE_HINTS).zip(s.angles_deg.iter_mut()) {
                ui.label(*label).on_hover_text(hint);
                ui.scope(|ui| num_plain_named(ui, v, &format!("{label} in degrees"))).response.on_hover_text(hint);
                ui.end_row();
            }
        });
    }
    if s.axis != StockAxis::Z {
        ui.label(RichText::new(TURNED_HINT).small().weak());
    }
    if let Some(note) = s.sim_note() {
        ui.colored_label(ui.visuals().warn_fg_color, note);
    }
}
