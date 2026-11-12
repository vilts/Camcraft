//! The view cube and the viewport navigation bar (M1.5).
//!
//! Provenance: the standard-orientation affordance of interactive 3D
//! CAD - a clickable cube whose faces, edges and corners name the
//! axis-aligned views, drawn from the camera's own rotation - written
//! from published CAD practice, no vendor material.
//!
//! The law lives here in one file and has three parts. `Hud::new` lays
//! the cube (top-right; the pane's bottom-right is the control hint's)
//! and the nav column under it out of the pane rect alone -
//! deterministic, so the pointer layer and the tests agree on every
//! pixel without talking to each other. `hit_dir` classifies a cube
//! click by ray-casting the point through the unit cube: one coordinate
//! past the margin is a face (that axis view), two are an edge (the 45°
//! two-view bisector), three a corner (the trihedral diagonal), and
//! nothing is a miss. `draw` paints the same geometry it hit-tests -
//! faces back-to-front with a fixed light, labels on the faces that
//! read, the hovered feature lit - so what can be clicked is exactly
//! what is visible.
//!
//! Everything it changes is view state, and orientation and `FrameAll`
//! go through `actions` (the effect-once law); the three display flips
//! write their fields directly exactly as panes.rs's radios and the
//! shortcut layer's flips do - the same fields, the same
//! immediate-mode exemption the camera has (state.rs): never a
//! command, never saved.

use crate::actions;
use crate::state::{DisplayMode, State};
use crate::viewport::Camera;

/// Side of the cube's screen footprint, and the nav buttons stacked
/// under it. A pane too small to host the furniture (narrow docked
/// window, the tiny harness before layout) gets no hud at all - the
/// viewport keeps every gesture it had.
pub(crate) const CUBE_PX: f32 = 108.0;
const MARGIN: f32 = 10.0;
const NAV_PX: f32 = 26.0;
const NAV_GAP: f32 = 4.0;

/// How close (in cube half-units - the cube spans [-1,1]^3) a ray's
/// entry coordinate must come to a face to count as riding that
/// feature: a ~12 px-wide edge band on the default cube, wide enough to
/// click without hunting, narrow enough that face centers read as faces.
const SNAP: f64 = 0.16;

/// The eight corners, in cube units.
const CORNERS: [[f64; 3]; 8] = [
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, 1.0],
];

/// The six face normals in cube units with the labels that name them
/// (original UI strings; the app's Z-up convention: FRONT looks along
/// +Y, so a click commands `Camera::toward([0,-1,0])` - the eye lands
/// south of the part and faces north).
const FACES: [([f64; 3], &str); 6] = [
    ([0.0, 0.0, 1.0], "TOP"),
    ([0.0, 0.0, -1.0], "BOT"),
    ([1.0, 0.0, 0.0], "RIGHT"),
    ([-1.0, 0.0, 0.0], "LEFT"),
    ([0.0, 1.0, 0.0], "BACK"),
    ([0.0, -1.0, 0.0], "FRONT"),
];

/// The four `CORNERS` bounding each face, one row per `FACES` entry (the
/// cube's faces are convex, so any cyclic order draws the same quad).
const FACE_QUADS: [[usize; 4]; 6] = [
    [4, 5, 6, 7], // TOP   z+
    [0, 3, 2, 1], // BOT   z-
    [1, 2, 6, 5], // RIGHT x+
    [0, 4, 7, 3], // LEFT  x-
    [3, 2, 6, 7], // BACK  y+
    [0, 1, 5, 4], // FRONT y-
];

pub(crate) struct Hud {
    pub cube: egui::Rect,
    nav: Vec<(Nav, egui::Rect)>,
}

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
}

pub(crate) const NAVS: [Nav; 6] = [Nav::Frame, Nav::Iso, Nav::Wire, Nav::Paths, Nav::Axes, Nav::PickThrough];

/// The plain-English state line of the pick-through button (#692).
#[must_use]
pub(crate) fn pick_through_label(on: bool) -> &'static str {
    if on { "Pick through: on" } else { "Pick through: off" }
}

impl Hud {
    /// Top-right corner furniture of `pane`, or None when the pane is
    /// too small. Pure arithmetic on the rect - the pointer layer calls
    /// it before routing presses, the draw pass after, and both get the
    /// same rects.
    #[must_use]
    pub fn new(pane: egui::Rect) -> Option<Hud> {
        if pane.width() < CUBE_PX + 2.0 * MARGIN || pane.height() < 260.0 {
            return None;
        }
        let cube = egui::Rect::from_min_size(
            egui::pos2(pane.max.x - MARGIN - CUBE_PX, pane.min.y + MARGIN),
            egui::vec2(CUBE_PX, CUBE_PX),
        );
        let nav = NAVS
            .iter()
            .enumerate()
            .map(|(i, &kind)| {
                let y = cube.max.y + 8.0 + i as f32 * (NAV_PX + NAV_GAP);
                (
                    kind,
                    egui::Rect::from_min_size(
                        egui::pos2(cube.max.x - NAV_PX, y),
                        egui::vec2(NAV_PX, NAV_PX),
                    ),
                )
            })
            .collect();
        Some(Hud { cube, nav })
    }

    #[must_use]
    pub fn contains(&self, p: egui::Pos2) -> bool {
        self.cube.contains(p) || self.nav.iter().any(|(_, r)| r.contains(p))
    }

    /// Where a nav button lives in this layout - the probes the
    /// gesture tests use to aim a click at one without recomputing the
    /// arithmetic (nothing in the live frame asks).
    #[cfg(test)]
    #[must_use]
    pub fn nav_rect(&self, kind: Nav) -> egui::Rect {
        self.nav
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(egui::Rect::ZERO, |(_, r)| *r)
    }

    /// Cube center and px-per-cube-unit scale, shared by painter and
    /// pick: a cube-unit point q projects to
    /// `center + vec2(dot(q, right), -dot(q, up)) * scale`. A corner
    /// (|q| = 3 coordinates at 1) then spans the rect edge to corner -
    /// the sqrt(3) is exactly that fit.
    #[must_use]
    fn frame(&self) -> (egui::Pos2, f32) {
        (self.cube.center(), self.cube.width() * 0.5 / 3.0f32.sqrt())
    }
}

/// Screen point of a cube-unit point - how the tests aim a click at the
/// center of a named face without reading the painter's mind.
#[must_use]
pub fn point_screen(cam: &Camera, h: &Hud, q: [f64; 3]) -> egui::Pos2 {
    let (center, scale) = h.frame();
    let ((rx, ry, rz), (ux, uy, uz), _) = cam.view_basis();
    let x = rx * q[0] + ry * q[1] + rz * q[2];
    let y = ux * q[0] + uy * q[1] + uz * q[2];
    egui::pos2(
        center.x + (x * f64::from(scale)) as f32,
        center.y - (y * f64::from(scale)) as f32,
    )
}

/// Which world direction a cube click commands (unit vector, cube
/// axes). None = past the silhouette. The pointer layer consumes a miss
/// too - a stray press on the furniture never orbits or picks.
#[must_use]
pub fn hit_dir(cam: &Camera, h: &Hud, p: egui::Pos2) -> Option<[f64; 3]> {
    let (center, scale) = h.frame();
    let ((rx, ry, rz), (ux, uy, uz), (fx, fy, fz)) = cam.view_basis();
    if scale <= 0.0 {
        return None;
    }
    // Screen px -> the world point of the view plane it stands on
    // (spanned by right and up), in cube units.
    let sx = f64::from((p.x - center.x) / scale);
    let sy = f64::from((center.y - p.y) / scale);
    let o = [rx * sx + ux * sy, ry * sx + uy * sy, rz * sx + uz * sy];
    // Orthographic ray through that plane point along +fwd. `o` sits on
    // the plane through the cube center (o dot f = 0), so the cast must
    // START on the eye side - p0 = o - 3f stands past every corner
    // (the cube's farthest point is sqrt(3) < 3 cube units out) - and
    // enter the box front-first. The entry point's coordinates classify
    // the feature.
    let f = [fx, fy, fz];
    let p0 = [o[0] - 3.0 * f[0], o[1] - 3.0 * f[1], o[2] - 3.0 * f[2]];
    let (mut t_near, mut t_far) = (0.0f64, 6.0f64);
    for i in 0..3 {
        if f[i].abs() < 1e-12 {
            if p0[i].abs() > 1.0 {
                return None;
            }
            continue;
        }
        let t1 = (-1.0 - p0[i]) / f[i];
        let t2 = (1.0 - p0[i]) / f[i];
        t_near = t_near.max(t1.min(t2));
        t_far = t_far.min(t1.max(t2));
    }
    if t_near > t_far {
        return None;
    }
    let q = [
        p0[0] + f[0] * t_near,
        p0[1] + f[1] * t_near,
        p0[2] + f[2] * t_near,
    ];
    let sign = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
    let past: Vec<usize> = (0..3).filter(|&i| q[i].abs() >= 1.0 - SNAP).collect();
    let mut d = [0.0f64; 3];
    match past.len() {
        0 => return None, // grazed past the slab; nothing to command
        1 => d[past[0]] = sign(q[past[0]]),
        _ => {
            for &i in &past {
                d[i] = sign(q[i]);
            }
        }
    }
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if l <= f64::EPSILON {
        return None;
    }
    Some([d[0] / l, d[1] / l, d[2] / l])
}

/// A cube click or nav press, resolved and dispatched - the pointer
/// layer's single door into the furniture. True = consumed.
pub(crate) fn click(state: &mut State, h: &Hud, p: egui::Pos2) -> bool {
    // Widgets own their pixels before the cube claims any: a nav button
    // sits below the cube's box, but its pixel's 3-D ray still strikes
    // the cube's volume - the raycast must never outrank the button.
    for (kind, r) in &h.nav {
        if r.contains(p) {
            act(state, *kind);
            return true;
        }
    }
    if h.cube.contains(p)
        && let Some(d) = hit_dir(&state.cam, h, p)
    {
        actions::orient_view(state, d);
        return true;
    }
    h.contains(p)
}

/// The nav buttons' effects, once each. `FrameAll` is a queued command
/// like the F key's; the toggles are direct view-state writes exactly
/// as panes.rs flips them (never commands - the camera's exemption).
fn act(state: &mut State, kind: Nav) {
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
    }
}

/// Paint the furniture. Pure state -> paint: queues nothing, touches
/// nothing. `hover` (cursor inside the hud) lights the feature the next
/// click would command - the cube's own affordance.
pub(crate) fn draw(
    ui: &egui::Ui,
    h: &Hud,
    cam: &Camera,
    state: &State,
    hover: Option<egui::Pos2>,
    font: egui::FontId,
) {
    let (_, _, (fx, fy, fz)) = cam.view_basis();
    let hover_dir = hover
        .filter(|p| h.cube.contains(*p))
        .and_then(|p| hit_dir(cam, h, p));

    // Visible faces back-to-front: the one whose unit normal points
    // deepest into the screen (+fwd) draws first.
    let mut order: Vec<(usize, f64)> = FACES
        .iter()
        .enumerate()
        .map(|(i, (n, _))| {
            (
                i,
                n[0] * fx + n[1] * fy + n[2] * fz,
            )
        })
        .collect();
    order.sort_by(|a, b| b.1.total_cmp(&a.1));
    let ink = super::ground_ink::cube();
    let stroke = egui::Stroke::new(1.0, ink.edge);
    for &(i, depth) in &order {
        if depth >= 0.0 {
            continue; // back face
        }
        let (n, label) = FACES[i];
        // A fixed head-light bias gives the three axis pairs three
        // readable grays, so the cube's faces stay identifiable by
        // tone alone once the camera tumbles.
        let t = 0.5 + 0.5 * (n[2] * 0.82 + n[0] * 0.33 - n[1] * 0.46);
        let mut g = (ink.face_base + ink.face_span * t.clamp(0.0, 1.0) as f32) as u8;
        let lit = hover_dir.is_some_and(|d| {
            d[0] * n[0] + d[1] * n[1] + d[2] * n[2] > 0.999
                && (d[0] * d[0] + d[1] * d[1] + d[2] * d[2] - 1.0).abs() < 0.01
        });
        if lit {
            g = g.saturating_add(ink.hover_lift);
        }
        let quad = FACE_QUADS[i];
        let pts: Vec<egui::Pos2> = quad.iter().map(|&k| point_screen(cam, h, CORNERS[k])).collect();
        ui.painter()
            .add(egui::Shape::convex_polygon(pts, egui::Color32::from_gray(g), stroke));
        if -depth > 0.15 {
            // A near-edge-on face carries no readable label.
            ui.painter().text(
                point_screen(
                    cam,
                    h,
                    [n[0] * 0.62, n[1] * 0.62, n[2] * 0.62],
                ),
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                ink.label,
            );
        }
    }
    if let Some(d) = hover_dir {
        // Push the unit direction out to the surface it names: face
        // center, edge midpoint, corner - exactly where it was clicked.
        let m = d[0].abs().max(d[1].abs()).max(d[2].abs()).max(f64::EPSILON);
        let q = [d[0] / m, d[1] / m, d[2] / m];
        ui.painter()
            .circle_filled(point_screen(cam, h, q), 3.0, egui::Color32::from_rgb(255, 217, 102));
    }

    for (kind, r) in &h.nav {
        let active = match kind {
            Nav::Frame | Nav::Iso => false,
            Nav::Wire => state.display == DisplayMode::Wireframe,
            Nav::Paths => state.show_all_paths,
            Nav::Axes => state.show_world_axes,
            Nav::PickThrough => state.pick_through,
        };
        let hov = hover.is_some_and(|p| r.contains(p));
        let fill = if hov { ink.button.saturating_add(28) } else { ink.button };
        ui.painter().rect_filled(*r, 5.0, egui::Color32::from_gray(fill));
        if ink.outline_buttons {
            // The light ground outlines the button like a cube face.
            ui.painter().rect_stroke(*r, 5.0, stroke, egui::StrokeKind::Inside);
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
                // The origin triad: X down-right, Y down-left, Z up.
                // An isometric triad: equal arms 120 deg apart, Z up, X and Y
                // 30 deg below the horizontal; the box spans y -l..l/2, so the
                // origin sits l/4 below the centre to centre it.
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
        }
    }
}
