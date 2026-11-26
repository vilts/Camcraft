//! Lead tangents at a closed ring's seam (#3019): a lead that crosses its
//! own ring is turned to the air side, and every other lead keeps its
//! tangent. Specced from plane geometry (a mirror across a unit vector)
//! and the verifier's crossing law (`arc_cross`, O'Rourke's orientation
//! predicate); no vendor material consulted.

use crate::cl::PathBuilder;
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::arc_cross::moves_cross_prior;
use super::arcs::{Seg, lead_in, lead_out, push_segs, walk_tangent};
use super::contour::{ContourParams, Lead, air_side};
use super::ring::dist;

/// A walk end this close to its start (mm) closes a ring at a seam.
const SEAM_CLOSED: f64 = 1e-6;

/// `t` mirrored across the unit direction `axis`.
fn mirror(t: Point2, axis: Point2) -> Point2 {
    let k = 2.0 * (t.x * axis.x + t.y * axis.y);
    Point2::new(k * axis.x - t.x, k * axis.y - t.y)
}

/// Do the lead pieces `segs`, walked from `from`, cross a feed of `ring`
/// under the verifier's law (one plane, shared ends are a touch)?
fn crosses(from: Point2, segs: &[Seg], ring: &[crate::cl::Move]) -> bool {
    if segs.is_empty() {
        return false;
    }
    let mut b = PathBuilder::new(Point3::new(from.x, from.y, 0.0));
    push_segs(&mut b, segs, 0.0, 1.0);
    moves_cross_prior(&b.finish().moves, ring)
}

/// The lead-in and lead-out tangents of `walk`. Each is the walk's own
/// end tangent, except at the seam of a closed ring where the full lead
/// along that tangent crosses the ring's own feed moves. That occurs where
/// the seam falls on a fitted corner arc whose end tangent turns a hair
/// toward the material: a straight lead then cuts back over the ring next
/// to the seam, and verify holds the post. Only there the tangent is
/// mirrored across the other end's tangent, and only if the mirrored lead
/// does not cross. A lead that does not cross keeps its tangent, so the
/// seam, the leads and the entry of every other ring do not change.
pub(super) fn seam_tangents(walk: &[Seg], poly: &[Point2], p: &ContourParams) -> (Point2, Point2) {
    let t0 = walk_tangent(walk, poly, true);
    let t1 = walk_tangent(walk, poly, false);
    let (Some(&s), Some(&e)) = (poly.first(), poly.last()) else {
        return (t0, t1);
    };
    if poly.len() < 3 || dist(s, e) > SEAM_CLOSED {
        return (t0, t1);
    }
    let mut b = PathBuilder::new(Point3::new(s.x, s.y, 0.0));
    push_segs(&mut b, walk, 0.0, 1.0);
    let ring = b.finish().moves;
    let out_shape = p.lead_out_shape();
    let out_crosses = |t: Point2| crosses(e, &lead_out(e, t, air_side(t, p.climb), out_shape), &ring);
    let in_crosses = |t: Point2| {
        let (entry, segs) = lead_in(s, t, air_side(t, p.climb), p.lead);
        crosses(entry, &segs, &ring)
    };
    let out = if out_shape != Lead::None && out_crosses(t1) {
        let m = mirror(t1, t0);
        if out_crosses(m) { t1 } else { m }
    } else {
        t1
    };
    let inn = if p.lead != Lead::None && in_crosses(t0) {
        let m = mirror(t0, t1);
        if in_crosses(m) { t0 } else { m }
    } else {
        t0
    };
    (inn, out)
}
