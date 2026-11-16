//! The viewport's pick marks (#1505): one style for what an
//! operation's Geometry has selected and one, clearly different, for what
//! the pointer is over - the edge a click would take (the Geometry-list row
//! hover is main's `interact::row_hover_segments`, #1507). Both
//! are screen-space strokes drawn over the model with a dark halo, so they
//! read on the shaded and the wireframe model in either theme. Hover is
//! ephemeral view state: nothing here is a command.
//!
//! Provenance: our own overlay styling; the stroke widths are plain UI
//! choices, no vendor material consulted.

use camcraft_app::operation::FeatureSel;

use crate::egui_app::ground_ink::{Overlay, overlay};
use crate::state::State;

/// Width of a selected pick's stroke, logical px.
pub const SELECTED_PX: f32 = 3.0;
/// Width of the hover stroke, logical px: thicker than the selected one, so
/// the two never read as the same mark.
pub const HOVER_PX: f32 = 5.0;
/// How much wider the dark halo under either stroke is, px.
pub const HALO_EXTRA_PX: f32 = 2.5;
/// The halo's alpha (black): a rim that separates the stroke from a light
/// or a dark model.
pub const HALO_ALPHA: u8 = 200;

/// Screen segments of the selected picks (viewport uv), points excluded:
/// those draw as their own diamonds (`vertex_pick::picked_point_marks`).
pub(crate) fn selected_segments(state: &State, aspect: f64) -> Vec<([f32; 2], [f32; 2])> {
    let mut out = Vec::new();
    for g in crate::interact::shown_picks(state) {
        if !matches!(g.feature, FeatureSel::Vertex(_)) {
            crate::interact::feature_segments(state, &g.entity, g.feature, aspect, &mut out);
        }
    }
    out
}

/// Paint the selected picks, then the hover over them. `to_px` maps a
/// viewport uv into the pane; `ink` turns an overlay rgb into a colour.
pub(crate) fn paint(
    painter: &egui::Painter,
    state: &State,
    aspect: f64,
    to_px: impl Fn([f32; 2]) -> egui::Pos2,
    ink: impl Fn([f32; 3]) -> egui::Color32,
) {
    let halo = egui::Color32::from_black_alpha(HALO_ALPHA);
    let stroke = |segs: Vec<([f32; 2], [f32; 2])>, px: f32, rgb: [f32; 3]| {
        for (a, b) in &segs {
            painter.line_segment([to_px(*a), to_px(*b)], egui::Stroke::new(px + HALO_EXTRA_PX, halo));
        }
        for (a, b) in &segs {
            painter.line_segment([to_px(*a), to_px(*b)], egui::Stroke::new(px, ink(rgb)));
        }
    };
    stroke(selected_segments(state, aspect), SELECTED_PX, overlay(Overlay::Pick));
    stroke(crate::interact::hover_features(state, aspect), HOVER_PX, overlay(Overlay::Hover));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui_app::ground_ink::overlay_ink;

    /// The style law: a selected pick is a real stroke, the hover is
    /// thicker, and their inks differ on both grounds.
    #[test]
    fn selected_and_hover_styles_are_strong_and_tell_apart() {
        const { assert!(SELECTED_PX >= 3.0, "a selected edge is no hairline") };
        const { assert!(HOVER_PX > SELECTED_PX, "hover reads as the stronger mark") };
        const { assert!(HALO_EXTRA_PX > 0.0 && HALO_ALPHA >= 128) };
        for light in [false, true] {
            assert_ne!(overlay_ink(Overlay::Pick, light), overlay_ink(Overlay::Hover, light));
        }
    }
}
