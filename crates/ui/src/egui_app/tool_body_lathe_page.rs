//! The tool editor's Body section for a lathe-held tool (#361): the
//! imported holder is placed in the turning tool frame - the model
//! directions that are radial and along the spindle (a preset or a
//! picked face normal), the model point that is the tip (numbers or a
//! picked corner) and the optional insert zone - on the same orbitable
//! preview the spindle placement uses, and Apply body queues the one
//! `SetToolBody::lathe`. A boring bar, which serves both machines, gets
//! the frame switch. Picks are ephemeral; only Apply body / Clear body
//! reach the history. Provenance: our own pane over
//! `app::tool_body_lathe_view` (the handbook turning-tool convention: X
//! radial, Z along the spindle); no vendor material consulted.

use egui::{Color32, ComboBox, Painter, Pos2, Stroke, Ui};

use camcraft_app::tool_body_lathe_view::{self, FrameChoice};
use camcraft_app::tool_body_view::{self, AXIS_CHOICES};
use camcraft_app::units::{self, Unit};

use super::fields::num_narrow;
use super::layout::prop_row;
use crate::state::{BodyPickMode, State, ToolBodyUi};

/// A boring bar's frame switch; nothing for any other kind (a lathe
/// steel is always placed in the turning frame, a spun tool never).
pub(super) fn frame_row(ui: &mut Ui, state: &mut State) {
    if state.editor_body_frames() != FrameChoice::Either {
        return;
    }
    let lathe = state.tool_body_ui.lathe.is_some();
    let mut pick = lathe;
    prop_row(ui, "Body frame", |ui| {
        ComboBox::from_id_salt("tool_body_frame")
            .selected_text(if lathe { "Lathe (turning tool frame)" } else { "Mill (spindle axis)" })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut pick, false, "Mill (spindle axis)");
                ui.selectable_value(&mut pick, true, "Lathe (turning tool frame)");
            });
    });
    if pick != lathe {
        state.set_body_draft_frame(pick);
    }
}

/// The lathe placement, its preview, its note and Apply body. Called
/// with a mesh in the draft and the draft in the lathe frame. Kept to
/// short rows: the editor window does not scroll.
pub(super) fn section(ui: &mut Ui, state: &mut State, u: Unit) {
    let Some(l) = state.tool_body_ui.lathe.as_mut() else { return };
    ui.horizontal(|ui| {
        ui.label("Radial").on_hover_text("the model direction that points away from the spindle axis");
        direction_combo(ui, "tool_body_radial", &mut l.place.radial);
    });
    ui.horizontal(|ui| {
        ui.label("Spindle +Z").on_hover_text("the model direction of the lathe's +Z: from the chuck toward the free end");
        direction_combo(ui, "tool_body_along", &mut l.place.along);
    });
    ui.horizontal(|ui| {
        ui.label(format!("Tip ({})", u.suffix())).on_hover_text("the model point the moves carry: the insert's nose centre (model X, Y, Z)");
        for v in &mut l.place.tip {
            num_narrow(ui, u, v);
        }
    });
    let pick = state.tool_body_ui.pick;
    ui.horizontal(|ui| {
        let modes = [
            (BodyPickMode::Tip, "Pick tip", "click the body in the preview: the nearest corner becomes the tool tip"),
            (BodyPickMode::Radial, "Pick radial", "click a face that looks away from the spindle axis: its normal becomes the radial direction"),
            (BodyPickMode::Along, "Pick spindle", "click a face that looks along the lathe's +Z (toward the free end): its normal becomes the spindle direction"),
        ];
        for (mode, label, hover) in modes {
            if super::pick_button::pick_button(ui, pick == mode, label).on_hover_text(hover).clicked() {
                state.tool_body_ui.pick = if pick == mode { BodyPickMode::Off } else { mode };
            }
        }
    });
    super::tool_body_page::placement_view(ui, state);
    zone_rows(ui, state, u);
    let note = match state.lathe_body_preview() {
        Some(Ok(b)) => tool_body_lathe_view::extent(b).map_or_else(String::new, |(lo, hi)| {
            format!(
                "Placed: radial {} to {}, spindle {} to {} {} from the tip",
                units::len(lo[0], u, 1),
                units::len(hi[0], u, 1),
                units::len(lo[2], u, 1),
                units::len(hi[2], u, 1),
                u.suffix()
            )
        }),
        Some(Err(e)) => format!("Placement error: {e}"),
        None => String::new(),
    };
    if !note.is_empty() {
        ui.label(note);
    }
    if ui.button("Apply body").on_hover_text("attach the holder with this placement (one undo step)").clicked() {
        state.queue_apply_tool_body();
    }
}

/// The insert zone: off, or how far the insert plate reaches from the
/// tip on each side of each turning-frame axis.
fn zone_rows(ui: &mut Ui, state: &mut State, u: Unit) {
    let Some(l) = state.tool_body_ui.lathe.as_mut() else { return };
    ui.checkbox(&mut l.zone_on, "Insert zone")
        .on_hover_text("the file models the insert plate with its holder: a box about the tip the holder check leaves out, no larger than one insert");
    if !l.zone_on {
        return;
    }
    let rows = [
        (0, "Insert radial", "how far the insert plate reaches from the tip: toward the spindle axis, then away from it"),
        (1, "Insert across", "how far the insert plate reaches from the tip across the turning plane: below it, then above"),
        (2, "Insert spindle", "how far the insert plate reaches from the tip along the spindle: toward the chuck, then toward the free end"),
    ];
    for (k, caption, hover) in rows {
        ui.horizontal(|ui| {
            ui.label(caption).on_hover_text(hover);
            num_narrow(ui, u, &mut l.zone_toward[k]);
            num_narrow(ui, u, &mut l.zone_away[k]);
        });
    }
}

fn direction_combo(ui: &mut Ui, id: &str, dir: &mut [f64; 3]) {
    let text = tool_body_view::axis_label(*dir)
        .map_or_else(|| format!("picked ({:.2}, {:.2}, {:.2})", dir[0], dir[1], dir[2]), str::to_owned);
    ComboBox::from_id_salt(id).selected_text(text).show_ui(ui, |ui| {
        for (name, d) in AXIS_CHOICES {
            if ui.selectable_label(tool_body_view::axis_label(*dir) == Some(name), name).clicked() {
                *dir = d;
            }
        }
    });
}

/// The lathe placement over the preview: the tip, the radial direction
/// from it (amber) and the spindle direction (cyan), each `reach` long.
pub(super) fn overlay(painter: &Painter, d: &ToolBodyUi, screen: &dyn Fn([f64; 3]) -> Pos2, reach: f64) {
    let Some(l) = d.lathe.as_ref() else { return };
    let tip = l.place.tip;
    if !tip.iter().all(|v| v.is_finite()) {
        return;
    }
    let arrows = [(l.place.radial, Color32::from_rgb(240, 180, 60)), (l.place.along, Color32::from_rgb(90, 200, 230))];
    for (dir, colour) in arrows {
        let n = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if n.is_finite() && n > 1e-12 {
            let end = [0, 1, 2].map(|i| tip[i] + dir[i] / n * reach);
            painter.line_segment([screen(tip), screen(end)], Stroke::new(1.5, colour));
        }
    }
    painter.circle_filled(screen(tip), 4.0, Color32::from_rgb(240, 150, 40));
}

/// What the preview says while a pick mode waits for its click.
pub(super) fn pick_hint(mode: BodyPickMode) -> &'static str {
    match mode {
        BodyPickMode::Off => "",
        BodyPickMode::Tip => "click the tip",
        BodyPickMode::Axis => "click a spindle-facing face",
        BodyPickMode::Radial => "click a face looking away from the spindle axis",
        BodyPickMode::Along => "click a face looking along the lathe's +Z",
    }
}
