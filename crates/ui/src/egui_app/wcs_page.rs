//! The WCS page (#342: the active setup's one work coordinate system),
//! split out of `pages.rs` and laid out in three sections (#739): Origin
//! and orientation (label and field on one row, the fields in one column),
//! Quarter turns (equal buttons filling the pane's width) and Anchor on
//! (the stock box or model bounds as a 3x3 compass of equal cells, or the
//! centre of a picked circular edge). Every change is a command:
//! `SetWcs` for the fields, turns and box points, `SetWcsCircle` for the
//! circle centre (the app's shared `pick::circle_centre` reads the
//! circle) - the page only drafts and queues.
//!
//! Provenance: behaviour specced from published CAM setup practice; the
//! layout is our own over the `layout` form helpers; no vendor material.

use egui::{Button, ComboBox, RichText, Ui, Vec2};

use camcraft_app::geometry::{quat_from_axis_angle, quat_from_euler, quat_mul, quat_to_euler};
use camcraft_app::machine::{Kind, kind_of};
use camcraft_app::units::{self, Unit};
use camcraft_app::{model, setup, setup_wcs_tilt};

use crate::interact;
use crate::state::{CommandKind, State, quat_yaw_deg};

use super::fields::{num, num_plain};
use super::layout::{group, prop_grid};

/// The "Anchor on" choices; index 2 is the #739 circle centre.
pub(super) const SOURCES: [&str; 3] = ["Stock box", "Model bounds", "Circle centre"];
const CIRCLE: usize = 2;
/// A button row's height, and the compass cells' size cap.
const BUTTON_H: f32 = 24.0;
const CELL_MAX: Vec2 = Vec2::new(72.0, 30.0);

/// The quarter turns, two rows of three: (label, setup axis, sign).
const TURNS: [[(&str, usize, f64); 3]; 2] = [
    [("Turn X+", 0, 1.0), ("Turn Y+", 1, 1.0), ("Turn Z+", 2, 1.0)],
    [("Turn X-", 0, -1.0), ("Turn Y-", 1, -1.0), ("Turn Z-", 2, -1.0)],
];

const MILL_TILT_HINT: &str =
    "Milling cuts along machine Z. Use Part alignment on the Setup page to turn the part.";

pub(super) fn wcs_page(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    let is_mill = kind_of(&state.world) == Kind::Mill;
    let active = setup::active(&state.world);
    let tilted_mill = setup_wcs_tilt::setup_problem(&state.world, active.as_deref()).is_some();
    if tilted_mill {
        ui.label(
            RichText::new(
                "This milling WCS is tilted off machine Z and generation will refuse it. Click Level, then turn the part with Part alignment on the Setup page.",
            )
            .small()
            .color(ui.visuals().warn_fg_color),
        );
    }
    group(ui, "Origin and orientation", None, |ui| origin_fields(ui, state, u, is_mill));
    group(ui, "Quarter turns", None, |ui| quarter_turns(ui, state, is_mill));
    group(ui, "Anchor on", None, |ui| anchor_section(ui, state));
    readout(ui, state, u);
}

/// Label | field rows, one grid so the fields share one column.
fn origin_fields(ui: &mut Ui, state: &mut State, u: Unit, is_mill: bool) {
    let w = &mut state.wcs_ui;
    prop_grid(ui, "wcs_origin_grid", |ui| {
        for (label, v) in [("X origin", &mut w.x), ("Y origin", &mut w.y), ("Z origin", &mut w.z)] {
            ui.label(label);
            num(ui, u, v);
            ui.end_row();
        }
        for (label, v, tilt_axis) in [
            ("Tilt about X (roll)", &mut w.roll_deg, true),
            ("Tilt about Y (pitch)", &mut w.pitch_deg, true),
            ("Turn about Z (yaw)", &mut w.yaw_deg, false),
        ] {
            ui.label(label);
            let enabled = !is_mill || !tilt_axis;
            ui.add_enabled_ui(enabled, |ui| num_plain(ui, v))
                .response
                .on_disabled_hover_text(MILL_TILT_HINT);
            ui.end_row();
        }
    });
    if is_mill {
        ui.label(RichText::new(MILL_TILT_HINT).small().weak());
    }
}

/// One row of `labels` as equal buttons filling the available width;
/// the index of the one clicked.
pub(super) fn even_row(ui: &mut Ui, labels: &[&str], size: Vec2) -> Option<usize> {
    let mut hit = None;
    ui.horizontal(|ui| {
        for (i, label) in labels.iter().enumerate() {
            if ui.add_sized(size, Button::new(*label)).clicked() {
                hit = Some(i);
            }
        }
    });
    hit
}

/// The width each of `n` equal cells takes to fill `avail`.
pub(super) fn cell_w(ui: &Ui, avail: f32, n: usize) -> f32 {
    let gaps = ui.spacing().item_spacing.x * n.saturating_sub(1) as f32;
    ((avail - gaps) / n.max(1) as f32).max(24.0)
}

fn turn_row(ui: &mut Ui, row: &[(&str, usize, f64); 3], size: Vec2, is_mill: bool) -> Option<usize> {
    let mut hit = None;
    ui.horizontal(|ui| {
        for (i, &(label, axis, _)) in row.iter().enumerate() {
            let enabled = !is_mill || axis == 2;
            let resp = ui
                .add_enabled_ui(enabled, |ui| ui.add_sized(size, Button::new(label)))
                .inner
                .on_disabled_hover_text(MILL_TILT_HINT);
            if resp.clicked() {
                hit = Some(i);
            }
        }
    });
    hit
}

fn quarter_turns(ui: &mut Ui, state: &mut State, is_mill: bool) {
    ui.label(RichText::new("About the setup axes; applied at once.").small().weak());
    let avail = ui.available_width();
    let size3 = Vec2::new(cell_w(ui, avail, 3), BUTTON_H);
    for row in TURNS {
        if let Some(i) = turn_row(ui, &row, size3, is_mill)
            && let Some((_, axis, sign)) = row.get(i)
        {
            turn_wcs(state, *axis, *sign);
        }
    }
    let size2 = Vec2::new(cell_w(ui, avail, 2), BUTTON_H);
    match even_row(ui, &["Level", "Set Origin"], size2) {
        Some(0) => {
            state.wcs_ui.roll_deg = 0.0;
            state.wcs_ui.pitch_deg = 0.0;
            state.wcs_ui.yaw_deg = 0.0;
            apply_wcs_drafts(state);
        }
        Some(_) => apply_wcs_drafts(state),
        None => {}
    }
}

// Box-point anchoring (feedback 5, revised): the reference box is an
// explicit choice - stock or model - and the nine points of the chosen
// level can be clicked in the viewport as markers instead of hunting
// through the button grid. The buttons and the markers share
// `interact::anchor_points`, so what you click, what highlights, and what
// places are the same points by construction (the imgui pane's origin
// picker, restored after the M-g retirement). The third choice (#739)
// anchors on a picked circle's centre instead.
fn anchor_section(ui: &mut Ui, state: &mut State) {
    let src = state.wcs_ui.anchor_source.min(CIRCLE);
    prop_grid(ui, "wcs_anchor_grid", |ui| {
        ui.label("Anchor on");
        ComboBox::from_id_salt("wcs_src").selected_text(SOURCES[src]).show_ui(ui, |ui| {
            for (i, s) in SOURCES.iter().enumerate() {
                ui.selectable_value(&mut state.wcs_ui.anchor_source, i, *s);
            }
        });
        ui.end_row();
    });
    // One armed viewport pick at a time: each choice disarms the other's.
    if state.wcs_ui.anchor_source.min(CIRCLE) == CIRCLE {
        state.wcs_ui.pick_anchor = false;
        circle_anchor(ui, state);
    } else {
        state.wcs_ui.pick_circle = false;
        box_anchor(ui, state);
    }
}

fn box_anchor(ui: &mut Ui, state: &mut State) {
    let Some((_, what)) = interact::anchor_reference(state) else {
        ui.label("Import a model or set stock to anchor the origin on a point.");
        return;
    };
    let chosen = SOURCES[state.wcs_ui.anchor_source.min(1)];
    if !chosen.eq_ignore_ascii_case(what) {
        ui.label(format!("No {chosen} defined - anchoring on {what}."));
    }
    // One plain row, not the label column: three radios beside a 120 px
    // label would widen the section past a narrow pane.
    ui.horizontal(|ui| {
        ui.label("Z level");
        for (i, name) in ["Bottom", "Middle", "Top"].iter().enumerate() {
            ui.radio_value(&mut state.wcs_ui.anchor_z, i, *name);
        }
    });
    ui.checkbox(&mut state.wcs_ui.pick_anchor, "Pick markers in viewport");
    if state.wcs_ui.pick_anchor {
        ui.label("Click a yellow marker, a corner or a circle centre of the part in the viewport to set the origin.");
    }
    compass(ui, state);
}

/// The nine box points as a centred 3x3 compass of equal cells.
fn compass(ui: &mut Ui, state: &mut State) {
    let pts = interact::anchor_points(state);
    let avail = ui.available_width();
    let w = cell_w(ui, avail, 3).min(CELL_MAX.x);
    let size = Vec2::new(w, CELL_MAX.y);
    let gap = ui.spacing().item_spacing.x;
    // Centred by placement, not by padding: each row is one exact strip
    // the cells are put into.
    let pad = ((avail - (3.0 * w + 2.0 * gap)) * 0.5).max(0.0);
    for row in 0..3usize {
        let (strip, _) = ui.allocate_exact_size(Vec2::new(avail, size.y), egui::Sense::hover());
        let mut hit = None;
        for col in 0..3usize {
            let k = row * 3 + col;
            let code = pts.get(k).map_or("?", |(_, name)| *name);
            let label = if state.wcs_ui.anchor_xy == k { format!("[{code}]") } else { code.to_owned() };
            let at = strip.min + Vec2::new(pad + col as f32 * (w + gap), 0.0);
            if ui.put(egui::Rect::from_min_size(at, size), Button::new(label)).clicked() {
                hit = Some(col);
            }
        }
        if let Some(col) = hit {
            let k = row * 3 + col;
            if let Some((pos, _)) = pts.get(k) {
                state.wcs_ui.anchor_xy = k;
                state.wcs_ui.x = pos[0];
                state.wcs_ui.y = pos[1];
                state.wcs_ui.z = pos[2];
                apply_wcs_drafts(state);
            }
        }
    }
}

/// The text the page shows for the last circle pick.
pub(super) fn circle_line(state: &State) -> String {
    let u = state.units();
    match &state.wcs_ui.circle {
        Some(Ok(c)) => format!(
            "{} Ø {} {}, centre ({}, {}, {})",
            if c.closed { "Circle" } else { "Arc" },
            units::len(c.diameter(), u, 2),
            u.suffix(),
            units::len(c.centre[0], u, 2),
            units::len(c.centre[1], u, 2),
            units::len(c.centre[2], u, 2),
        ),
        Some(Err(why)) => format!("Not anchored: {why}"),
        None => String::from("No circle picked yet."),
    }
}

fn circle_anchor(ui: &mut Ui, state: &mut State) {
    let avail = ui.available_width();
    let armed = state.wcs_ui.pick_circle;
    if ui.add_sized(Vec2::new(avail, BUTTON_H), super::pick_button::pick_widget(ui, armed, "Pick a circular edge")).clicked() {
        state.wcs_ui.pick_circle = !armed;
    }
    if state.wcs_ui.pick_circle {
        ui.label("Click a hole rim or a boss edge in the viewport: the origin goes to its centre.");
    }
    ui.label(circle_line(state));
}

/// The placed WCS, and where its G-code word is chosen.
fn readout(ui: &mut Ui, state: &mut State, u: Unit) {
    // #342: the setup's one WCS, and the work offset its post writes. The
    // dropdown that picks it is drawn under this page (#343), so the note
    // names what this page shows, not where the choice lives.
    let wcs_ctx = camcraft_app::setup::active(&state.world);
    let label = camcraft_app::setup::wcs_label(&state.world, wcs_ctx.as_deref());
    let line = match camcraft_app::setup::wcs_entity(&state.world, wcs_ctx.as_deref())
        .and_then(|e| model::transform_of(&state.world, &e))
    {
        Some(loc) => format!(
            "{label}: {}, {}, {}  yaw {:.1}",
            units::len(loc.pos.x, u, 2),
            units::len(loc.pos.y, u, 2),
            units::len(loc.pos.z, u, 2),
            quat_yaw_deg(loc.rot)
        ),
        None => format!("{label}: not placed (at the origin)"),
    };
    ui.label(line);
    ui.label("Work offset: the offset this setup posts, as its post names it. It does not move the origin.");
}

pub(super) fn apply_wcs_drafts(state: &mut State) {
    state.pending.push_back(CommandKind::SetWcs {
        pos: [state.wcs_ui.x, state.wcs_ui.y, state.wcs_ui.z],
        roll_deg: state.wcs_ui.roll_deg,
        pitch_deg: state.wcs_ui.pitch_deg,
        yaw_deg: state.wcs_ui.yaw_deg,
    });
}

pub(super) fn turn_wcs(state: &mut State, axis: usize, sign: f64) {
    let q = quat_from_euler(
        state.wcs_ui.roll_deg.to_radians(),
        state.wcs_ui.pitch_deg.to_radians(),
        state.wcs_ui.yaw_deg.to_radians(),
    );
    let turn = quat_from_axis_angle(axis, sign * 90.0f64.to_radians());
    // Right composition turns about the current setup axes.
    let (r, p, y) = quat_to_euler(quat_mul(q, turn));
    state.wcs_ui.roll_deg = r.to_degrees();
    state.wcs_ui.pitch_deg = p.to_degrees();
    state.wcs_ui.yaw_deg = y.to_degrees();
    apply_wcs_drafts(state);
}
