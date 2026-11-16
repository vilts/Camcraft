//! Whether a STEP face's surface is proven continuous over its whole
//! parameter range, from its definition alone (#1568): the test that lets
//! a finer tessellation pass that ran out of evaluations fall back to the
//! coarser pass's mesh, and refuses that fallback for a surface that may
//! jump (#1536's smear).
//!
//! Provenance: our own design, on the standard B-spline facts (Piegl &
//! Tiller, "The NURBS Book", 2nd ed., sections 2.2-2.3 and 4.2): a degree-p
//! B-spline is C^(p-m) across an interior knot of multiplicity m, so it is
//! at least C0 wherever m <= p; a rational one with positive weights keeps
//! the continuity of its numerator and denominator. Nothing from any CAM
//! vendor.
//!
//! Why it is enough: truck's subdivision splits a cell while its midpoint
//! stands off the bilinear patch of its corners by more than the
//! tolerance. On a surface continuous over a closed parameter box (hence
//! uniformly continuous) that deviation goes to zero as cells shrink, so
//! the subdivision ends at every tolerance: a pass that ran out of
//! evaluations was only expensive, and the coarser pass it followed is a
//! converged mesh at its own (reported) tolerance. A surface that may jump
//! has no such guarantee - a pass wider than its gap converges with the
//! jump smeared into a wall - so it is never "proven".

use truck_stepio::r#in::alias::{KnotVec, Surface};

/// The smallest NURBS weight, relative to the net's largest, that still
/// counts as proven (#1568 review). Real CAD weights stay within a few
/// decades of each other; 1e-9 refuses only a near-pole.
const WEIGHT_FLOOR: f64 = 1e-9;

/// True only when `s` is proven continuous (C0) over its whole parameter
/// range. Unknown is `false`: the caller then refuses, never guesses.
pub(super) fn proven_continuous(s: &Surface) -> bool {
    match s {
        // Planes, cylinders, cones, spheres and tori are analytic.
        Surface::ElementarySurface(_) => true,
        // An extruded or revolved curve may itself be a B-spline that
        // jumps; not proven here (no runaway is known on one, #1545).
        Surface::SweptCurve(_) => false,
        Surface::BSplineSurface(b) => {
            let rows = b.control_points();
            let finite = rows.iter().flatten().all(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite());
            finite && knots_continuous(b.knot_vecs(), rows)
        }
        Surface::NurbsSurface(n) => {
            let b = n.non_rationalized();
            let rows = b.control_points();
            // A zero or negative weight puts a pole (or a sign flip of the
            // denominator) in the range: not continuous. A weight below
            // WEIGHT_FLOOR of the largest is treated the same: the surface
            // is continuous, but nearly a pole, and a coarser mesh checked
            // at one point per cell can miss the narrow spike it makes.
            let finite = rows.iter().flatten().all(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite() && p.w.is_finite());
            let top = rows.iter().flatten().map(|p| p.w).fold(0.0f64, f64::max);
            let weights = rows.iter().flatten().all(|p| p.w > 0.0 && p.w >= WEIGHT_FLOOR * top);
            finite && weights && knots_continuous(b.knot_vecs(), rows)
        }
    }
}

/// Both knot vectors keep every interior multiplicity at or under the
/// degree (and the degrees are well formed for the control net).
fn knots_continuous<P>((uk, vk): &(KnotVec, KnotVec), rows: &[Vec<P>]) -> bool {
    let cols = rows.first().map_or(0, Vec::len);
    // degree = knots - control points - 1, checked: a malformed net is not
    // proven anything.
    let degree = |k: &KnotVec, n: usize| k.len().checked_sub(n)?.checked_sub(1);
    let (Some(p), Some(q)) = (degree(uk, rows.len()), degree(vk, cols)) else {
        return false;
    };
    interior_at_most(uk, p) && interior_at_most(vk, q)
}

/// Every distinct interior knot of `k` has multiplicity at most `degree`,
/// and every knot is finite.
fn interior_at_most(k: &KnotVec, degree: usize) -> bool {
    let (knots, mults) = k.to_single_multi();
    if knots.len() < 2 || !knots.iter().all(|t| t.is_finite()) {
        return false;
    }
    let interior = knots.len() - 2;
    mults.iter().skip(1).take(interior).all(|&m| m <= degree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use truck_stepio::r#in::alias::{BSplineSurface, NurbsSurface, Point3, Vector4};

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// Degree 1 in u; `uk` decides whether the knot 1.0 is repeated past
    /// the degree (a jump) or not.
    fn bilinear(uk: Vec<f64>, rows: usize) -> Surface {
        let cps = (0..rows).map(|i| vec![p(i as f64, 0.0, 0.0), p(i as f64, 1.0, (i % 2) as f64)]).collect();
        Surface::BSplineSurface(Box::new(BSplineSurface::new((KnotVec::from(uk), KnotVec::bezier_knot(1)), cps)))
    }

    #[test]
    fn an_interior_knot_repeated_past_the_degree_is_not_proven() {
        // #1536's fixture shape: degree 1, knot 1.0 twice -> may jump.
        assert!(!proven_continuous(&bilinear(vec![0.0, 0.0, 1.0, 1.0, 2.0, 2.0], 4)));
        // The same degree with the knot once: C0, proven.
        assert!(proven_continuous(&bilinear(vec![0.0, 0.0, 1.0, 2.0, 2.0], 3)));
    }

    #[test]
    fn a_smooth_patch_is_proven_and_end_knots_do_not_count() {
        let k = KnotVec::bezier_knot(2);
        let cps = (0..3).map(|i| (0..3).map(|j| p(i as f64, j as f64, ((i * j) % 2) as f64)).collect()).collect();
        assert!(proven_continuous(&Surface::BSplineSurface(Box::new(BSplineSurface::new((k.clone(), k), cps)))));
    }

    #[test]
    fn a_nurbs_needs_every_weight_positive() {
        let k = KnotVec::bezier_knot(1);
        let net = |w: f64| {
            let v = |x: f64, y: f64| Vector4::new(x, y, 0.0, 1.0);
            vec![vec![v(0.0, 0.0), v(0.0, 1.0)], vec![v(1.0, 0.0), Vector4::new(w, w, 0.0, w)]]
        };
        let s = |w| Surface::NurbsSurface(Box::new(NurbsSurface::new(BSplineSurface::new((k.clone(), k.clone()), net(w)))));
        assert!(proven_continuous(&s(1.0)));
        assert!(!proven_continuous(&s(0.0)), "a zero weight is a pole");
        assert!(!proven_continuous(&s(-1.0)), "a negative weight flips the denominator");
    }

    /// A 2 x 2 rational net whose last point is `last` (homogeneous).
    fn rational(last: Vector4) -> Surface {
        let k = KnotVec::bezier_knot(1);
        let v = |x: f64, y: f64| Vector4::new(x, y, 0.0, 1.0);
        let net = vec![vec![v(0.0, 0.0), v(0.0, 1.0)], vec![v(1.0, 0.0), last]];
        Surface::NurbsSurface(Box::new(NurbsSurface::new(BSplineSurface::new((k.clone(), k), net))))
    }

    #[test]
    fn a_nurbs_with_a_non_finite_coordinate_or_weight_is_not_proven() {
        assert!(proven_continuous(&rational(Vector4::new(1.0, 1.0, 0.0, 1.0))));
        assert!(!proven_continuous(&rational(Vector4::new(f64::NAN, 1.0, 0.0, 1.0))), "NaN x");
        assert!(!proven_continuous(&rational(Vector4::new(1.0, f64::INFINITY, 0.0, 1.0))), "infinite y");
        assert!(!proven_continuous(&rational(Vector4::new(1.0, 1.0, f64::NAN, 1.0))), "NaN z");
        assert!(!proven_continuous(&rational(Vector4::new(1.0, 1.0, 0.0, f64::INFINITY))), "infinite w");
        // Every weight infinite: no relative floor can catch it.
        let k = KnotVec::bezier_knot(1);
        let inf = |x: f64, y: f64| Vector4::new(x, y, 0.0, f64::INFINITY);
        let net = vec![vec![inf(0.0, 0.0), inf(0.0, 1.0)], vec![inf(1.0, 0.0), inf(1.0, 1.0)]];
        let s = Surface::NurbsSurface(Box::new(NurbsSurface::new(BSplineSurface::new((k.clone(), k), net))));
        assert!(!proven_continuous(&s), "every weight infinite");
    }

    /// #1568 review: a weight far below the net's largest is nearly a pole;
    /// not proven, though positive. A spread of a million is ordinary.
    #[test]
    fn a_near_zero_weight_relative_to_the_net_is_not_proven() {
        assert!(proven_continuous(&rational(Vector4::new(1e-6, 1e-6, 0.0, 1e-6))));
        assert!(!proven_continuous(&rational(Vector4::new(1e-12, 1e-12, 0.0, 1e-12))));
    }

    #[test]
    fn a_non_finite_knot_is_not_proven() {
        // Degree 1, interior knot 1.0 once (C0), but the end knot infinite.
        assert!(!proven_continuous(&bilinear(vec![0.0, 0.0, 1.0, f64::INFINITY, f64::INFINITY], 3)));
    }

    #[test]
    fn a_swept_surface_is_not_proven() {
        use truck_stepio::r#in::alias::{Curve3D, ExtrudedCurve, Line, SweptCurve, Vector3};
        let line = Curve3D::Line(Line(p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0)));
        let swept = SweptCurve::ExtrudedCurve(ExtrudedCurve::by_extrusion(line, Vector3::new(0.0, 0.0, 1.0)));
        assert!(!proven_continuous(&Surface::SweptCurve(Box::new(swept))));
    }

    #[test]
    fn a_non_finite_control_point_is_not_proven() {
        let k = KnotVec::bezier_knot(1);
        let cps = vec![vec![p(0.0, 0.0, 0.0), p(0.0, 1.0, 0.0)], vec![p(1.0, 0.0, f64::NAN), p(1.0, 1.0, 0.0)]];
        assert!(!proven_continuous(&Surface::BSplineSurface(Box::new(BSplineSurface::new((k.clone(), k), cps)))));
    }
}
