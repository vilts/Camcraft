//! The viewport's bottom control bar (#1198 slice a): a rounded grey
//! ribbon floating over the bottom edge of the 3D pane, carrying the view
//! buttons that used to stack under the view cube - frame, iso,
//! shaded/wireframe, toolpaths and world axes.
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
/// Ten buttons in a row: the ribbon's width and height, so the layout
/// gate can ask the pane for room (pinned by `bar_fits_its_buttons`).
pub(crate) const BAR_W: f32 = 10.0 * NAV_PX + 9.0 * NAV_GAP + 2.0 * PAD;
const BAR_H: f32 = NAV_PX + 2.0 * PAD;
const BAR_RADIUS: f32 = 8.0;

/// Camera buttons (#1198 follow-up): the projection is a direct
/// view-state write like the display-mode toggle; the orbit / pan /
/// zoom tools latch the drag onto that mode and leave the shortcut to
/// the vertex (a tool latched is a tool any drag obeys). View state
/// like the camera - never commands, never saved.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Nav {
    /// Reframe the whole document - the F key's button twin.
    Frame,
    /// The house isometric, yaw 0.9 / pitch 0.5.
    Iso,
    /// Shaded-with-wire <-> hidden-line wireframe (Solids radio's twin).
    Wire,
    /// Draw every cached path, not just the selected op's.
    Paths,
    /// The dimmed world origin axes.
    Axes,
    /// Viewport picks may reach the far side of the part (#692). Off by
    /// default; when on, a click sends `through: true`.
    PickThrough,
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
pub(crate) const NAV_COUNT: usize = 10;
pub(crate) const NAVS: [Nav; NAV_COUNT] = [
    Nav::Frame,
    Nav::Iso,
    Nav::Wire,
    Nav::Paths,
    Nav::Axes,
    Nav::PickThrough,
    Nav::Projection,
    Nav::Orbit,
    Nav::Pan,
    Nav::Zoom,
];

/// The plain-English state line of the pick-through button (#692).
#[must_use]
pub(crate) fn pick_through_label(on: bool) -> &'static str {
    if on { "Pick through: on" } else { "Pick through: off" }
}

/// The status line the projection button flips the viewer between.
#[must_use]
pub(crate) fn projection_label(mode: ProjectionMode) -> &'static str {
    match mode {
        ProjectionMode::Perspective => "projection: perspective",
        ProjectionMode::Orthographic => "projection: orthographic",
    }
}

/// The status line a latched nav tool wears on the status bar.
#[must_use]
pub(crate) fn nav_tool_label(tool: DragMode) -> &'static str {
    match tool {
        DragMode::Orbit => "nav tool: orbit",
        DragMode::Pan => "nav tool: pan",
        DragMode::Zoom => "nav tool: zoom",
    }
}

/// The status line when a nav tool latches off.
pub(crate) const NAV_TOOL_CLEAR: &str = "nav tool: off";

/// The ribbon: centred on the pane's width, resting on its bottom edge at
/// the same margin as before. Floats over the 3D view (#1198, maintainer:
/// "bottom-center floating ontop of the 3D viewer").
#[must_use]
pub(crate) fn bar_rect(pane: egui::Rect) -> egui::Rect {
    let min = egui::pos2(pane.center().x - BAR_W * 0.5, pane.max.y - BOTTOM_MARGIN - BAR_H);
    egui::Rect::from_min_size(min, egui::vec2(BAR_W, BAR_H))
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
                DisplayMode::ShadedEdges => DisplayMode::Wireframe,
                DisplayMode::Wireframe => DisplayMode::ShadedEdges,
            };
        }
        Nav::Paths => state.toggle_all_toolpaths(),
        Nav::Axes => state.show_world_axes = !state.show_world_axes,
        Nav::PickThrough => {
            state.pick_through = !state.pick_through;
            state.status = pick_through_label(state.pick_through).to_string();
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

/// Paint the ribbon and its buttons. Pure state -> paint, like the cube.
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
            Nav::Wire => state.display == DisplayMode::Wireframe,
            Nav::Paths => state.show_all_paths,
            Nav::Axes => state.show_world_axes,
            Nav::PickThrough => state.pick_through,
            Nav::Projection => state.cam.projection == ProjectionMode::Orthographic,
            Nav::Orbit => state.nav_tool == Some(DragMode::Orbit),
            Nav::Pan => state.nav_tool == Some(DragMode::Pan),
            Nav::Zoom => state.nav_tool == Some(DragMode::Zoom),
        };
        let hov = hover.is_some_and(|p| r.contains(p));
        let fill = if hov { ink.button.saturating_add(28) } else { ink.button };
        ui.painter().rect_filled(r, 5.0, egui::Color32::from_gray(fill));
        if ink.outline_buttons {
            // The light ground outlines the button like a cube face.
            ui.painter().rect_stroke(r, 5.0, stroke, egui::StrokeKind::Inside);
        }
        let s = egui::Stroke::new(1.4, if active { ink.glyph_on } else { ink.glyph_off });
        let (a, b) = (r.min, r.max);
        let m = NAV_PX * 0.22;
        match kind {
            Nav::Frame => {
                // Four corner brackets around the button's inset square.
                let i0 = a + egui::vec2(m * 0.45, m * 0.45);
                let i1 = b - egui::vec2(m * 0.45, m * 0.45);
                for (p, dx, dy) in [
                    (i0, 1.0f32, 1.0f32),
                    (egui::pos2(i1.x, i0.y), -1.0, 1.0),
                    (egui::pos2(i0.x, i1.y), 1.0, -1.0),
                    (i1, -1.0, -1.0),
                ] {
                    let hx = egui::vec2(dx * m, 0.0);
                    let hy = egui::vec2(0.0, dy * m);
                    ui.painter().line_segment([p + hx, p + hx + hy * 0.62], s);
                    ui.painter().line_segment([p + hy, p + hx * 0.62 + hy], s);
                }
            }
            Nav::Iso => {
                // The cube from the house diagonal: hexagon silhouette,
                // three visible faces' inner edges meeting at the center.
                let c = r.center();
                let rr = NAV_PX * 0.30;
                let hex: Vec<egui::Pos2> = (0..6)
                    .map(|k| {
                        let t = std::f64::consts::FRAC_PI_3 * f64::from(k) + 0.261_799;
                        c + egui::vec2(
                            (t.cos() * f64::from(rr)) as f32,
                            (t.sin() * f64::from(rr)) as f32,
                        )
                    })
                    .collect();
                ui.painter().add(egui::Shape::convex_polygon(
                    hex.clone(),
                    egui::Color32::TRANSPARENT,
                    s,
                ));
                for k in [0usize, 2, 4] {
                    ui.painter().line_segment([c, hex[k]], s);
                }
            }
            Nav::Wire => {
                let q0 = a + egui::vec2(m * 0.62, m * 0.62);
                let q1 = b - egui::vec2(m * 0.62, m * 0.62);
                ui.painter().rect_stroke(
                    egui::Rect::from_min_max(q0, q1),
                    1.0,
                    s,
                    egui::StrokeKind::Inside,
                );
                ui.painter().line_segment([q0, q1], s);
                ui.painter()
                    .line_segment([egui::pos2(q1.x, q0.y), egui::pos2(q0.x, q1.y)], s);
            }
            Nav::Paths => {
                // A zigzag toolpath.
                let p0 = egui::pos2(a.x + m * 0.4, r.center().y);
                let step = (r.width() - m * 0.8) / 4.0;
                let pts: Vec<egui::Pos2> = (0..5)
                    .map(|k| {
                        p0 + egui::vec2(
                            step * k as f32,
                            if k % 2 == 0 { 0.0 } else { -r.height() * 0.20 },
                        )
                    })
                    .collect();
                ui.painter().add(egui::Shape::line(pts, s));
            }
            Nav::Axes => {
                // The origin triad: an isometric triad, equal arms 120 deg
                // apart, Z up, X and Y 30 deg below the horizontal; the box
                // spans y -l..l/2, so the origin sits l/4 below the centre.
                let l = NAV_PX * 0.32;
                let (c30, s30) = (l * 0.866, l * 0.5);
                let o = r.center() + egui::vec2(0.0, l * 0.25);
                let [x, y, z] = ink.triad;
                for (dir, col) in [
                    (egui::vec2(c30, s30), x),
                    (egui::vec2(-c30, s30), y),
                    (egui::vec2(0.0, -l), z),
                ] {
                    ui.painter()
                        .line_segment([o, o + dir], egui::Stroke::new(1.4, col));
                }
            }
            Nav::PickThrough => {
                // A pick ray crossing a bar: the ray runs through, the bar
                // is the near surface it passes.
                let c = r.center();
                let half = r.width() * 0.5 - m;
                ui.painter().line_segment([c - egui::vec2(half, 0.0), c + egui::vec2(half, 0.0)], s);
                ui.painter().line_segment([c - egui::vec2(m * 0.4, m), c + egui::vec2(m * 0.4, m)], s);
            }
            Nav::Projection => {
                // The two doors on one viewer: converging bars for
                // perspective, a plain square for orthographic.
                let c = r.center();
                let half = r.width() * 0.5 - m;
                if matches!(state.cam.projection, ProjectionMode::Orthographic) {
                    ui.painter().rect_stroke(
                        egui::Rect::from_center_size(c, egui::vec2(half * 1.7, half * 1.7)),
                        1.0,
                        s,
                        egui::StrokeKind::Inside,
                    );
                } else {
                    let top = c.y - half * 0.8;
                    let bot = c.y + half * 0.8;
                    let tip_top = half;
                    let tip_bot = half * 0.4;
                    ui.painter().line_segment([egui::pos2(c.x - tip_top, top), egui::pos2(c.x - tip_bot, bot)], s);
                    ui.painter().line_segment([egui::pos2(c.x + tip_top, top), egui::pos2(c.x + tip_bot, bot)], s);
                    ui.painter().line_segment([egui::pos2(c.x - tip_top, top), egui::pos2(c.x + tip_top, top)], s);
                    ui.painter().line_segment([egui::pos2(c.x - tip_bot, bot), egui::pos2(c.x + tip_bot, bot)], s);
                }
            }
            Nav::Orbit => {
                // The orbit ring with the eye-satellite riding it.
                let c = r.center();
                let rad = NAV_PX * 0.27;
                ui.painter().circle_stroke(c, rad, s);
                let a = std::f32::consts::FRAC_PI_4;
                let dot = c + egui::vec2(a.cos() * rad, -a.sin() * rad);
                ui.painter().circle_filled(dot, 2.4, s.color);
                ui.painter().arrow(dot, egui::vec2(a.sin(), a.cos()) * 5.0, s);
            }
            Nav::Pan => {
                // A pair of rails bowed out: the move arrows' move.
                let c = r.center();
                let l = NAV_PX * 0.24;
                for (origin, vec) in [
                    (c, egui::vec2(l, 0.0)),
                    (c, egui::vec2(-l, 0.0)),
                    (c, egui::vec2(0.0, l)),
                    (c, egui::vec2(0.0, -l)),
                ] {
                    ui.painter().arrow(origin, vec, s);
                }
            }
            Nav::Zoom => {
                // A magnifier: ring plus the handle it hangs from.
                let c = r.center() - egui::vec2(2.5, 2.5);
                ui.painter().circle_stroke(c, NAV_PX * 0.21, s);
                ui.painter().line_segment(
                    [
                        c + egui::vec2(NAV_PX * 0.15, NAV_PX * 0.15),
                        c + egui::vec2(NAV_PX * 0.30, NAV_PX * 0.30),
                    ],
                    s,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_buttons_flip_the_viewer_and_latch_a_tool() {
        // Projection: flips perspective <-> orthographic, the View
        // door's radios' true twin.
        let mut state = State::new();
        let p0 = state.cam.projection;
        act(&mut state, Nav::Projection);
        assert_ne!(state.cam.projection, p0, "the projection must flip");
        assert!(state.status.contains("orthographic"), "{:?}", state.status);
        act(&mut state, Nav::Projection);
        assert_eq!(state.cam.projection, p0, "one more press flips back");
        // The tool buttons latch once on and once off, and say so.
        for (nav, tool, word) in [
            (Nav::Orbit, DragMode::Orbit, "orbit"),
            (Nav::Pan, DragMode::Pan, "pan"),
            (Nav::Zoom, DragMode::Zoom, "zoom"),
        ] {
            let mut state = State::new();
            act(&mut state, nav);
            assert_eq!(state.nav_tool, Some(tool));
            assert!(state.status.contains(word), "{:?}", state.status);
            // Latching a different tool replaces the first.
            act(&mut state, if matches!(nav, Nav::Zoom) { Nav::Orbit } else { Nav::Zoom });
            assert_ne!(state.nav_tool, Some(tool));
            // The second press on the same tool latches off.
            act(&mut state, nav);
            assert_eq!(state.nav_tool, Some(tool));
            act(&mut state, nav);
            assert_eq!(state.nav_tool, None);
            assert_eq!(state.status, NAV_TOOL_CLEAR);
        }
    }

    #[test]
    fn bar_fits_its_buttons() {
        assert_eq!(NAVS.len(), NAV_COUNT, "NAV_COUNT sizes the ribbon");
        let count = NAVS.len() as f32;
        let width = count * NAV_PX + (count - 1.0) * NAV_GAP + 2.0 * PAD;
        assert!((BAR_W - width).abs() < 1e-6, "the ribbon is as wide as its buttons");
        let pane = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 600.0));
        let bar = bar_rect(pane);
        assert!((bar.width() - BAR_W).abs() < 1e-6);
        assert!(
            (bar.center().x - pane.center().x).abs() < 1e-3,
            "the ribbon is centred on the bottom edge"
        );
        assert!(
            (bar.max.y - (pane.max.y - BOTTOM_MARGIN)).abs() < 1e-3,
            "it rests on the bottom edge"
        );
        let kids = buttons(bar);
        assert_eq!(kids.len(), NAVS.len());
        assert!(kids.iter().all(|(_, r)| bar.contains(r.min) && bar.contains(r.max)));
        let last = kids.last().map(|(k, r)| (*k, *r));
        assert_eq!(
            last.map(|(k, _)| k),
            NAVS.last().copied(),
            "the last button is the last in the list"
        );
        assert!(
            last.is_some_and(|(_, r)| bar.contains(r.max)),
            "the last button sits inside the ribbon"
        );
    }
}
