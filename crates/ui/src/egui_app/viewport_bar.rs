//! The viewport's bottom control bar (#1198 slice a): a rounded grey
//! ribbon floating over the bottom edge of the 3D pane, carrying the view
//! buttons that used to stack under the view cube - frame, iso,
//! shaded/wireframe, toolpaths and world axes - and since slice b the Move
//! mode that puts drag handles on the selected part (`crate::move_drag`),
//! since slice c (#2047) the Rotate mode that puts turn rings on it
//! (`crate::turn_drag`).
//!
//! Provenance: the floating button ribbon of interactive 3D CAD viewports,
//! written from published CAD practice - no vendor material.
//!
//! Layout is pure arithmetic on the pane rect, like `viewcube::Hud`, so the
//! pointer layer and the paint pass agree on every pixel. The buttons' act
//! and their glyphs live here; the cube stays in `viewcube`. Each button
//! keeps its old effect: `Frame` is a queued command, the toggles are direct
//! view-state writes (the camera's immediate-mode exemption).

use crate::actions;
use crate::state::{DisplayMode, DragMode, State};
use crate::viewport::ProjectionMode;

/// Button side, the gap between buttons, the ribbon's inner padding and its
/// margin from the pane's bottom edge.
pub(crate) const NAV_PX: f32 = 26.0;
const NAV_GAP: f32 = 4.0;
const PAD: f32 = 6.0;
const BOTTOM_MARGIN: f32 = 10.0;
/// Thirteen buttons in a row: the ribbon's width and height, so the layout
/// gate can ask the pane for room (pinned by `bar_fits_its_buttons`).
pub(crate) const BAR_W: f32 = 13.0 * NAV_PX + 12.0 * NAV_GAP + 2.0 * PAD;
const BAR_H: f32 = NAV_PX + 2.0 * PAD;
const CTRL_H: f32 = 52.0;
const ROW_GAP: f32 = 4.0;
const BAR_RADIUS: f32 = 8.0;

/// Camera buttons (#1198 follow-up): the projection is a direct
/// view-state write like the display-mode toggle; the orbit / pan /
/// zoom tools latch the drag onto that mode and leave the shortcut to
/// the vertex (a tool latched is a tool any drag obeys). View state
/// like the camera - never commands, never saved.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Nav {
    /// Reframe the whole document - the F key's button twin.
    Frame,
    /// The house isometric, yaw 0.9 / pitch 0.5.
    Iso,
    /// Wireframe on and off: one press gives the hidden-line wireframe from
    /// each mode, the next gives shaded with edges (the Display menu has
    /// the four modes).
    Wire,
    /// Open or close the Display menu (#2818, `display_menu`): display
    /// mode, Look at and graphics.
    Display,
    /// Draw every cached path, not just the selected op's.
    Paths,
    /// The dimmed world origin axes.
    Axes,
    /// Viewport picks may reach the far side of the part (#692). Off by
    /// default; when on, a click sends `through: true`.
    PickThrough,
    /// Move mode (#1198 b): the selected part wears axis handles whose
    /// balls drag it. Off by default.
    Move,
    /// Rotate mode (#2047): the selected part wears turn rings that turn
    /// it about an axis through its centre. Off by default.
    Rotate,
    /// Perspective <-> Orthographic, the View door's radios' twin.
    Projection,
    /// Latch the drag onto the orbit tool whatever the button.
    Orbit,
    /// Latch the drag onto the pan tool whatever the button.
    Pan,
    /// Latch the drag onto the zoom tool whatever the button.
    Zoom,
}

/// The buttons in bar order. `NAV_COUNT` sizes the ribbon, and a test checks
/// the two agree.
pub(crate) const NAV_COUNT: usize = 13;
pub(crate) const NAVS: [Nav; NAV_COUNT] = [
    Nav::Frame,
    Nav::Iso,
    Nav::Wire,
    Nav::Display,
    Nav::Paths,
    Nav::Axes,
    Nav::PickThrough,
    Nav::Move,
    Nav::Rotate,
    Nav::Projection,
    Nav::Orbit,
    Nav::Pan,
    Nav::Zoom,
];

/// The ribbon: centred on the pane's width, resting on its bottom edge at
/// the same margin as before. Floats over the 3D view (#1198, maintainer:
/// "bottom-center floating ontop of the 3D viewer").
#[must_use]
pub(crate) fn bar_rect(pane: egui::Rect) -> egui::Rect {
    let min = egui::pos2(pane.center().x - BAR_W * 0.5, pane.max.y - BOTTOM_MARGIN - BAR_H);
    egui::Rect::from_min_size(min, egui::vec2(BAR_W, BAR_H))
}

/// The full rounded overlay rect: `bar_rect(pane)` at rest, extended
/// upward by the Move / Rotate controls row when either mode is active.
#[must_use]
pub(crate) fn panel_rect(pane: egui::Rect, expanded: bool) -> egui::Rect {
    let base = bar_rect(pane);
    if !expanded {
        return base;
    }
    egui::Rect::from_min_max(
        egui::pos2(base.min.x, base.min.y - CTRL_H - ROW_GAP),
        base.max,
    )
}

/// The inner strip inside an expanded bar that holds the axis, typed
/// value and snap step controls.
#[must_use]
pub(crate) fn controls_rect(panel: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(panel.min.x + PAD, panel.min.y + PAD),
        egui::vec2((panel.width() - 2.0 * PAD).max(1.0), CTRL_H),
    )
}

/// Each button's rect inside the ribbon, left to right in `NAVS` order.
#[must_use]
pub(crate) fn buttons(bar: egui::Rect) -> Vec<(Nav, egui::Rect)> {
    NAVS.iter()
        .enumerate()
        .map(|(i, &kind)| {
            let min = egui::pos2(
                bar.min.x + PAD + i as f32 * (NAV_PX + NAV_GAP),
                bar.min.y + PAD,
            );
            (kind, egui::Rect::from_min_size(min, egui::vec2(NAV_PX, NAV_PX)))
        })
        .collect()
}

/// The buttons' effects, once each. `FrameAll` is a queued command like the
/// F key's; the toggles are direct view-state writes exactly as panes.rs
/// flips them (never commands - the camera's exemption).
pub(crate) fn act(state: &mut State, kind: Nav) {
    match kind {
        Nav::Frame => actions::frame_all(state),
        Nav::Iso => actions::view_iso(state),
        Nav::Wire => {
            state.display = match state.display {
                m if m.is_wireframe() => DisplayMode::ShadedEdges,
                _ => DisplayMode::Wireframe,
            };
            state.persist_view();
        }
        Nav::Display => state.display_menu_open = !state.display_menu_open,
        Nav::Paths => state.toggle_all_toolpaths(),
        Nav::Axes => state.show_world_axes = !state.show_world_axes,
        Nav::PickThrough => {
            state.pick_through = !state.pick_through;
            state.status = pick_through_label(state.pick_through).to_string();
        }
        Nav::Move => {
            state.move_handles = !state.move_handles;
            // Leaving Move mode drops a drag in flight - nothing queued.
            // One gizmo shows both kinds of handle (#2624).
            crate::move_drag::cancel(state);
            crate::turn_drag::cancel(state);
            state.rotate_handles = state.move_handles;
            state.status = move_label(state.move_handles).to_string();
        }
        Nav::Rotate => {
            if state.move_handles {
                // The legacy shortcut chooses a value, never hides a ring.
                state.handle_bar.turn_value = true;
                return;
            }
            state.rotate_handles = !state.rotate_handles;
            crate::move_drag::cancel(state);
            crate::turn_drag::cancel(state);
            state.move_handles &= !state.rotate_handles;
            state.status = rotate_label(state.rotate_handles).to_string();
        }
        Nav::Projection => {
            state.cam.projection = match state.cam.projection {
                ProjectionMode::Perspective => ProjectionMode::Orthographic,
                ProjectionMode::Orthographic => ProjectionMode::Perspective,
            };
            state.status = projection_label(state.cam.projection).to_string();
        }
        Nav::Orbit | Nav::Pan | Nav::Zoom => {
            let wanted = match kind {
                Nav::Orbit => DragMode::Orbit,
                Nav::Pan => DragMode::Pan,
                _ => DragMode::Zoom,
            };
            state.nav_tool = if state.nav_tool == Some(wanted) {
                None
            } else {
                Some(wanted)
            };
            state.status = state
                .nav_tool
                .map_or_else(|| NAV_TOOL_CLEAR.to_string(), |t| nav_tool_label(t).to_string());
        }
    }
}

/// Paint the ribbon and its buttons, with hover help for each icon.
pub(crate) fn draw(
    ui: &egui::Ui,
    bar: egui::Rect,
    nav: &[(Nav, egui::Rect)],
    state: &State,
    hover: Option<egui::Pos2>,
) {
    let ink = super::ground_ink::cube();
    let stroke = egui::Stroke::new(1.0, ink.edge);
    let g = ink.button;
    ui.painter().rect_filled(
        bar,
        BAR_RADIUS,
        egui::Color32::from_rgba_unmultiplied(g, g, g, 160),
    );
    for &(kind, r) in nav {
        let active = match kind {
            Nav::Frame | Nav::Iso => false,
            Nav::Wire => state.display.is_wireframe(),
            Nav::Display => state.display_menu_open,
            Nav::Paths => state.show_all_paths,
            Nav::Axes => state.show_world_axes,
            Nav::PickThrough => state.pick_through,
            Nav::Move => state.move_handles,
            Nav::Rotate => state.rotate_handles,
            Nav::Projection => state.cam.projection == ProjectionMode::Orthographic,
            Nav::Orbit => state.nav_tool == Some(DragMode::Orbit),
            Nav::Pan => state.nav_tool == Some(DragMode::Pan),
            Nav::Zoom => state.nav_tool == Some(DragMode::Zoom),
        };
        let hov = hover.is_some_and(|p| r.contains(p));
        let lab = label(kind);
        let tip = tooltip(kind, state);
        let resp = ui.interact(r, ui.id().with(("viewport_nav", kind)), egui::Sense::hover());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, lab));
        if hov && state.drag.is_none() {
            resp.on_hover_text(tip);
        }
        let fill = if hov { ink.button.saturating_add(28) } else { ink.button };
        ui.painter().rect_filled(r, 5.0, egui::Color32::from_gray(fill));
        if ink.outline_buttons {
            // The light ground outlines the button like a cube face.
            ui.painter().rect_stroke(r, 5.0, stroke, egui::StrokeKind::Inside);
        }
        let s = egui::Stroke::new(1.4, if active { ink.glyph_on } else { ink.glyph_off });
        glyphs::draw(ui, kind, r, s, ink.triad, state.cam.projection);
    }
}

#[cfg(test)]
#[path = "viewport_bar_tests.rs"]
mod tests;

#[path = "viewport_bar_glyphs.rs"]
mod glyphs;

#[path = "viewport_bar_controls.rs"]
pub(crate) mod controls;

pub(crate) use controls::HandleBarState;

#[path = "viewport_bar_text.rs"]
pub(crate) mod text;

pub(crate) use text::*;
