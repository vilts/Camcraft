//! External threads on a boss (#985): the door's boss half, the crest law's region and the crest ring's reading of everything that is not the boss.
//!
//! Provenance: ISO 965-1 / DIN 13 (an external thread is drawn at its
//! major diameter and cut 17/24 H = 0.6134 x pitch deep, to the design
//! root) and public handbook thread milling (Machinery's Handbook,
//! "Thread milling"; Smid, "CNC Programming Handbook": the cutter
//! orbits outside the work, entering on a tangential arc from clear
//! air). The three readings below are our own design; the slab clip is
//! Sutherland-Hodgman polygon clipping and the point-to-polygon
//! distance is textbook computational geometry. No vendor material
//! consulted.
//!
//! A tool beside a boss is where the flat footprint region fails: the
//! boss rises from a wider base, so the part's silhouette says nothing
//! about the wall the teeth cut, and reads the base's outline as a
//! wall the tool stands "inside". So an external thread is judged in
//! three readings that never ask the silhouette, and only on a round
//! boss ([`not_round`] refuses a polygonal one by name):
//! - the **crest law** - the tangency verdict against [`crest_region`],
//!   each boss's MINOR circle: the teeth may reach in to the design
//!   root and no further (the boss is drawn at its major diameter, so a
//!   crest inside it by the thread depth is the thread itself, #1071's
//!   lesson read for a boss);
//! - the **root law** - the solid law with the cutter narrowed by the
//!   external depth ([`super::thread_op::root_cutter`]): the body under
//!   the teeth, the neck, the shank and the holder stay out of the
//!   whole part, the boss's own wall and its shoulder included;
//! - the **neighbour law** - [`neighbours`]: the crest ring itself, at
//!   the tool's full radius over its cutting length, against every
//!   facet that is not the threaded boss's own wall, with no thread
//!   allowance. The root law grants the thread depth radially at EVERY
//!   wall; inside a hole there is no other wall, beside a boss there
//!   may be - a rib, a neighbouring boss, a pocket wall.

use crate::cl::{MoveKind, Toolpath};
use crate::drilling::Hole;
use crate::geo2d::{Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::tools::Tool;
use crate::verify::Warning;

use super::material::GOUGE_TOL;
use super::material_source::disk_loop;
use super::types::OpParams;

/// How far past a boss's fitted radius a facet's vertices may stand and
/// still be the boss's own wall, mm: over the recogniser's circle-fit
/// tolerance, far under any neighbouring feature.
const BOSS_SKIN: f64 = 0.05;

/// Sample spacing of the neighbour law along a move, mm. A facet can
/// slip between two samples only by the sagitta of the tool's own
/// circle over half a step - `step^2 / (8 r)`, under 0.01 mm for any
/// thread mill of 1 mm radius or more.
const STEP: f64 = 0.25;

/// Sagitta of the chords an arc is sampled along, mm.
const ARC_SAG: f64 = 0.002;

/// The most samples one path is read at; a longer one fails closed.
const MAX_SAMPLES: usize = 4_000_000;

/// The door's boss half: why `tool` cannot thread the listed bosses, by
/// name, or `None`. `depth` is the thread's radial depth when the pitch
/// is known. `bottom` says how far down the thread runs: `Some(None)`
/// each boss's own foot, `Some(Some(z))` a resolved stated bottom,
/// `None` a stated bottom nobody has resolved yet (the reach is then
/// left to generate, which asks again).
pub(super) fn door(tool: &Tool, bosses: &[Hole], depth: Option<f64>, bottom: Option<Option<f64>>) -> Option<String> {
    let h = depth?;
    // Float dust off the numbers a refusal names (0.6134 x 1.25 is not 0.76675 in binary).
    let mm = |v: f64| (v * 1e6).round() / 1e6;
    if let Some(x) = bosses.iter().find(|x| x.radius <= h) {
        return Some(format!(
            "the {} mm boss at ({}, {}) is thinner than the {} mm thread is deep - an external thread needs a core under its root",
            x.radius * 2.0,
            x.xy.x,
            x.xy.y,
            mm(h)
        ));
    }
    let bottom = bottom?;
    // What stands above the teeth and is wider than their roots meets
    // the boss once the tip is that far below its top: the shank first,
    // the holder after it.
    let root_r = tool.diameter * 0.5 - h;
    let wider = |d: f64, len: f64| len > 0.0 && d * 0.5 > root_r + GOUGE_TOL;
    let (reach, what) = if wider(tool.shank_d, tool.shank_len) {
        (tool.flute_len, format!("its {} mm shank, wider than the tooth roots,", tool.shank_d))
    } else if wider(tool.holder_d, tool.holder_len) {
        (tool.flute_len + tool.shank_len.max(0.0), "its holder".to_owned())
    } else {
        return None;
    };
    bosses.iter().find_map(|x| {
        let long = x.entry_z - bottom.unwrap_or(x.floor_z);
        (long.is_finite() && long > reach + 1e-9).then(|| {
            format!(
                "the thread on the boss at ({}, {}) is {} mm long, but this thread mill reaches only {} mm down beside a boss before {what} meets it - state a higher bottom or mount a longer thread mill",
                x.xy.x,
                x.xy.y,
                mm(long),
                mm(reach)
            )
        })
    })
}

/// The crest law's region for an external thread: each boss's minor
/// circle (`depth` inside the drawn major wall) and nothing else, so
/// the tangency verdict convicts a tooth crest that passes the thread's
/// design root and nothing short of it. The part's silhouette is left
/// out on purpose (the module's law); [`neighbours`] and the root law
/// read every other wall off the mesh.
pub(super) fn crest_region(bosses: &[Hole], depth: f64) -> Result<Region, GenError> {
    let loops = bosses.iter().map(|x| disk_loop(x.xy, x.radius - depth)).collect::<Result<Vec<_>, _>>()?;
    Ok(Region::new(loops)?)
}

/// Why the bosses of `plan` cannot be threaded, by name, when one is
/// not a round wall in the part, else `None` (#985 x #1205). The crest
/// law reads each boss's minor circle and the neighbour law passes the
/// boss's own wall, both on the word that the wall is a circle: a
/// polygonal boss (a hex stud, a square post) stands its flats inside
/// that circle, where the teeth would cut them unjudged. So every wall
/// facet within the drawn (major) radius plus [`crate::verify::WALL_SAG`]
/// of the axis, over the thread's span, must read as a tessellated
/// circle by the solid law's own sagitta test - the internal side's
/// `thread_op::walls_round` - or the thread is refused.
pub(super) fn not_round(facets: &[Triangle], plan: &crate::machining2d::ThreadParams) -> Option<String> {
    plan.holes.iter().find_map(|h| {
        let (lo, hi) = (h.floor_z.min(h.top_z) - plan.tool_diameter, h.floor_z.max(h.top_z));
        let round = crate::verify::hole_is_round(facets, [h.xy.x, h.xy.y], h.radius + crate::verify::WALL_SAG, lo, hi);
        (!round).then(|| {
            format!(
                "the boss at ({:.3}, {:.3}) is not round - external thread milling needs a round boss (its flats would be cut inside the thread's minor circle)",
                h.xy.x, h.xy.y
            )
        })
    })
}

/// The boss each move is threading, by its place in the list; `None`
/// for a rapid and for a run of feeds that orbits no listed boss. Read
/// off the path's own structure, never off nearness: the ride feeds
/// from its station to its station without a rapid, so each run of
/// feeds belongs to the boss its helix arcs are centred on (of two
/// concentric bosses, the one the orbit's radius fits) - a neighbouring
/// boss the tool merely passes is never mistaken for the one it cuts.
pub(super) fn owners(path: &Toolpath, bosses: &[Hole], tool_r: f64) -> Vec<Option<usize>> {
    let orbited = |m: &crate::cl::Move| {
        let arc = m.arc?;
        let fit = |x: &Hole| ((m.to.x - x.xy.x).hypot(m.to.y - x.xy.y) - (x.radius + tool_r)).abs();
        (0..bosses.len())
            .filter(|k| (arc.center.x - bosses[*k].xy.x).abs() <= 1e-6 && (arc.center.y - bosses[*k].xy.y).abs() <= 1e-6)
            .min_by(|a, b| fit(&bosses[*a]).total_cmp(&fit(&bosses[*b])))
    };
    let mut out = vec![None; path.moves.len()];
    let mut i = 0;
    while i < path.moves.len() {
        if path.moves[i].kind == MoveKind::Rapid {
            i += 1;
            continue;
        }
        let end = (i..path.moves.len()).find(|j| path.moves[*j].kind == MoveKind::Rapid).unwrap_or(path.moves.len());
        let own = path.moves[i..end].iter().find_map(orbited);
        out[i..end].iter_mut().for_each(|o| *o = own);
        i = end;
    }
    out
}

/// The positions one move takes the tool tip through, at most `step`
/// apart, both ends included; arcs along their chords at [`ARC_SAG`].
pub(super) fn samples(m: &crate::cl::Move, step: f64) -> Vec<Point3> {
    let mut out = vec![m.from];
    for (a, b) in m.segments3(ARC_SAG) {
        let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt();
        let n = if len.is_finite() && step > 0.0 { ((len / step).ceil() as usize).clamp(1, 1 << 20) } else { 1 };
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push(Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t));
        }
    }
    out
}

/// `t` clipped to the slab `lo <= z <= hi`, projected to XY (empty when
/// the triangle misses the slab).
fn slab_clip(t: &Triangle, lo: f64, hi: f64) -> Vec<Point2> {
    let mut poly = vec![t.a, t.b, t.c];
    for (keep_above, z) in [(true, lo), (false, hi)] {
        let inside = |p: &Point3| if keep_above { p.z >= z } else { p.z <= z };
        let mut next = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            if inside(&p) {
                next.push(p);
            }
            if inside(&p) != inside(&q) && (q.z - p.z).abs() > f64::EPSILON {
                let u = (z - p.z) / (q.z - p.z);
                next.push(Point3::new(p.x + (q.x - p.x) * u, p.y + (q.y - p.y) * u, z));
            }
        }
        poly = next;
    }
    poly.into_iter().map(|p| Point2::new(p.x, p.y)).collect()
}

/// Squared XY distance from `c` to the convex polygon `poly`, zero
/// inside it.
fn dist2_poly(c: Point2, poly: &[Point2]) -> f64 {
    let n = poly.len();
    let (mut best, mut pos, mut neg) = (f64::INFINITY, false, false);
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let cross = dx * (c.y - a.y) - dy * (c.x - a.x);
        pos |= cross > 0.0;
        neg |= cross < 0.0;
        let dd = dx * dx + dy * dy;
        let u = if dd > f64::EPSILON { (((c.x - a.x) * dx + (c.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
        let (ex, ey) = (a.x + u * dx - c.x, a.y + u * dy - c.y);
        best = best.min(ex * ex + ey * ey);
    }
    // Strictly one side of every edge: inside a polygon with area.
    if n >= 3 && pos != neg { 0.0 } else { best }
}

/// A facet near the path, with its bounds and the bosses whose own wall
/// it is.
struct Near {
    tri: Triangle,
    lo: Point3,
    hi: Point3,
    of_boss: Vec<bool>,
}

/// The neighbour law (#985): one warning per move of an external Thread
/// operation whose crest ring - the tool's full radius over its cutting
/// length - enters a facet that is not the threaded boss's own wall.
/// A feed is a [`Warning::Gouge`], a rapid a [`Warning::RapidInStock`];
/// a rapid excuses no boss at all. Empty for every other operation. A
/// tool or path that cannot be read fails closed: a gouge at the first
/// cutting move, never a pass.
pub(super) fn neighbours(params: &OpParams, path: &Toolpath, facets: &[Triangle], tool: &Tool) -> Vec<Warning> {
    let OpParams::Thread { external: true, holes: bosses, .. } = params else {
        return Vec::new();
    };
    let (r, len) = (tool.diameter * 0.5, tool.flute_len);
    let closed = || path.moves.iter().position(|m| m.kind != MoveKind::Rapid).map(|at| vec![Warning::Gouge { at }]).unwrap_or_default();
    if !(r.is_finite() && r > GOUGE_TOL && len.is_finite() && len > 0.0 && path.all_finite()) {
        return closed();
    }
    // Only the facets the path's own box can reach.
    let (mut lo, mut hi) = (Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY), Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY));
    for p in path.moves.iter().flat_map(|m| [m.from, m.to]) {
        lo = Point3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Point3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    // An arc stands within its own diameter of its end.
    let bulge = path.moves.iter().filter_map(|m| m.arc.map(|a| (m.to.x - a.center.x).hypot(m.to.y - a.center.y))).fold(0.0, f64::max);
    let pad = r + 2.0 * bulge;
    let near: Vec<Near> = facets
        .iter()
        .filter(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite())
        .filter_map(|t| {
            let tl = Point3::new(t.a.x.min(t.b.x).min(t.c.x), t.a.y.min(t.b.y).min(t.c.y), t.a.z.min(t.b.z).min(t.c.z));
            let th = Point3::new(t.a.x.max(t.b.x).max(t.c.x), t.a.y.max(t.b.y).max(t.c.y), t.a.z.max(t.b.z).max(t.c.z));
            let reached = th.x >= lo.x - pad && tl.x <= hi.x + pad && th.y >= lo.y - pad && tl.y <= hi.y + pad && th.z >= lo.z && tl.z <= hi.z + len;
            reached.then(|| {
                let within = |x: &Hole| [t.a, t.b, t.c].iter().all(|p| (p.x - x.xy.x).hypot(p.y - x.xy.y) <= x.radius + BOSS_SKIN);
                Near { tri: *t, lo: tl, hi: th, of_boss: bosses.iter().map(within).collect() }
            })
        })
        .collect();
    let floor2 = (r - GOUGE_TOL) * (r - GOUGE_TOL);
    let owned = owners(path, bosses, r);
    let mut out = Vec::new();
    let mut budget = MAX_SAMPLES;
    for (i, m) in path.moves.iter().enumerate() {
        let rapid = m.kind == MoveKind::Rapid;
        let pts = samples(m, STEP);
        if pts.len() > budget {
            return closed();
        }
        budget -= pts.len();
        let hit = pts.iter().any(|p| {
            let c = Point2::new(p.x, p.y);
            // The boss this feed is cutting keeps its own wall: the crest law reads it.
            let own = owned.get(i).copied().flatten();
            // The slab starts a hair above the tip: a shoulder the tip stands ON is the root law's.
            let (z0, z1) = (p.z + GOUGE_TOL, p.z + len);
            near.iter().any(|f| {
                if own.is_some_and(|k| f.of_boss.get(k).copied().unwrap_or(false)) {
                    return false;
                }
                if f.hi.z < z0 || f.lo.z > z1 || f.hi.x < c.x - r || f.lo.x > c.x + r || f.hi.y < c.y - r || f.lo.y > c.y + r {
                    return false;
                }
                let poly = slab_clip(&f.tri, z0, z1);
                !poly.is_empty() && dist2_poly(c, &poly) < floor2
            })
        });
        if hit {
            out.push(if rapid { Warning::RapidInStock { at: i } } else { Warning::Gouge { at: i } });
        }
    }
    out
}

#[cfg(test)]
#[path = "thread_boss_tests.rs"]
mod tests;
