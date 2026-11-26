//! The swept-ball guard of the Rotary Flow kernel (#2824, review of
//! PR #3630): safe stations do not prove a safe cut. Each emitted move
//! carries the ball center along a segment, and the guard proves that
//! segment clear of every facet by the EXACT segment-to-triangle
//! distance - the point, edge and face reduction of Ericson, "Real-Time
//! Collision Detection" 5.1 (the sim's `dist_seg_tri` law, written for
//! this crate, which does not depend on the sim).
//!
//! A posed move also turns its tool axis. The controller interpolates
//! the tip and the axis; the ball center then leaves the straight
//! segment by at most `r * (1 - cos(theta / 2))`, where `theta` is the
//! angle between the two poses (the normalized blend of two unit
//! vectors has the length `cos` of half the angle at its worst). The
//! guard adds that bound to the clearance that it demands, so the
//! proof covers the interpolated sweep too.
//!
//! Provenance: closest-point queries between convex primitives from
//! Ericson (2005), chapter 5; the interpolated-axis bound is elementary
//! vector geometry. No vendor material consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::rotary_flow_pass::TOUCH;

/// The pose a move without one stands under (`retip::tip_of`'s own
/// fallback: world up).
const UP: [f64; 3] = [0.0, 0.0, 1.0];

/// The squared distance from `p` to the segment `a -> b`.
fn pt_seg2(p: Point3, a: Point3, b: Point3) -> f64 {
    let ab = b - a;
    let l = ab.dot(ab);
    let t = if l > 0.0 {
        ((p - a).dot(ab) / l).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = Point3::new(a.x + ab.x * t, a.y + ab.y * t, a.z + ab.z * t);
    let d = p - q;
    d.dot(d)
}

/// The distance between the segments `p1 -> q1` and `p2 -> q2`: the
/// clamped two-line solve, with every endpoint-to-segment distance as
/// the complete candidate set for the degenerate and parallel cases.
fn seg_seg(p1: Point3, q1: Point3, p2: Point3, q2: Point3) -> f64 {
    let mut best = pt_seg2(p1, p2, q2)
        .min(pt_seg2(q1, p2, q2))
        .min(pt_seg2(p2, p1, q1))
        .min(pt_seg2(q2, p1, q1));
    let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (b, c) = (d1.dot(d2), d1.dot(r));
    let den = a * e - b * b;
    if a > 1e-30 && e > 1e-30 && den > 1e-18 * a * e {
        let s = ((b * f - c * e) / den).clamp(0.0, 1.0);
        let t = ((b * s + f) / e).clamp(0.0, 1.0);
        let s = ((b * t - c) / a).clamp(0.0, 1.0);
        let x = Point3::new(p1.x + d1.x * s, p1.y + d1.y * s, p1.z + d1.z * s);
        let y = Point3::new(p2.x + d2.x * t, p2.y + d2.y * t, p2.z + d2.z * t);
        let d = x - y;
        best = best.min(d.dot(d));
    }
    best.sqrt()
}

/// The exact distance between the segment `a -> b` and the facet `t`:
/// the two endpoint-to-face distances, the three edge pairs, and zero
/// where the segment pierces the face.
pub(super) fn seg_tri(a: Point3, b: Point3, t: &Triangle) -> f64 {
    let (e0, e1) = (t.b - t.a, t.c - t.a);
    let n = e0.cross(e1);
    let nn = n.dot(n);
    if nn > 1e-24 {
        let (da, db) = (n.dot(a - t.a), n.dot(b - t.a));
        if da * db < 0.0 {
            let s = da / (da - db);
            let ab = b - a;
            let w = Point3::new(a.x + ab.x * s, a.y + ab.y * s, a.z + ab.z * s) - t.a;
            let (u, v) = (w.cross(e1).dot(n) / nn, e0.cross(w).dot(n) / nn);
            if u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9 {
                return 0.0;
            }
        }
    }
    let pt = |p: Point3| {
        let d = p - closest_pt_tri(p, &[t.a, t.b, t.c]);
        d.dot(d).sqrt()
    };
    let mut d = pt(a).min(pt(b));
    for (p, q) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
        d = d.min(seg_seg(a, b, p, q));
    }
    d
}

/// The smallest distance from the segment `a -> b` to the facets whose
/// boxes come within `reach` of the segment's own box (a facet farther
/// away is at least `reach` off, so it cannot lower an answer below
/// `reach`). Infinite in empty air. Non-finite input reads 0: it fails
/// every clearance test closed.
pub(super) fn seg_clearance(
    a: Point3,
    b: Point3,
    facets: &[Triangle],
    boxes: &[[f64; 6]],
    reach: f64,
) -> f64 {
    if !(a.is_finite() && b.is_finite() && reach.is_finite()) {
        return 0.0;
    }
    let (x0, x1) = (a.x.min(b.x) - reach, a.x.max(b.x) + reach);
    let (y0, y1) = (a.y.min(b.y) - reach, a.y.max(b.y) + reach);
    let (z0, z1) = (a.z.min(b.z) - reach, a.z.max(b.z) + reach);
    let mut best = f64::INFINITY;
    for (f, bx) in facets.iter().zip(boxes) {
        if x1 < bx[0] || x0 > bx[3] || y1 < bx[1] || y0 > bx[4] || z1 < bx[2] || z0 > bx[5] {
            continue;
        }
        best = best.min(seg_tri(a, b, f));
    }
    best
}

/// The ball center a move's tip and pose state (the pivot law, the
/// inverse of `retip::tip_of`).
fn center(tip: Point3, pose: [f64; 3], r: f64) -> Point3 {
    Point3::new(tip.x + pose[0] * r, tip.y + pose[1] * r, tip.z + pose[2] * r)
}

/// The worst sideways drift of the ball center when the controller
/// blends the unit pose `p0` into `p1`: `r * (1 - cos(theta / 2))`.
fn axis_drift(p0: [f64; 3], p1: [f64; 3], r: f64) -> f64 {
    let c = (p0[0] * p1[0] + p0[1] * p1[1] + p0[2] * p1[2]).clamp(-1.0, 1.0);
    // cos(theta / 2) = sqrt((1 + cos theta) / 2)
    r * (1.0 - (0.5 * (1.0 + c)).sqrt())
}

/// What a move does, in the refusal's words.
fn what(k: MoveKind) -> &'static str {
    match k {
        MoveKind::Rapid => "rapid",
        MoveKind::Plunge => "plunge",
        _ => "feed move",
    }
}

/// The proof over the whole program: every move's ball sweep keeps
/// the ball radius `r` (less the oracle's float allowance) off every
/// facet, with the interpolated-axis drift added. The first move that
/// cannot be proved refuses the program by name, at its own spot -
/// never posted.
pub(super) fn prove_path(
    path: &Toolpath,
    facets: &[Triangle],
    boxes: &[[f64; 6]],
    r: f64,
) -> Result<(), GenError> {
    let mut prev: Option<[f64; 3]> = None;
    for (i, m) in path.moves.iter().enumerate() {
        let p1 = m.axis.or(prev).unwrap_or(UP);
        let p0 = prev.unwrap_or(p1);
        let (c0, c1) = (center(m.from, p0, r), center(m.to, p1, r));
        let need = r - TOUCH + axis_drift(p0, p1, r);
        let got = seg_clearance(c0, c1, facets, boxes, need + 1e-6);
        if got.is_nan() || need.is_nan() || got < need {
            return Err(GenError::invalid_at(
                format!(
                    "Rotary Flow: move {} (a {}) sweeps the ball {:.4} mm off the part, inside \
                     its {r:.3} mm radius - no clear path is proved to this station; pick \
                     guides on exposed ground, raise the clearance, or use a smaller ball",
                    i + 1,
                    what(m.kind),
                    got.max(0.0),
                ),
                c1,
            ));
        }
        prev = Some(p1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p3(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn seg_tri_reads_endpoints_edges_and_piercing() {
        let t = Triangle::new(p3(0.0, 0.0, 0.0), p3(4.0, 0.0, 0.0), p3(0.0, 4.0, 0.0));
        // Pierces the face.
        assert_eq!(seg_tri(p3(1.0, 1.0, -1.0), p3(1.0, 1.0, 1.0), &t), 0.0);
        // Above the face: the endpoint is nearest.
        assert!((seg_tri(p3(1.0, 1.0, 2.0), p3(1.0, 1.0, 3.0), &t) - 2.0).abs() < 1e-12);
        // Both endpoints far, the middle passes 0.5 over an edge: only
        // the edge pair witnesses it.
        let d = seg_tri(p3(2.0, -3.0, 0.5), p3(2.0, 3.0, 0.5), &t);
        assert!((d - 0.5).abs() < 1e-12, "{d}");
    }

    #[test]
    fn axis_drift_is_zero_for_a_held_pose_and_grows_with_the_turn() {
        assert!(axis_drift(UP, UP, 1.0).abs() < 1e-15);
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let d = axis_drift(UP, [s, 0.0, s], 1.0);
        let want = 1.0 - (std::f64::consts::PI / 8.0).cos();
        assert!((d - want).abs() < 1e-12, "{d} vs {want}");
    }
}
