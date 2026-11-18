//! The property pages' value widgets (graduated from `pages.rs` with the
//! property grid, user review 2026-11-06) and the shared prelude every
//! kind-family page module glob-imports: the unit-lens numbers (unit as
//! the value's suffix, never in the label), the combos, the 3+2 index
//! group, and the form helpers re-exported from `layout`.
//!
//! Provenance: our own port of the M-c page widgets (egui DragValue /
//! ComboBox, the vendored egui 0.36 docs); no vendor material involved.

pub(super) use egui::{ComboBox, DragValue, Ui};
pub(super) use flecs_ecs::prelude::World;

pub(super) use camcraft_app::machining2d::{ARC_FIT_TOL, FaceHoles, Lead, PocketStrategy, Side};
pub(super) use camcraft_app::model;
pub(super) use camcraft_app::operation::{self, Height, OpParams, Tilt};
pub(super) use camcraft_app::units::{self, Unit};

pub(super) use crate::operations::{
    cycle_from_index, cycle_index, cycle_names, side_names, strategy_names, strategy3d_names,
};
pub(super) use crate::state::State;

pub(super) use super::heights_page::{HEIGHT_SRC_ID, LEVELS, corner_note, height_editor, retract_hop, retract_note};
pub(super) use super::kind_editors::*;
pub(super) use super::layout::{
    CTRL_W, auto_opt, auto_value, button, check_row, consequence, group, hint, list_row, prop_row,
    radio_pair, small_button,
};
pub(super) use super::lead_page;
pub(super) use super::op_tabs::{SEC_FEEDS, SEC_GEO, SEC_HEIGHTS, SEC_LEADS, SEC_MACHINE, SEC_PASSES};
pub(super) use super::params_page::Cx;

/// Lead in/out combo items - the whole `Lead` vocabulary, index order
/// matching the combo-arm mapping in the contour page.
pub(super) const LEADS: [&str; 5] = ["None", "Linear", "Arc", "Tangential", "Perpendicular"];
/// The 3-D finish kinematics, in the order the combo maps them.
pub(super) const FINISH_PATTERNS: [&str; 18] = [
    "Waterline",
    "Parallel",
    "Parallel - multi",
    "Scallop",
    "Radial",
    "Spiral",
    "Contour",
    "Project",
    "Steep and shallow",
    "Flat",
    "Pencil",
    "Corner",
    "Ramp",
    "Morphed spiral",
    "Geodesic",
    "Blend",
    "Morph",
    "Flow",
];

/// A caption over the control below it - the setup pages' and the tool
/// editor's stacked form; the operation dialog's rows use `prop_row`.
pub(crate) fn field(ui: &mut Ui, label: &str) {
    ui.add_space(3.0);
    ui.label(label);
}

pub(crate) fn num(ui: &mut Ui, u: Unit, v: &mut f64) {
    num_prec(ui, u, v, 4, u.suffix());
}

/// The unit-lens widget law (M1.1): in mm mode the historical
/// `DragValue` (speed 0.1, default format), now with the unit as its
/// suffix. In inch mode the widget works on a display copy -
/// `units::from_display` puts the world value back on the micron
/// lattice, so a typed `0.5` stores exactly `12.7`. The drag speed
/// divides by 25.4 so one pixel moves the same *millimeter* distance
/// either way - the unit is a lens, not a gearbox. egui's smart aim
/// rounds a drag toward the nicest number of the SHOWN unit within a
/// one-pixel radius; because of that speed law the radius is the same
/// ±0.1 mm in both modes, so the assist is unit-fair.
fn num_prec(ui: &mut Ui, u: Unit, v: &mut f64, prec: usize, unit: &str) {
    num_sized(ui, u, v, prec, &format!(" {unit}"), CTRL_W);
}

/// A narrow length field without a unit suffix, for a row of several
/// under one caption that states the unit (a point's X, Y, Z - #361):
/// the same lens as [`num`].
pub(crate) fn num_narrow(ui: &mut Ui, u: Unit, v: &mut f64) {
    num_sized(ui, u, v, 4, "", NARROW_W);
}

/// Width of a [`num_narrow`] field, logical px.
const NARROW_W: f32 = 62.0;

fn num_sized(ui: &mut Ui, u: Unit, v: &mut f64, prec: usize, suffix: &str, width: f32) -> egui::Response {
    let h = ui.spacing().interact_size.y;
    // Long values must overflow forward; add_sized centers overflow and moves the window's origin.
    let layout = egui::Layout::centered_and_justified(ui.layout().main_dir())
        .with_main_align(egui::Align::Min)
        .with_cross_align(egui::Align::Min);
    if u == Unit::Millimeters {
        ui.allocate_ui_with_layout([width, h].into(), layout, |ui| {
            ui.add(DragValue::new(v).speed(0.1).suffix(suffix))
        })
        .inner
    } else {
        let mut d = units::to_display(*v, u);
        let resp = ui.allocate_ui_with_layout([width, h].into(), layout, |ui| {
            ui.add(
                DragValue::new(&mut d)
                    .speed(0.1 / 25.4)
                    .custom_formatter(move |n, _| format!("{n:.prec$}"))
                    .suffix(suffix),
            )
        })
        .inner;
        if resp.changed() {
            *v = units::from_display(d, u);
        }
        resp
    }
}

/// [`num`] named by the label beside it (#1119): the field's accessible
/// name is that label's text. Nothing visible changes.
pub(crate) fn num_by(ui: &mut Ui, u: Unit, v: &mut f64, label: egui::Id) {
    num_sized(ui, u, v, 4, &format!(" {}", u.suffix()), CTRL_W).labelled_by(label);
}

/// Feed rates (mm/min stored, in/min shown): same lens, coarser text.
pub(crate) fn num_feed(ui: &mut Ui, u: Unit, v: &mut f64) {
    num_prec(ui, u, v, 2, u.feed_suffix());
}

fn plain_drag(ui: &mut Ui, v: &mut f64) -> egui::Response {
    let h = ui.spacing().interact_size.y;
    ui.add_sized([CTRL_W, h], DragValue::new(v).speed(0.1))
}

/// Rpm, counts, ratios: no lens - these were never millimeters.
pub(crate) fn num_plain(ui: &mut Ui, v: &mut f64) {
    plain_drag(ui, v);
}

/// [`num`] with an accessible name (#1119), for inline fields without a
/// dedicated row label.
pub(crate) fn num_named(ui: &mut Ui, u: Unit, v: &mut f64, name: &str) {
    named(num_sized(ui, u, v, 4, &format!(" {}", u.suffix()), CTRL_W), name);
}

/// [`num_feed`] with an accessible name (#1119).
pub(crate) fn num_feed_named(ui: &mut Ui, u: Unit, v: &mut f64, name: &str) {
    named(num_sized(ui, u, v, 2, &format!(" {}", u.feed_suffix()), CTRL_W), name);
}

/// [`num_plain`] with an accessible name (#1119).
pub(crate) fn num_plain_named(ui: &mut Ui, v: &mut f64, name: &str) {
    named(plain_drag(ui, v), name);
}

/// [`num`] inside a `prop_row`, named by that row's label (#1119).
pub(crate) fn num_row(ui: &mut Ui, u: Unit, v: &mut f64) {
    let r = num_sized(ui, u, v, 4, &format!(" {}", u.suffix()), CTRL_W);
    if let Some(l) = super::layout::row_label_id(ui) {
        r.labelled_by(l);
    }
}

/// [`num_plain`] inside a `prop_row`, named by that row's label (#1119).
pub(crate) fn num_plain_row(ui: &mut Ui, v: &mut f64) {
    let r = plain_drag(ui, v);
    if let Some(l) = super::layout::row_label_id(ui) {
        r.labelled_by(l);
    }
}

/// Give an input with no visible label a plain-English accessible name
/// (#1119). Screen-reader only: nothing is drawn.
pub(crate) fn named(r: egui::Response, name: &str) -> egui::Response {
    r.ctx.accesskit_node_builder(r.id, |b| b.set_label(name));
    r
}

/// [`num_plain`] named by the label beside it (#1119).
pub(crate) fn num_plain_by(ui: &mut Ui, v: &mut f64, label: egui::Id) {
    plain_drag(ui, v).labelled_by(label);
}

/// Bounded integer count inside a `prop_row`, named by that row's
/// label (#1119, #1122).
pub(crate) fn int_range_row<N: egui::emath::Numeric>(
    ui: &mut Ui,
    v: &mut N,
    range: std::ops::RangeInclusive<N>,
) {
    let r = ui.add(DragValue::new(v).range(range));
    if let Some(l) = super::layout::row_label_id(ui) {
        r.labelled_by(l);
    }
}

/// A lens-free number with its unit as the suffix ("°", "s", "%").
pub(crate) fn num_unit(ui: &mut Ui, v: &mut f64, unit: &str) {
    super::layout::value_with_unit(ui, v, unit, 0.1);
}

/// The 3+2 index editor (M5.1 stage B): the group's header checkbox arms
/// or disarms a tilt, and the rows edit the two RS-274 letters (degrees -
/// the A/B of [`camcraft_app::cl`]'s pose convention, C left to the
/// table) and the plane anchor. It writes the draft, like every other
/// field here, so the existing EditOperationParams commit path makes the
/// edit undoable. The panel never normalizes and never refuses: the door
/// in [`Tilt::is_sane`] owns the bounds.
pub(super) fn orientation_editor(ui: &mut Ui, u: Unit, tilt: &mut Option<Tilt>) {
    let mut armed = tilt.is_some();
    let g = group(ui, "Tilted plane (3+2)", Some(&mut armed), |ui| {
        if let Some(t) = tilt.as_mut() {
            prop_row(ui, "Tool axis A", |ui| num_unit(ui, &mut t.a_deg, "°"));
            prop_row(ui, "Tool axis B", |ui| num_unit(ui, &mut t.b_deg, "°"));
            prop_row(ui, "Plane origin X", |ui| num(ui, u, &mut t.origin[0]));
            prop_row(ui, "Plane origin Y", |ui| num(ui, u, &mut t.origin[1]));
            prop_row(ui, "Plane origin Z", |ui| num(ui, u, &mut t.origin[2]));
        }
    });
    if g.toggled {
        *tilt = if armed { Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg: 0.0, origin: [0.0; 3] }) } else { None };
    }
}

/// Combo over `&[&str]` with a usize model (imgui's combo idiom).
pub(crate) fn combo(ui: &mut Ui, id: &str, idx: &mut usize, names: &[&str]) {
    let cur = names.get((*idx).min(names.len().saturating_sub(1))).copied().unwrap_or("");
    ComboBox::from_id_salt(id).width(CTRL_W).selected_text(cur).show_ui(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            ui.selectable_value(idx, i, *n);
        }
    });
}

/// Same, for owned name lists (tool/source lists built per frame).
pub(crate) fn combo_owned(ui: &mut Ui, id: &str, idx: &mut usize, names: &[String]) {
    let cur: String = names.get((*idx).min(names.len().saturating_sub(1))).cloned().unwrap_or_default();
    ComboBox::from_id_salt(id).width(CTRL_W).selected_text(cur).show_ui(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            ui.selectable_value(idx, i, n.as_str());
        }
    });
}
