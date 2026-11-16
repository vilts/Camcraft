//! The view cube and the viewport navigation bar (M1.5).
//!
//! Provenance: the standard-orientation affordance of interactive 3D
//! CAD - a clickable cube whose faces, edges and corners name the
//! axis-aligned views, drawn from the camera's own rotation - written
//! from published CAD practice, no vendor material.
//!
//! The law lives here in one file and has three parts. `Hud::new` lays
//! the cube (top-right; the pane's bottom-right is the control hint's)
//! and the bottom control bar (`viewport_bar`, #1198) out of the pane
//! rect alone -
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

pub(crate) use super::viewport_bar::Nav;
#[cfg(test)]
pub(crate) use super::viewport_bar::pick_through_label;
use super::viewport_bar;
use crate::actions;
use crate::state::State;
use crate::viewport::Camera;

/// Side of the cube's screen footprint. A pane too small to host the
/// furniture (narrow docked window, the tiny harness before layout) gets
/// no hud at all - the viewport keeps every gesture it had.
pub(crate) const CUBE_PX: f32 = 108.0;
const MARGIN: f32 = 10.0;

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

/// The narrowest pane that hosts the furniture: the wider of the cube and
/// the ribbon, plus the margin on each side.
#[must_use]
pub(crate) fn min_width() -> f32 {
    CUBE_PX.max(viewport_bar::BAR_W) + 2.0 * MARGIN
}

pub(crate) struct Hud {
    pub cube: egui::Rect,
    /// The bottom control ribbon (`viewport_bar`), grey background.
    bar: egui::Rect,
    nav: Vec<(Nav, egui::Rect)>,
}

impl Hud {
    /// Corner furniture of `pane`, or None when the pane is too small: the
    /// cube top-right and the control ribbon along the bottom. Pure
    /// arithmetic on the rect - the pointer layer calls it before routing
    /// presses, the draw pass after, and both get the same rects.
    #[must_use]
    pub fn new(pane: egui::Rect) -> Option<Hud> {
        if pane.width() < min_width() || pane.height() < 260.0 {
            return None;
        }
        let cube = egui::Rect::from_min_size(
            egui::pos2(pane.max.x - MARGIN - CUBE_PX, pane.min.y + MARGIN),
            egui::vec2(CUBE_PX, CUBE_PX),
        );
        let bar = viewport_bar::bar_rect(pane);
        let nav = viewport_bar::buttons(bar);
        Some(Hud { cube, bar, nav })
    }

    #[must_use]
    pub fn contains(&self, p: egui::Pos2) -> bool {
        self.cube.contains(p) || self.bar.contains(p)
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
            viewport_bar::act(state, *kind);
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

    viewport_bar::draw(ui, h.bar, &h.nav, state, hover);
}
