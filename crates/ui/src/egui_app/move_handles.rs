//! Paint the move handles (#1198 slice b): three axis arms from the
//! selected part's centre, each ending in a ball, at a constant size on
//! screen; the ball under the pointer (or in a drag) is drawn larger. While
//! a drag is in flight the signed distance sits just above the bottom bar.
//! The geometry and the drag law live in `crate::move_drag`; this file only
//! paints what it returns.
//!
//! Provenance: our own paint code; the axis colours are the theme's triad.

use crate::move_drag;
use crate::state::State;

/// The arms are computed once per frame and serve the paint, the hot
/// ball and the cursor.
pub(crate) fn draw(ui: &egui::Ui, state: &State, pointer: Option<[f32; 2]>, pane: egui::Rect, font: &egui::FontId) {
    let arms = move_drag::arms_px(state);
    if arms.is_empty() {
        return;
    }
    let ink = super::ground_ink::cube();
    let shade = egui::Color32::from_black_alpha(170);
    let live = state.body_drag.as_ref().map(|d| d.axis);
    let under = pointer.and_then(|p| move_drag::ball_in(&arms, p));
    let hot = live.or(under);
    let pos = |p: [f32; 2]| egui::pos2(p[0], p[1]);
    for &(axis, c, b) in &arms {
        let col = ink.triad.get(axis).copied().unwrap_or(egui::Color32::WHITE);
        let dim = live.is_some_and(|a| a != axis);
        let col = if dim { col.gamma_multiply(0.35) } else { col };
        ui.painter().line_segment([pos(c), pos(b)], egui::Stroke::new(4.0, shade));
        ui.painter().line_segment([pos(c), pos(b)], egui::Stroke::new(2.0, col));
        let r = if hot == Some(axis) { move_drag::BALL_PX * 1.35 } else { move_drag::BALL_PX };
        let label = format!("Move along {}", ["X", "Y", "Z"][axis]);
        let response = ui.interact(egui::Rect::from_center_size(pos(b), egui::Vec2::splat(r * 2.0)), ui.id().with(("move_handle", axis)), egui::Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label));
        if under == Some(axis) {
            response.on_hover_text(format!("{label}: drag the ball. Hold Shift to snap."));
        }
        ui.painter().circle(pos(b), r, col, egui::Stroke::new(1.5, shade));
    }
    if let Some(text) = move_drag::readout(state) {
        let bar = super::viewport_bar::panel_rect(pane, true);
        let at = egui::pos2(bar.center().x, bar.min.y - 6.0);
        ui.painter().text(at + egui::vec2(1.0, 1.0), egui::Align2::CENTER_BOTTOM, &text, font.clone(), shade);
        ui.painter().text(at, egui::Align2::CENTER_BOTTOM, text, font.clone(), egui::Color32::WHITE);
    }
    if under.is_some() || live.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
}
