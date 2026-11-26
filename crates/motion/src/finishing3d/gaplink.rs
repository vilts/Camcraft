//! gaplink.rs - the `Straight` gap-link verdict, used by the raster
//! run chain instead of the plain retract/clearance hop.
//!
//! Provenance: a ball swept along a segment is that segment inflated
//! by the ball radius - the Minkowski sum, textbook geometry (the
//! segment-triangle distance test is Ericson, Real-Time Collision
//! Detection). The verdict law below is our own derivation, written
//! from geometry, not from any vendor's code, output forensics, or
//! documentation text.
//!
//! ## The law here
//!
//! - **Certified or not at all.** The straight link answers yes only
//!   when the facets themselves say the move keeps the ball out of
//!   steel - exactly, not sampled. Anything else answers no and the
//!   hop stands, byte for byte as every older file ran.
//! - **The link is decided, not derived.** `Straight` fixes the
//!   line - both run ends, at their own heights - and the exact
//!   sweep verdict is the whole answer: tangent rests pass, a bite
//!   refuses, and a refusal takes the plain hop.

use crate::geometry::{Point3, Triangle};
use crate::stock_sim::dist_seg_tri;

use super::params::TOUCH_EPS;
use super::grid::SupSet;

/// The `Straight` gap link in one question: may the tool link the
/// runs with ONE straight move from where it stands (`from`, tip) to
/// the next run's first point (`to`, tip), at those two points' own
/// heights? Nothing is chosen - both ends are legal centers by the
/// march's own construction - so the exact sweep verdict is the
/// whole answer: ball centers run the straight line offset by `r`,
/// a swept ball is that segment inflated by `r`, and a facet bites
/// when the segment-triangle distance falls short of `r` by more
/// than float's tangent slop. Tangency is the law's own currency:
/// each end rests tangent to its ground at distance exactly `r`, so
/// rims the link grazes sit on the budget's edge and only a real
/// intrusion refuses. A facet wholly outside the swept band in z
/// can never be reached and is dropped with its vertices unread.
pub(super) fn straight_ok(from: Point3, to: Point3, r: f64, sups: &SupSet) -> bool {
    if !(from.x.is_finite()
        && from.y.is_finite()
        && from.z.is_finite()
        && to.x.is_finite()
        && to.y.is_finite()
        && to.z.is_finite()
        && r.is_finite()
        && r > 0.0)
    {
        return false;
    }
    let (lo_x, hi_x) = if from.x <= to.x { (from.x, to.x) } else { (to.x, from.x) };
    let (lo_y, hi_y) = if from.y <= to.y { (from.y, to.y) } else { (to.y, from.y) };
    let (a, b) = (Point3::new(from.x, from.y, from.z + r), Point3::new(to.x, to.y, to.z + r));
    let (z_lo, z_hi) = (a.z.min(b.z) - r, a.z.max(b.z) + r);
    for s in sups {
        if hi_x < s.xmin || lo_x > s.xmax || hi_y < s.ymin || lo_y > s.ymax {
            continue;
        }
        let (z0, z1, z2) = (s.tri[0].z, s.tri[1].z, s.tri[2].z);
        if (z0 < z_lo && z1 < z_lo && z2 < z_lo) || (z0 > z_hi && z1 > z_hi && z2 > z_hi) {
            continue;
        }
        let t = Triangle::new(s.tri[0], s.tri[1], s.tri[2]);
        if dist_seg_tri(a, b, &t) < r - TOUCH_EPS {
            return false;
        }
    }
    true
}
