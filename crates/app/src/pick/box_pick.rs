//! Box selection under the one pick rule (#979, #692): every feature edge
//! chain - or every welded face - whose projection falls in a screen
//! rectangle, among the visible ones only. Window mode takes what lies
//! wholly inside the box; crossing mode also takes what the box touches.
//! Tessellation edges are never candidates (only feature chains are), and
//! a candidate counts only if some part of it is unoccluded from the eye,
//! by the same sight test the click pick runs - unless pick-through is on.
//!
//! Provenance: window/crossing rubber-band selection is the long-standing
//! CAD convention (drag right = window, drag left = crossing, as public
//! CAD user documentation describes it); the segment/rectangle clip is
//! Liang-Barsky (Liang & Barsky, ACM TOG 1984). Our own code over our own
//! pick API; no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::model;
use crate::operation::{FeatureSel, GeoSel};

use super::feature_graph::FeatureGraph;
use super::feature_pick::{PickBody, PickKinds, PickRay, Projection, occluded, pick_bodies_of, world_of};
use super::feature_sel::is_feature_chain;

/// Which side of the box rule a drag asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoxMode {
    /// Wholly inside the box (a drag to the right).
    #[default]
    Window,
    /// Inside or crossing the box edge (a drag to the left).
    Crossing,
}

impl BoxMode {
    /// The CAD convention: a drag from left to right is a window, from
    /// right to left a crossing box.
    #[must_use]
    pub fn of_drag(from_x: f64, to_x: f64) -> BoxMode {
        if to_x < from_x { BoxMode::Crossing } else { BoxMode::Window }
    }
}

/// One box selection request. `project` maps a world point to the 2D
/// frame the box is drawn in (screen px in the viewport, world XY for a
/// top-down MCP box); `None` means it does not draw (behind the eye).
pub struct BoxQuery<'a> {
    pub project: &'a dyn Fn([f64; 3]) -> Option<[f64; 2]>,
    /// Two opposite corners of the box, any order.
    pub corners: [[f64; 2]; 2],
    pub mode: BoxMode,
    /// Only `edges` (feature chains) and `faces` (welded faces) are read.
    pub kinds: PickKinds,
    /// The sight line used for occlusion: perspective `origin` is the
    /// eye; orthographic `dir` is the view direction.
    pub view: PickRay,
    /// Ignore occlusion (an explicit pick-through mode only).
    pub through: bool,
}

/// At most this many sight rays per candidate, spread along it: a long
/// chain mostly hidden still costs a bounded number of rays.
const SIGHT_SAMPLES: usize = 16;

struct Rect {
    lo: [f64; 2],
    hi: [f64; 2],
}

impl Rect {
    fn of(c: [[f64; 2]; 2]) -> Option<Rect> {
        let ok = c.iter().flatten().all(|v| v.is_finite());
        let lo = [c[0][0].min(c[1][0]), c[0][1].min(c[1][1])];
        let hi = [c[0][0].max(c[1][0]), c[0][1].max(c[1][1])];
        (ok && hi[0] > lo[0] && hi[1] > lo[1]).then_some(Rect { lo, hi })
    }

    fn holds(&self, p: [f64; 2]) -> bool {
        p[0] >= self.lo[0] && p[0] <= self.hi[0] && p[1] >= self.lo[1] && p[1] <= self.hi[1]
    }

    /// Liang-Barsky: the parameter span of `a`-`b` inside the box, if any.
    fn clip(&self, a: [f64; 2], b: [f64; 2]) -> Option<(f64, f64)> {
        let d = [b[0] - a[0], b[1] - a[1]];
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for (p, q) in [
            (-d[0], a[0] - self.lo[0]),
            (d[0], self.hi[0] - a[0]),
            (-d[1], a[1] - self.lo[1]),
            (d[1], self.hi[1] - a[1]),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    return None;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
            }
        }
        (t0 <= t1 && t0.is_finite() && t1.is_finite()).then_some((t0, t1))
    }
}

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Up to [`SIGHT_SAMPLES`] of `pts`, evenly spread.
fn spread(pts: &[[f64; 3]]) -> impl Iterator<Item = [f64; 3]> + '_ {
    let step = pts.len().div_ceil(SIGHT_SAMPLES).max(1);
    pts.iter().step_by(step).copied()
}

struct Ctx<'a> {
    bodies: &'a [PickBody],
    q: &'a BoxQuery<'a>,
    rect: Rect,
    dir: [f64; 3],
}

impl Ctx<'_> {
    fn seen(&self, own: usize, samples: &[[f64; 3]]) -> bool {
        !samples.is_empty()
            && (self.q.through || spread(samples).any(|p| !occluded(self.bodies, own, &self.q.view, self.dir, p)))
    }

    /// A feature chain's verdict: the sample points it offers the sight
    /// test (world), or `None` when the box rule leaves it out.
    fn chain_samples(&self, b: &PickBody, g: &FeatureGraph, edges: &[u32]) -> Option<Vec<[f64; 3]>> {
        let mut samples = Vec::new();
        for &ei in edges {
            let (a, z) = g.edge_ends(ei)?;
            let (wa, wz) = (world_of(b, a), world_of(b, z));
            let (pa, pz) = ((self.q.project)(wa), (self.q.project)(wz));
            match self.q.mode {
                BoxMode::Window => {
                    let (pa, pz) = (pa?, pz?);
                    if !(self.rect.holds(pa) && self.rect.holds(pz)) {
                        return None;
                    }
                    samples.push(lerp(wa, wz, 0.5));
                }
                BoxMode::Crossing => {
                    if let (Some(pa), Some(pz)) = (pa, pz)
                        && let Some((t0, t1)) = self.rect.clip(pa, pz)
                    {
                        samples.push(lerp(wa, wz, 0.5 * (t0 + t1)));
                    }
                }
            }
        }
        (!samples.is_empty()).then_some(samples)
    }

    /// A welded face's sight samples (facet centroids in the box), or
    /// `None` when the box rule leaves it out.
    fn face_samples(&self, b: &PickBody, facets: &[u32]) -> Option<Vec<[f64; 3]>> {
        let mut samples = Vec::new();
        for &fi in facets {
            let t = b.tris.get(fi as usize)?;
            let w = [t.a, t.b, t.c].map(|p| world_of(b, [p.x, p.y, p.z]));
            let px = w.map(|p| (self.q.project)(p));
            let c = [
                (w[0][0] + w[1][0] + w[2][0]) / 3.0,
                (w[0][1] + w[1][1] + w[2][1]) / 3.0,
                (w[0][2] + w[1][2] + w[2][2]) / 3.0,
            ];
            match self.q.mode {
                BoxMode::Window => {
                    if !px.iter().all(|p| p.is_some_and(|p| self.rect.holds(p))) {
                        return None;
                    }
                    samples.push(c);
                }
                BoxMode::Crossing => {
                    let [Some(a), Some(bb), Some(cc)] = px else { continue };
                    if self.touches(a, bb, cc) {
                        samples.push(c);
                    }
                }
            }
        }
        (!samples.is_empty()).then_some(samples)
    }

    /// Does the projected triangle meet the box?
    fn touches(&self, a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
        if [a, b, c].iter().any(|p| self.rect.holds(*p)) || [(a, b), (b, c), (c, a)].iter().any(|(p, q)| self.rect.clip(*p, *q).is_some()) {
            return true;
        }
        // The box wholly inside the triangle: its centre is.
        let m = [(self.rect.lo[0] + self.rect.hi[0]) * 0.5, (self.rect.lo[1] + self.rect.hi[1]) * 0.5];
        let side = |p: [f64; 2], q: [f64; 2]| (q[0] - p[0]) * (m[1] - p[1]) - (q[1] - p[1]) * (m[0] - p[0]);
        let s = [side(a, b), side(b, c), side(c, a)];
        s.iter().all(|v| *v >= 0.0) || s.iter().all(|v| *v <= 0.0)
    }
}

/// The box rule over explicit bodies: the selections the box takes, in
/// body order, each once. `take` says which bodies offer candidates (a
/// box picks part features, so the caller leaves out stock and fixtures);
/// every body still occludes. Degenerate input (an empty or non-finite
/// box, a zero view direction) takes nothing - never a panic.
#[must_use]
pub fn box_bodies(bodies: &[PickBody], q: &BoxQuery<'_>, take: impl Fn(&PickBody) -> bool) -> Vec<GeoSel> {
    let Some(rect) = Rect::of(q.corners) else { return Vec::new() };
    let d0 = q.view.dir;
    let len = (d0[0] * d0[0] + d0[1] * d0[1] + d0[2] * d0[2]).sqrt();
    if !(len.is_finite() && len > 1e-12 && q.view.origin.iter().all(|v| v.is_finite())) {
        return Vec::new();
    }
    let ctx = Ctx { bodies, q, rect, dir: [d0[0] / len, d0[1] / len, d0[2] / len] };
    let mut out: Vec<GeoSel> = Vec::new();
    let mut push = |entity: &str, feature: FeatureSel| {
        if !out.iter().any(|g| g.entity == entity && g.feature == feature) {
            out.push(GeoSel { entity: entity.to_string(), feature });
        }
    };
    for (bi, b) in bodies.iter().enumerate() {
        if !take(b) {
            continue;
        }
        let g = &b.graph;
        if q.kinds.edges {
            for c in g.chains().iter().filter(|c| is_feature_chain(g, c)) {
                if ctx.chain_samples(b, g, &c.edges).is_some_and(|s| ctx.seen(bi, &s)) {
                    push(&b.name, FeatureSel::Chain(c.id));
                }
            }
        }
        if q.kinds.faces {
            for f in &g.faces {
                let Some(sel) = f.sel else { continue };
                if ctx.face_samples(b, &f.facets).is_some_and(|s| ctx.seen(bi, &s)) {
                    push(&b.name, sel);
                }
            }
        }
    }
    out
}

/// The box rule over the world's pickable bodies ([`pick_bodies_of`]:
/// what the viewport draws, less what `skip` hides). Candidates are the
/// user's parts only - never stock, never a fixture - though both still
/// hide what lies behind them.
#[must_use]
pub fn box_select(world: &World, q: &BoxQuery<'_>, skip: impl Fn(&str) -> bool) -> Vec<GeoSel> {
    let bodies = pick_bodies_of(world, skip);
    box_bodies(&bodies, q, |b| part_body(world, b))
}

fn part_body(world: &World, b: &PickBody) -> bool {
    !b.stock
        && model::find_by_name(world, &b.name).is_some_and(|e| model::is_model(world, &e) && !model::is_fixture(world, &e))
}

/// The box rule seen from straight above (+Z, orthographic): `corners`
/// are world X/Y, mm. The headless twin of the viewport box (MCP
/// `pick_in_box`), with the same visibility law - an edge under the top
/// face is hidden from above, as it is in the Top view - unless `through`
/// (pick-through, as the viewport's toggle) takes the hidden ones too.
#[must_use]
pub fn box_select_top(world: &World, corners: [[f64; 2]; 2], mode: BoxMode, kinds: PickKinds, through: bool) -> Vec<GeoSel> {
    let bodies = pick_bodies_of(world, |_| false);
    // The sight lines start just above everything drawn: a far eye would
    // stretch the occlusion slack, which grows with the sight length.
    let top = bodies
        .iter()
        .flat_map(|b| b.tris.iter().flat_map(move |t| [t.a, t.b, t.c].map(|p| world_of(b, [p.x, p.y, p.z])[2])))
        .filter(|z| z.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    if !top.is_finite() {
        return Vec::new();
    }
    let project = |p: [f64; 3]| Some([p[0], p[1]]);
    let view = PickRay { origin: [0.0, 0.0, top + 10.0], dir: [0.0, 0.0, -1.0], projection: Projection::Ortho };
    let q = BoxQuery { project: &project, corners, mode, kinds, view, through };
    box_bodies(&bodies, &q, |b| part_body(world, b))
}
