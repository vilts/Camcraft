//! Paint the turn rings (#2047, #1198 slice c): three rings round the
//! selected part's centre, one square to each axis, at a constant size on
//! screen; the ring under the pointer (or in a drag) is drawn thicker.
//! While a turn is in flight the signed angle sits just above the bottom
//! bar. The geometry and the drag law live in `crate::turn_drag`; this file
//! only paints what it returns, plus the bar button's glyph.
//!
//! Provenance: our own paint code; the axis colours are the theme's triad.

use crate::state::State;
use crate::turn_drag;

/// The rings are computed once per frame and serve the paint, the hot
/// ring and the cursor.
pub(crate) fn draw(ui: &egui::Ui, state: &State, pointer: Option<[f32; 2]>, pane: egui::Rect, font: &egui::FontId) {
    let rings = turn_drag::rings_px(state);
    if rings.is_empty() {
        return;
    }
    let ink = super::ground_ink::cube();
    let shade = egui::Color32::from_black_alpha(170);
    let live = state.body_turn.as_ref().map(|d| d.axis);
    let under = pointer.and_then(|p| turn_drag::ring_in(&rings, p));
    let hot = live.or(under);
    for (axis, pts) in &rings {
        let col = ink.triad.get(*axis).copied().unwrap_or(egui::Color32::WHITE);
        let dim = live.is_some_and(|a| a != *axis);
        let col = if dim { col.gamma_multiply(0.35) } else { col };
        let w = if hot == Some(*axis) { 3.5 } else { 2.0 };
        let line: Vec<egui::Pos2> = pts.iter().map(|p| egui::pos2(p[0], p[1])).collect();
        ui.painter().add(egui::Shape::line(line.clone(), egui::Stroke::new(w + 2.0, shade)));
        ui.painter().add(egui::Shape::line(line, egui::Stroke::new(w, col)));
    }
    if let Some(text) = turn_drag::readout(state) {
        let bar = super::viewport_bar::bar_rect(pane);
        let at = egui::pos2(bar.center().x, bar.min.y - 6.0);
        ui.painter().text(at + egui::vec2(1.0, 1.0), egui::Align2::CENTER_BOTTOM, &text, font.clone(), shade);
        ui.painter().text(at, egui::Align2::CENTER_BOTTOM, text, font.clone(), egui::Color32::WHITE);
    }
    if under.is_some() || live.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
}

/// The Rotate button's glyph: three quarters of a ring with an arrow head
/// at its open end - the turn the mode puts on the part.
pub(crate) fn glyph(ui: &egui::Ui, r: egui::Rect, s: egui::Stroke) {
    let c = r.center();
    let rad = r.width() * 0.28;
    let pts: Vec<egui::Pos2> = (0..=18)
        .map(|k| {
            let t = std::f32::consts::FRAC_PI_2 * 3.0 * k as f32 / 18.0;
            c + egui::vec2(t.cos() * rad, -t.sin() * rad)
        })
        .collect();
    let tip = pts.last().copied().unwrap_or(c);
    ui.painter().add(egui::Shape::line(pts, s));
    // The open end points down the ring (at -90 degrees, travelling +x).
    ui.painter().line_segment([tip, tip + egui::vec2(-3.0, -3.0)], s);
    ui.painter().line_segment([tip, tip + egui::vec2(-3.0, 3.0)], s);
}
