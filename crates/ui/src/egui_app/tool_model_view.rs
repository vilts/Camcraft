//! #3894: the small orbitable model view of a tool, shared by the body
//! placement view (#371, `tool_body_page`) and the tool library window's
//! 3D preview. Painted with egui - flat-shaded, depth-sorted triangles
//! (the painter's algorithm) - so it needs no GPU and runs under the test
//! harness. The orbit, zoom and pan are view state (`state::Orbit`); a
//! gesture never queues a command.
//! Provenance: our own view, moved out of `tool_body_page`; orthographic
//! projection and ray casting from general graphics practice. No vendor
//! material consulted.

use egui::{Color32, Mesh, Pos2, Rect, Response, Ui, Vec2};

use camcraft_app::geometry::Triangle;

use crate::state::Orbit;

/// The background of every tool preview.
pub(super) const BG: Color32 = Color32::from_rgb(24, 26, 31);
/// Past this many facets the view paints every n-th one (picking still
/// reads them all) - a vendor STEP can carry 10^5+.
pub(crate) const MAX_PAINTED: usize = 30_000;
/// The default view: a three-quarter look from +X-Y, above.
const YAW0: f32 = -0.9;
const PITCH0: f32 = 0.45;

/// The orthographic view: a center, a scale, and the right/up/forward
/// basis (forward points into the screen).
pub(super) struct View {
    c: [f64; 3],
    s: f64,
    r: [f64; 3],
    up: [f64; 3],
    pub(super) f: [f64; 3],
    mid: Pos2,
    pub(super) reach: f64,
}

impl View {
    /// The view that fits `tris` (and the model origin) into `rect`,
    /// turned and zoomed by `orbit`. `None` for an empty or degenerate mesh.
    pub(super) fn new(tris: &[Triangle], rect: Rect, orbit: &Orbit) -> Option<Self> {
        let (lo, hi) = bounds(tris);
        Self::fit(lo, hi, rect, orbit)
    }

    /// As `new`, from the mesh's `bounds` (a caller that caches them
    /// walks no facet on a still frame).
    pub(super) fn fit(lo: [f64; 3], hi: [f64; 3], rect: Rect, orbit: &Orbit) -> Option<Self> {
        let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
        let reach = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() * 0.5;
        if !(reach.is_finite() && reach > 1e-9) {
            return None;
        }
        let (y, p) = (f64::from(YAW0 + orbit.yaw), f64::from((PITCH0 + orbit.pitch).clamp(-1.5, 1.5)));
        let f = [-(p.cos() * y.cos()), -(p.cos() * y.sin()), -p.sin()];
        let r = norm([f[1], -f[0], 0.0]).unwrap_or([1.0, 0.0, 0.0]);
        let up = cross(r, f);
        let zoom = f64::from(crate::state::clamp_zoom(orbit.zoom, 0.0)).exp2();
        let s = f64::from(rect.width().min(rect.height())) * 0.45 / reach * zoom;
        let pan = if orbit.pan.iter().all(|v| v.is_finite()) { orbit.pan } else { [0.0; 2] };
        let c = [0, 1, 2].map(|i| c[i] + r[i] * pan[0] + up[i] * pan[1]);
        Some(Self { c, s, r, up, f, mid: rect.center(), reach })
    }

    pub(super) fn screen(&self, q: [f64; 3]) -> Pos2 {
        let d = [q[0] - self.c[0], q[1] - self.c[1], q[2] - self.c[2]];
        Pos2::new(
            self.mid.x + (dot(d, self.r) * self.s) as f32,
            self.mid.y - (dot(d, self.up) * self.s) as f32,
        )
    }

    fn depth(&self, q: [f64; 3]) -> f64 {
        dot([q[0] - self.c[0], q[1] - self.c[1], q[2] - self.c[2]], self.f)
    }

    /// Screen point `at` as view units (right, up) off the view centre.
    pub(super) fn offset(&self, at: Pos2) -> [f64; 2] {
        [f64::from(at.x - self.mid.x) / self.s, -f64::from(at.y - self.mid.y) / self.s]
    }

    /// The ray under screen point `at`: from in front of the body, into it.
    pub(super) fn ray(&self, at: Pos2) -> ([f64; 3], [f64; 3]) {
        let (dx, dy) = (f64::from(at.x - self.mid.x) / self.s, -f64::from(at.y - self.mid.y) / self.s);
        let back = 4.0 * self.reach;
        let o = [0, 1, 2].map(|i| self.c[i] + self.r[i] * dx + self.up[i] * dy - self.f[i] * back);
        (o, self.f)
    }
}

/// The box of `tris` and the model origin (the axis and tip live there):
/// the low and the high corner. Non-finite coordinates are skipped.
pub(super) fn bounds(tris: &[Triangle]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for t in tris {
        for p in [t.a, t.b, t.c] {
            for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                if v.is_finite() {
                    lo[i] = lo[i].min(v);
                    hi[i] = hi[i].max(v);
                }
            }
        }
    }
    for i in 0..3 {
        lo[i] = lo[i].min(0.0);
        hi[i] = hi[i].max(0.0);
    }
    (lo, hi)
}

/// The screen mesh of `tris` in `view`: far facets first, each shaded by
/// how squarely it faces the eye.
pub(super) fn project(view: &View, tris: &[Triangle]) -> Mesh {
    let stride = tris.len().div_ceil(MAX_PAINTED).max(1);
    let mut order: Vec<(f64, &Triangle)> = tris
        .iter()
        .step_by(stride)
        .map(|t| (view.depth(centroid(t)), t))
        .filter(|(z, _)| z.is_finite())
        .collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut mesh = Mesh::default();
    for (_, t) in order {
        let [a, b, c] = [t.a, t.b, t.c].map(|p| [p.x, p.y, p.z]);
        let lit = norm(cross(sub(b, a), sub(c, a))).map_or(0.3, |n| dot(n, view.f).abs());
        let g = (60.0 + 150.0 * lit).clamp(0.0, 255.0) as u8;
        let col = Color32::from_rgb(g, g, g.saturating_add(12));
        let base = mesh.vertices.len() as u32;
        for q in [a, b, c] {
            mesh.colored_vertex(view.screen(q), col);
        }
        mesh.add_triangle(base, base + 1, base + 2);
    }
    mesh
}

/// What the pointer asked of the view this frame. View state only.
#[derive(Default)]
pub(super) struct Gesture {
    /// Zoom by (log2) about the view offset under the cursor.
    pub zoom: Option<(f32, [f64; 2])>,
    /// A double-click: the default view again.
    pub fit: bool,
    /// A drag, screen px: it turns the view.
    pub turn: Option<Vec2>,
}

/// Read the zoom (the wheel or a pinch, also Ctrl+wheel, while hovered,
/// about the cursor), the double-click fit and the drag orbit. A drag
/// orbits also while a pick is armed: a face behind the body turns into
/// view, and the pick waits for a click (egui does not report a drag as a
/// click).
pub(super) fn gesture(ui: &Ui, resp: &Response, view: &View) -> Gesture {
    let mut g = Gesture::default();
    if resp.hovered() {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let by = scroll * 0.004 + pinch.max(1e-3).log2();
        if by.abs() > 1e-6 {
            g.zoom = Some((by, resp.hover_pos().map_or([0.0; 2], |p| view.offset(p))));
        }
    }
    g.fit = resp.double_clicked();
    if resp.dragged() {
        g.turn = Some(resp.drag_delta());
    }
    g
}

/// Apply `g` to `orbit`.
pub(super) fn apply(orbit: &mut Orbit, g: &Gesture) {
    if let Some((by, about)) = g.zoom {
        orbit.zoom_about(by, about);
    }
    if g.fit {
        *orbit = Orbit::default();
    }
    if let Some(d) = g.turn {
        orbit.turn(d.x, d.y);
    }
}

fn centroid(t: &Triangle) -> [f64; 3] {
    [(t.a.x + t.b.x + t.c.x) / 3.0, (t.a.y + t.b.y + t.c.y) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub(super) fn norm(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 1e-12).then(|| [a[0] / n, a[1] / n, a[2] / n])
}
