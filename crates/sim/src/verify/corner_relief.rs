//! Corner relief verification grant (#2400): bounds every declared
//! relief to the cutter radius, cut depth above pocket floor, and stock
//! boundary so verification never weakens real gouge checks.
//!
//! Provenance: clean-room verification guard over [`super::solid`] and
//! [`super::section`] in this codebase; no vendor source consulted.

use crate::geo2d::{Point2, Region};
use crate::geometry::{Point3, Triangle};

/// One declared corner-relief corridor on a pocket/slot floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerReliefZone {
    pub corner: Point2,
    pub start: Point2,
    pub end: Point2,
    pub radius: f64,
    pub z_bottom: f64,
    pub z_top: f64,
}

#[inline]
pub(super) fn seg_point_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd <= 1e-18 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0)
    };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

fn seg_seg_dist_2d(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> f64 {
    if super::laws::proper_cross(a0, a1, b0, b1) {
        return 0.0;
    }
    seg_point_dist(a0, b0, b1)
        .min(seg_point_dist(a1, b0, b1))
        .min(seg_point_dist(b0, a0, a1))
        .min(seg_point_dist(b1, a0, a1))
}

/// Validate declared relief zones strictly against cutter radius and stock bounds.
pub(super) fn valid_zones<'a>(
    reliefs: &'a [CornerReliefZone],
    tool_r: f64,
    stock_to_leave: f64,
    tol: f64,
    stock: Option<&Region>,
) -> Vec<&'a CornerReliefZone> {
    let leave = (-stock_to_leave).max(0.0);
    let wall_floor = (tool_r - leave - tol).max(0.0);
    reliefs
        .iter()
        .filter(|rc| {
            let finite = rc.corner.x.is_finite()
                && rc.corner.y.is_finite()
                && rc.start.x.is_finite()
                && rc.start.y.is_finite()
                && rc.end.x.is_finite()
                && rc.end.y.is_finite()
                && rc.radius.is_finite()
                && rc.z_bottom.is_finite()
                && rc.z_top.is_finite();
            if !finite || rc.radius <= 0.0 || rc.z_bottom > rc.z_top {
                return false;
            }
            if rc.radius > tool_r + tol {
                return false;
            }
            if (rc.end.x - rc.corner.x).hypot(rc.end.y - rc.corner.y) < rc.radius - tol {
                return false;
            }
            if let Some(s) = stock {
                if !s.contains(rc.start) || !s.contains(rc.end) {
                    return false;
                }
                for lp in s.loops() {
                    let pts = lp.points();
                    for k in 0..pts.len() {
                        if seg_seg_dist_2d(rc.start, rc.end, pts[k], pts[(k + 1) % pts.len()]) < wall_floor {
                            return false;
                        }
                    }
                }
            }
            true
        })
        .collect()
}

/// Whether a segment stays along the corridor and within both relief heights.
#[inline]
pub(super) fn move_on_zone(
    a: Point2,
    b: Point2,
    z_min: f64,
    z_max: f64,
    r_eff: f64,
    tol: f64,
    rc: &CornerReliefZone,
) -> bool {
    z_min >= rc.z_bottom - tol
        && z_max <= rc.z_top + tol
        && r_eff <= rc.radius + tol
        && seg_point_dist(a, rc.start, rc.end) <= tol
        && seg_point_dist(b, rc.start, rc.end) <= tol
}

/// Whether a triangle hit on upright piece `p0..q0` is inside a declared relief zone.
pub(super) fn tri_in_zone(
    tri: &Triangle,
    p0: Point3,
    q0: Point3,
    r_eff: f64,
    band: [f64; 2],
    allow: f64,
    zones: &[CornerReliefZone],
) -> bool {
    if zones.is_empty() {
        return false;
    }
    // Only the overlap of the facet and the swept atom can be a contact.
    // The flute can extend above the relief into air without gaining an
    // exemption for material above the relief.
    let contact_min = tri.a.z.min(tri.b.z).min(tri.c.z).max(band[0]);
    let contact_max = tri.a.z.max(tri.b.z).max(tri.c.z).min(band[1]);
    let a = Point2::new(p0.x, p0.y);
    let b = Point2::new(q0.x, q0.y);
    let z_min = p0.z.min(q0.z);
    zones.iter().any(|rc| {
        move_on_zone(a, b, z_min, p0.z.max(q0.z), r_eff, allow, rc)
            && contact_min >= rc.z_bottom - allow
            && contact_max <= rc.z_top + allow
    })
}

/// Whether an interior sample point along piece `p0..q0` stands inside a declared relief zone.
pub(super) fn mid_in_zone(
    p: Point3,
    p0: Point3,
    q0: Point3,
    band: [f64; 2],
    allow: f64,
    zones: &[CornerReliefZone],
) -> bool {
    if zones.is_empty() {
        return false;
    }
    let a = Point2::new(p0.x, p0.y);
    let b = Point2::new(q0.x, q0.y);
    let q = Point2::new(p.x, p.y);
    let z_min = p0.z.min(q0.z);
    zones.iter().any(|rc| {
        move_on_zone(a, b, z_min, p0.z.max(q0.z), rc.radius, allow, rc)
            && band[0] >= rc.z_bottom - allow
            && band[1] <= rc.z_top + allow
            && seg_point_dist(q, rc.start, rc.end) <= allow
            && p.z >= rc.z_bottom - allow
            && p.z <= rc.z_top + allow
    })
}
