//! The Boundary group in the viewport (M11.5 UX, the user's hand-test:
//! "the visualization for edge and boundary selection is very vague"):
//! while the group is open or collecting, every entry's picked edges draw
//! thick in their role's colour (the colour-vision palette's
//! `bound_*`), the edge a click would take draws as a hover highlight,
//! an open chain's two ends carry a square, an open boundary carries
//! arrows pointing into the side that will be machined - read off the
//! draft's own machined-area preview, so an arrow can never promise a
//! side the region law does not cut - and that preview area is filled
//! translucent (`preview_fill`, drawn with the region fills) - the
//! area the tool really sweeps, so Face's fill is its bounding band.
//! Open-boundary edges draw dashed, closed loops solid.
//! Screen-space marks (egui strokes, so they can be thick) projected
//! through the same camera math as the hover outline.
//!
//! Provenance: our own display design; no vendor material consulted.

use std::sync::Arc;

use camcraft_app::geo2d::{Point2, Region};
use camcraft_app::operation::{self, PickShape, area_applies};

use crate::boundary_draft::{self, Role};
use crate::egui_app::colorvision::palette_of;
use crate::egui_app::ground_ink::{Overlay, overlay};
use crate::interact::{feature_segments, project_uv};
use crate::state::State;

/// What a mark says, for the tests and the draw order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// A picked boundary edge, in its role's colour.
    Edge(Role),
    /// The edge (or face) the next click would take.
    Hover,
    /// The square at an open chain's end.
    Endpoint,
    /// An arrow into the machined side of an open boundary.
    Side,
}

/// One screen-space stroke, viewport uv (0..1) like `hover_features`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub rgb: [f32; 3],
    pub width: f32,
    pub kind: MarkKind,
}

/// The hover ink, the hover outline's own yellow.
pub(crate) const HOVER_RGB: [f32; 3] = [1.0, 0.85, 0.40];
/// Arrows sampled along an open chain, at most.
const MAX_ARROWS: usize = 12;

/// Whether the group's marks draw this frame: an open operation whose
/// kind has an area, with the group open or collecting.
#[must_use]
pub fn active(state: &State) -> bool {
    !state.op_ui.name.is_empty()
        && area_applies(state.op_ui.params.kind())
        && (state.boundary_view.shown || state.boundary_pick.is_some())
}

fn role_rgb(state: &State, role: Role) -> [f32; 3] {
    let p = palette_of(&state.world);
    match role {
        Role::Include => p.bound_include,
        Role::Exclude => p.bound_exclude,
        Role::OpenA | Role::OpenB => p.bound_open,
    }
}

/// The plane the marks of open chains stand at: the preview's, else the
/// draft's own region plane.
fn mark_z(state: &State) -> Option<f64> {
    if let Some(p) = &state.boundary_view.preview {
        return Some(p.z);
    }
    let src = camcraft_app::model::find_by_name(&state.world, &state.op_ui.name)
        .and_then(|e| camcraft_app::model::operation_of(&state.world, &e))
        .map(|o| (o.setup, o.src))?;
    // A per-frame display read: the sketch Z span may come from the frame memo.
    operation::display_heights(|| {
        operation::region_z(&state.world, src.0.as_deref(), Some(&src.1), &state.op_ui.params)
    })
}

/// Every mark of the group this frame (empty when not [`active`]).
#[must_use]
pub fn boundary_marks(state: &State, aspect: f64) -> Vec<Mark> {
    let mut out = Vec::new();
    if !active(state) {
        return out;
    }
    let mvp = state.cam.mvp(aspect);
    let z = mark_z(state);
    let push = |out: &mut Vec<Mark>, a: [f64; 3], b: [f64; 3], rgb, width, kind| {
        if let (Some(a), Some(b)) = (project_uv(&mvp, a), project_uv(&mvp, b))
            && a != b
        {
            out.push(Mark { a, b, rgb, width, kind });
        }
    };
    for e in boundary_draft::entries(state) {
        let rgb = role_rgb(state, e.role);
        let target = state.boundary_pick == Some(boundary_draft::target_of(e.at));
        let width = if target { 4.0 } else { 3.0 };
        for g in &e.picks {
            let mut segs = Vec::new();
            feature_segments(state, &g.entity, g.feature.clone(), aspect, &mut segs);
            // An open boundary's edges draw dashed - told apart from a
            // closed loop by shape, not by colour alone.
            for (a, b) in segs {
                if e.role.is_open() {
                    out.extend(dashes(a, b).into_iter().map(|(a, b)| Mark { a, b, rgb, width, kind: MarkKind::Edge(e.role) }));
                } else {
                    out.push(Mark { a, b, rgb, width, kind: MarkKind::Edge(e.role) });
                }
            }
        }
        let (Some(PickShape::Open { chain, .. }), Some(z)) = (&e.shape, z) else {
            continue;
        };
        let len = chain_len(chain);
        let s = (len * 0.03).clamp(0.6, 4.0);
        for p in [chain.first(), chain.last()].into_iter().flatten() {
            let c = [[-s, -s], [s, -s], [s, s], [-s, s]];
            for k in 0..4 {
                let (u, v) = (c[k], c[(k + 1) % 4]);
                push(&mut out, [p.x + u[0], p.y + u[1], z], [p.x + v[0], p.y + v[1], z], rgb, 2.0, MarkKind::Endpoint);
            }
        }
        if e.role.is_open()
            && let Some(region) = state.boundary_view.preview.as_ref().and_then(|p| p.region.as_ref())
        {
            for (m, dir) in side_arrows(chain, region) {
                let l = (len * 0.06).clamp(1.5, 12.0);
                let tip = [m.x + dir.x * l, m.y + dir.y * l];
                push(&mut out, [m.x, m.y, z], [tip[0], tip[1], z], rgb, 2.0, MarkKind::Side);
                for turn in [2.6_f64, -2.6] {
                    let (c, si) = (turn.cos(), turn.sin());
                    let h = [dir.x * c - dir.y * si, dir.x * si + dir.y * c];
                    let back = [tip[0] + h[0] * l * 0.35, tip[1] + h[1] * l * 0.35];
                    push(&mut out, [tip[0], tip[1], z], [back[0], back[1], z], rgb, 2.0, MarkKind::Side);
                }
            }
        }
    }
    if state.boundary_pick.is_some()
        && let Some(h) = &state.hover
        && let Some(ref f) = h.feature
    {
        let mut segs = Vec::new();
        feature_segments(state, &h.entity, f.clone(), aspect, &mut segs);
        out.extend(segs.into_iter().map(|(a, b)| Mark { a, b, rgb: overlay(Overlay::Hover), width: 2.5, kind: MarkKind::Hover }));
    }
    out
}

/// Dash length and gap, viewport uv.
const DASH: f32 = 0.014;
const GAP: f32 = 0.008;
/// Dashes per segment, at most (a huge segment stays a bounded draw).
const MAX_DASHES: usize = 400;

/// One screen segment as dashes (the segment itself when it is shorter
/// than a dash).
fn dashes(a: [f32; 2], b: [f32; 2]) -> Vec<([f32; 2], [f32; 2])> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = dx.hypot(dy);
    if !(l.is_finite() && l > DASH) {
        return vec![(a, b)];
    }
    let step = DASH + GAP;
    let n = ((l / step).ceil() as usize).clamp(1, MAX_DASHES);
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let t0 = (k as f32 * step / l).min(1.0);
        let t1 = ((k as f32 * step + DASH) / l).min(1.0);
        if t1 > t0 {
            out.push(([a[0] + dx * t0, a[1] + dy * t0], [a[0] + dx * t1, a[1] + dy * t1]));
        }
    }
    out
}

fn chain_len(chain: &[Point2]) -> f64 {
    chain.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum()
}

/// Arrow feet and unit directions into the machined side: at the
/// midpoints of (up to [`MAX_ARROWS`] evenly spread) chain segments, the
/// side where the preview region stands just off the chain and the other
/// side does not. A segment the region borders on both sides (or
/// neither) gets no arrow - never a guess.
#[must_use]
pub fn side_arrows(chain: &[Point2], region: &Region) -> Vec<(Point2, Point2)> {
    let segs: Vec<(Point2, Point2)> = chain.windows(2).map(|w| (w[0], w[1])).collect();
    let n = segs.len();
    if n == 0 {
        return Vec::new();
    }
    let step = n.div_ceil(MAX_ARROWS).max(1);
    let mut out = Vec::new();
    for (a, b) in segs.into_iter().step_by(step) {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l = dx.hypot(dy);
        if !(l.is_finite() && l > 1e-9) {
            continue;
        }
        let nrm = Point2::new(-dy / l, dx / l);
        let m = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        // Probe close first, then further out: an offset or a tool
        // containment may hold the area a little off the chain.
        for d in [(l * 0.05).clamp(0.05, 1.0), 3.0, 8.0] {
            let left = region.contains(Point2::new(m.x + nrm.x * d, m.y + nrm.y * d));
            let right = region.contains(Point2::new(m.x - nrm.x * d, m.y - nrm.y * d));
            match (left, right) {
                (true, false) => out.push((m, nrm)),
                (false, true) => out.push((m, Point2::new(-nrm.x, -nrm.y))),
                (true, true) => {}
                (false, false) => continue,
            }
            break;
        }
    }
    out
}

/// The preview fill's triangles, shared so a clone of the preview stays
/// cheap.
pub type Fill = Arc<[[Point2; 3]]>;

/// #2246: triangulate a preview region once, when the preview is made
/// (`boundary_draft::refresh_preview`, keyed on the draft and the
/// document version); `None` when it refuses or is empty, as the scene
/// skipped those before. The span counts each build (the probe's pin: a
/// still frame makes none).
#[must_use]
pub fn fill_of(region: &Region) -> Option<Fill> {
    let _s = crate::frame_cost::span("ui/properties.boundary.preview.triangulate");
    region.triangles().ok().filter(|t| !t.is_empty()).map(Arc::from)
}

/// The draft's machined-area preview to fill: (plane, triangles), only
/// while the group is [`active`] and the preview resolved. The triangles
/// were made with the preview; a frame only reads them.
#[must_use]
pub fn preview_fill(state: &State) -> Option<(f64, &[[Point2; 3]])> {
    if !active(state) {
        return None;
    }
    let p = state.boundary_view.preview.as_ref()?;
    Some((p.z, p.fill.as_deref()?))
}
