//! Continuous deburr tool clearance, from convex support planes and
//! Sutherland-Hodgman polygon clipping. The cone/cylinder polyhedra
//! enclose the real tool; support offsets enclose its complete sweep.
//! Only the finite triangular prism of the requested chamfer is excused.
//! Provenance: elementary convex geometry and our own chamfer model.

use crate::geometry::{Point3, Triangle};

use super::avoid::{shank_hits_at, Shank};
use super::deburr_pose::{add, dot, scale, sub, unit, Cross, Lean, Pose};

const SIDES: usize = 32;
const EPS: f64 = 1e-9;
/// Refuse an approach longer than this many tool diameters. The
/// facet proof below remains mandatory even inside this travel bound.
const REACH_DIAMETERS: f64 = 100.0;

#[derive(Clone, Copy)]
struct Plane {
    n: Point3,
    d: f64,
}

/// Exact lowest point of the cone, relative to its tip.
pub(super) fn cone_floor(lean: &Lean, pose: &Pose) -> f64 {
    let h = lean.flank * lean.half.cos();
    let r = lean.flank * lean.half.sin();
    (h * pose.a[2] - r * (1.0 - pose.a[2] * pose.a[2]).max(0.0).sqrt()).min(0.0)
}

/// Clearance endpoint with a finite, bounded axis-line reach.
pub(super) fn above(tip: Point3, a: [f64; 3], z: f64, diameter: f64) -> Option<Point3> {
    if !tip.is_finite() || a.iter().any(|v| !v.is_finite()) || a[2] <= 1e-3 {
        return None;
    }
    let reach = (z - tip.z).max(0.0) / a[2];
    if !reach.is_finite() || reach > REACH_DIAMETERS * diameter {
        return None;
    }
    let q = add(tip, scale(Point3::new(a[0], a[1], a[2]), reach));
    q.is_finite().then_some(q)
}

/// A height that keeps the whole tool above every facet for any
/// upward pose, including intermediate orientations of an air turn.
pub(super) fn air_height(lean: &Lean, shank: Option<&Shank>, facets: &[Triangle]) -> Option<f64> {
    let mut radius = lean.flank * lean.half.sin();
    if let Some(sh) = shank {
        radius = radius.max(sh.shank_r).max(sh.holder_r);
    }
    let sky = facets.iter().flat_map(|f| [f.a.z, f.b.z, f.c.z]).fold(f64::NEG_INFINITY, f64::max);
    let air = sky + radius + 1.0;
    air.is_finite().then_some(air)
}

/// Cut, entry and exit are checked separately with one constant pose.
/// No point sampling is used to decide clearance.
#[allow(clippy::too_many_arguments)]
pub(super) fn clear(
    lean: &Lean,
    pose: &Pose,
    cr: &Cross,
    setback: f64,
    edge: (Point3, Point3),
    tips: (Point3, Point3),
    clearance: f64,
    shank: Option<&Shank>,
    facets: &[Triangle],
) -> bool {
    let Some(air) = air_height(lean, shank, facets) else { return false };
    if clearance <= air {
        return false;
    }
    let diameter = 2.0 * lean.flank * lean.half.sin();
    if shank.is_some_and(|sh| !holder_compatible(tips, pose.a, diameter * 0.5, sh, facets)) {
        return false;
    }
    let radial_z = (1.0 - pose.a[2] * pose.a[2]).max(0.0).sqrt();
    let mut floor = cone_floor(lean, pose);
    if let Some(sh) = shank {
        for (z, r, len) in [(sh.flute_len, sh.shank_r, sh.shank_len), (sh.flute_len + sh.shank_len, sh.holder_r, sh.holder_len)] {
            if [z, r, len].iter().any(|v| !v.is_finite() || *v < 0.0) {
                return false;
            }
            if len > 0.0 {
                floor = floor.min(z * pose.a[2] - r * radial_z);
            }
        }
    }
    let sky = facets.iter().flat_map(|f| [f.a.z, f.b.z, f.c.z]).fold(f64::NEG_INFINITY, f64::max);
    // Every rapid and every axis change then stands wholly above
    // the part, including intermediate positions between runs.
    if !floor.is_finite() || clearance + floor <= sky {
        return false;
    }
    let (Some(start), Some(end)) = (above(tips.0, pose.a, clearance, diameter), above(tips.1, pose.a, clearance, diameter)) else {
        return false;
    };
    [(tips.0, tips.1), (start, tips.0), (tips.1, end)]
        .into_iter()
        .all(|sweep| sweep_clear(lean, pose, cr, setback, edge, sweep, shank, facets, true))
        && [start, end].into_iter().all(|tip| sweep_clear(lean, pose, cr, setback, edge, (tip, tip), shank, facets, false))
}

#[allow(clippy::too_many_arguments)]
fn sweep_clear(
    lean: &Lean,
    pose: &Pose,
    cr: &Cross,
    setback: f64,
    edge: (Point3, Point3),
    sweep: (Point3, Point3),
    shank: Option<&Shank>,
    facets: &[Triangle],
    allow_break: bool,
) -> bool {
    let a = Point3::new(pose.a[0], pose.a[1], pose.a[2]);
    let h = lean.flank * lean.half.cos();
    let r = lean.flank * lean.half.sin();
    // The first side is the exact tangent chamfer plane. The other
    // sides circumscribe the circular cone; they never inscribe it.
    let Some(v) = unit(add(scale(cr.n, -1.0), scale(a, lean.half.sin()))) else {
        return false;
    };
    let side = a.cross(v);
    let cone = planes(sweep, a, v, side, 0.0, h, 0.0, lean.half.tan());
    if !facets.iter().all(|f| {
        intersection(f, &cone)
            .is_some_and(|poly| poly.is_empty() || (allow_break && poly.into_iter().all(|q| in_break(q, cr, setback, edge))))
    }) {
        return false;
    }
    let Some(sh) = shank else { return true };
    // All physical bands matter, including radii no wider than the
    // cutter. The ball-kerf discount is invalid for a tilted cone.
    let bands = [
        (h, r, (sh.flute_len - h).max(0.0), true),
        (sh.flute_len, sh.shank_r, sh.shank_len, false),
        (sh.flute_len + sh.shank_len, sh.holder_r, sh.holder_len, false),
    ];
    bands.into_iter().all(|(z, radius, length, cutting)| {
        if length <= 0.0 || radius <= 0.0 {
            return true;
        }
        if [z, radius, length].iter().any(|v| !v.is_finite()) {
            return false;
        }
        let mut band = planes(sweep, a, v, side, z, z + length, radius, 0.0);
        // Axial/radial supports alone lose their correlation during
        // a sweep. Preserve the exact support toward the chamfer so
        // a tangent cutting flute does not falsely enter the part.
        let n = scale(cr.n, -1.0);
        let na = dot(n, a);
        let support = (z * na).max((z + length) * na) + radius * (dot(n, n) - na * na).max(0.0).sqrt();
        band.push(Plane { n, d: dot(n, sweep.0) + support + dot(n, sub(sweep.1, sweep.0)).max(0.0) });
        facets.iter().all(|f| {
            intersection(f, &band).is_some_and(|poly| {
                // The flute continuation cuts the same requested wedge.
                // True shank/holder bands never receive this exemption.
                poly.is_empty() || (cutting && allow_break && poly.into_iter().all(|q| in_break(q, cr, setback, edge)))
            })
        })
    })
}

/// Retain the verifier's conservative holder-column verdict as an
/// additional filter. Continuous physical sweeps remain mandatory.
fn holder_compatible((p, q): (Point3, Point3), a: [f64; 3], r: f64, sh: &Shank, facets: &[Triangle]) -> bool {
    let boxes: Vec<_> = facets
        .iter()
        .map(|f| {
            [
                f.a.x.min(f.b.x.min(f.c.x)),
                f.a.y.min(f.b.y.min(f.c.y)),
                f.a.z.min(f.b.z.min(f.c.z)),
                f.a.x.max(f.b.x.max(f.c.x)),
                f.a.y.max(f.b.y.max(f.c.y)),
                f.a.z.max(f.b.z.max(f.c.z)),
            ]
        })
        .collect();
    let d = sub(q, p);
    let n = ((dot(d, d).sqrt() / r.max(1e-3)).ceil() as usize).clamp(1, 64);
    let axis = Point3::new(a[0], a[1], a[2]);
    (0..=n).all(|i| {
        let tip = add(p, scale(d, i as f64 / n as f64));
        !shank_hits_at(add(tip, scale(axis, r)), a, r, sh, facets, &boxes)
    })
}

/// An outer polyhedron of a cone or cylinder translated along a line.
/// Moving each support plane by its maximum endpoint projection
/// encloses every intermediate translated solid, not only endpoints.
#[allow(clippy::too_many_arguments)]
fn planes((p, q): (Point3, Point3), a: Point3, v: Point3, side: Point3, lo: f64, hi: f64, radius: f64, slope: f64) -> Vec<Plane> {
    let delta = sub(q, p);
    let mut out = Vec::with_capacity(2 * SIDES + 2);
    let mut push = |n: Point3, offset: f64| {
        out.push(Plane { n, d: dot(n, p) + offset + dot(n, delta).max(0.0) });
    };
    push(scale(a, -1.0), -lo);
    push(a, hi);
    for i in 0..SIDES {
        let theta = std::f64::consts::TAU * i as f64 / SIDES as f64;
        let radial = add(scale(v, theta.cos()), scale(side, theta.sin()));
        push(sub(radial, scale(a, slope)), radius);
        // The cone translated up its axis has a cylindrical upper
        // envelope, not an indefinitely widening cone. This extra
        // support also tightens sweeps in arbitrary travel directions.
        if slope > 0.0 {
            push(radial, radius + hi * slope);
        }
    }
    out
}

/// The finite triangular removal prism, never the whole adjacent
/// facet. An obstacle in front of the edge or past either end is not
/// part of the requested chamfer and receives no exemption.
fn in_break(q: Point3, cr: &Cross, s: f64, (p, end): (Point3, Point3)) -> bool {
    let Some(t) = unit(sub(end, p)) else {
        return false;
    };
    let d = sub(q, p);
    let h = dot(d, cr.n);
    let along = dot(d, t);
    let depth = cr.depth(s);
    let spread = 0.5 * cr.chord(s) / depth;
    q.is_finite()
        && h >= -depth - EPS
        && h <= EPS
        && dot(d, cr.c).abs() <= -h * spread + EPS
        && along >= -EPS
        && along <= dot(sub(end, p), t) + EPS
}

/// Clip a triangle to all halfspaces. None means arithmetic could not
/// prove clearance and is unsafe. The result is convex, so containment
/// of every vertex proves containment of the whole intersection.
fn intersection(f: &Triangle, planes: &[Plane]) -> Option<Vec<Point3>> {
    let mut poly = vec![f.a, f.b, f.c];
    for plane in planes {
        if !plane.n.is_finite() || !plane.d.is_finite() {
            return None;
        }
        if poly.is_empty() {
            break;
        }
        let mut next = Vec::new();
        let mut prev = *poly.last()?;
        let mut dp = dot(plane.n, prev) - plane.d;
        for &cur in &poly {
            let dc = dot(plane.n, cur) - plane.d;
            if !dp.is_finite() || !dc.is_finite() {
                return None;
            }
            if (dp <= 0.0) != (dc <= 0.0) {
                let denom = dp - dc;
                if !denom.is_finite() || denom == 0.0 {
                    return None;
                }
                let q = add(prev, scale(sub(cur, prev), dp / denom));
                if !q.is_finite() {
                    return None;
                }
                next.push(q);
            }
            if dc <= 0.0 {
                next.push(cur);
            }
            prev = cur;
            dp = dc;
        }
        poly = next;
    }
    Some(poly)
}
