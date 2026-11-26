//! Provenance: conservative XY convex-hull bound and segment-distance geometry, using Andrew's published hull algorithm.

use camcraft_geometry::dxf_profile::ExtrudeAxis;
use camcraft_geometry::profile_derive::derive_extruded_from_facets;

use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Triangle;
use std::ops::Range;

use super::Ladder;

const BOUND_LIMIT: f64 = 1e9;

/// [`super::ladder`] of rings the user PICKED (#3443): a ring clear of the
/// model's XY hull keeps every level without a section. Whole-model
/// rings never take this: their unread planes must still refuse.
#[allow(clippy::too_many_arguments)]
pub(in crate::machining2d) fn ladder_picked(
    rings: &[Vec<Point2>],
    facets: &[Triangle],
    bodies: &[Range<usize>],
    zs: &[f64],
    top: f64,
    r: f64,
    leave: f64,
    notes: &mut Vec<String>,
) -> Ladder {
    let hull = bound(facets);
    super::ladder_in(rings, facets, bodies, zs, top, r, leave, notes, hull)
}

/// The XY hull contains every triangle at every height. An unavailable
/// bound leaves the existing per-plane checks in charge.
pub(super) fn bound(facets: &[Triangle]) -> Option<Region> {
    // Keep this optional broad-phase proof within a bounded numeric range.
    // Larger models retain the ordinary section validation and ladder.
    if facets.iter().flat_map(|t| [t.a, t.b, t.c]).any(|p| {
        !p.is_finite()
            || p.x.abs() >= BOUND_LIMIT
            || p.y.abs() >= BOUND_LIMIT
            || p.z.abs() >= BOUND_LIMIT
    }) {
        return None;
    }
    let (profile, _) = derive_extruded_from_facets(facets, ExtrudeAxis::Z).ok()?;
    let points = profile
        .points
        .iter()
        .map(|p| Point2::new(p.u, p.v))
        .collect();
    Region::new(vec![Loop::new(points).ok()?]).ok()
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

fn segment_clear(p: Point2, q: Point2, a: Point2, b: Point2, need2: f64) -> bool {
    let opposite = |u: f64, v: f64| (u > 0.0 && v < 0.0) || (u < 0.0 && v > 0.0);
    if opposite(cross(a, b, p), cross(a, b, q)) && opposite(cross(p, q, a), cross(p, q, b)) {
        return false;
    }
    // Disjoint segments attain their minimum distance at an endpoint.
    super::dist2_seg(p, a, b)
        .min(super::dist2_seg(q, a, b))
        .min(super::dist2_seg(a, p, q))
        .min(super::dist2_seg(b, p, q))
        >= need2
}

/// Prove the entire closed cutter-center polyline clear, including the
/// space between samples. A concave wall or a shoulder inside the hull
/// fails this sufficient proof and keeps the normal depth ladder.
pub(super) fn clear(hull: &Region, ring: &[Point2], need: f64) -> bool {
    // Cover the hull API's 1e-6 mm point merge and polygon quantization.
    // This proof is slightly stricter than the ordinary ladder's slack.
    let need2 = (need.max(0.0) + 1e-5).powi(2);
    ring.len() >= 3
        && ring.iter().all(|p| {
            p.x.is_finite() && p.y.is_finite() && p.x.abs() < BOUND_LIMIT && p.y.abs() < BOUND_LIMIT
        })
        && super::clear_of(hull, ring, need2)
        && (0..ring.len()).all(|i| {
            let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
            hull.loops().iter().all(|l| {
                let pts = l.points();
                (0..pts.len()).all(|j| segment_clear(p, q, pts[j], pts[(j + 1) % pts.len()], need2))
            })
        })
}
