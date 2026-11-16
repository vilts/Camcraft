//! Boundary parameters at a collapsed edge (a pole) of a B-spline or NURBS
//! STEP face: the parameter search's answer is brought back inside the
//! surface's knot domain (#1548).
//!
//! Provenance: ISO 10303-42 (a `b_spline_surface` is defined on its knot
//! domain only) and the degenerate-patch treatment in Piegl & Tiller, "The
//! NURBS Book" (2nd ed., ch. 4-5: a patch edge whose control points coincide
//! maps one whole parameter side to a single point). Our own design around
//! truck's published parameter search (ricosjp, Apache-2.0), called
//! unchanged. Nothing from any CAM vendor.
//!
//! Why it is needed: the corner blend where three fillets meet is exported
//! as a rational patch with one side collapsed to a point - every parameter
//! along that side is a pre-image of the corner vertex. truck finds the
//! parameters of a face's boundary points by Newton iteration; at the pole
//! the Jacobian is singular and the free parameter comes back anywhere (-1.2e9
//! on the #1536 model), still "correct" because the extrapolated rational
//! surface passes through the same point. truck then subdivides the bounding
//! box of the boundary's parameters - a billion knot spans of extrapolation,
//! which never meets the tolerance (the #1536 hang) and, if it did, would
//! trim the face against a spike that cuts the pole's neighbourhood off.
//!
//! The rule: an answer inside the knot domain is returned untouched. One
//! outside it is replaced by a parameter inside the domain - the previous
//! boundary point's value for each coordinate that left it, else the nearest
//! domain bound - but only when the surface at the replacement is the same
//! point. Anything else is left as truck found it, so a face whose boundary
//! really runs off its surface behaves as before (metered by `step_guard`).

use truck_stepio::r#in::alias::{MetricSpace, ParametricSurface, Point3, Surface};

/// How far a replacement parameter's point may stand from the searched
/// point: truck's own acceptance distance for a parameter search (its
/// `TOLERANCE`, in model units).
const SAME_POINT: f64 = 1.0e-6;
/// Slack on the knot domain, as a fraction of its span: Newton's answer on
/// a domain bound lands a few ulps either side of it.
const DOMAIN_SLACK: f64 = 1.0e-9;

type Domain = ((f64, f64), (f64, f64));

/// The knot domain of a B-spline or NURBS surface; `None` for the other
/// kinds (their parameters are not bounded by knots) and for a domain that
/// is not a finite, non-empty box.
pub(super) fn knot_domain(s: &Surface) -> Option<Domain> {
    if !matches!(s, Surface::BSplineSurface(_) | Surface::NurbsSurface(_)) {
        return None;
    }
    let (u, v) = s.try_range_tuple();
    let ((u0, u1), (v0, v1)) = (u?, v?);
    let ok = [u0, u1, v0, v1].iter().all(|x| x.is_finite()) && u0 < u1 && v0 < v1;
    ok.then_some(((u0, u1), (v0, v1)))
}

/// `x` when it lies in `[lo, hi]` (with slack), else the replacement:
/// `carried` when that lies in the domain, else the nearer bound.
fn inside(x: f64, (lo, hi): (f64, f64), carried: Option<f64>) -> (f64, bool) {
    let slack = (hi - lo) * DOMAIN_SLACK;
    if x >= lo - slack && x <= hi + slack {
        return (x, false);
    }
    match carried {
        Some(c) if c >= lo && c <= hi => (c, true),
        _ => (if x < lo { lo } else { hi }, true),
    }
}

/// Bring a parameter-search answer for `point` back inside the knot domain
/// of `surface` when the surface there is the same point (a pole); `hint`
/// is the previous boundary point's parameter. See the module comment.
pub(super) fn settle(surface: &Surface, point: Point3, hint: Option<(f64, f64)>, found: (f64, f64)) -> (f64, f64) {
    let Some((urange, vrange)) = knot_domain(surface) else {
        return found;
    };
    let (u, moved_u) = inside(found.0, urange, hint.map(|h| h.0));
    let (v, moved_v) = inside(found.1, vrange, hint.map(|h| h.1));
    if !(moved_u || moved_v) {
        return found;
    }
    // A NaN distance fails the comparison: the answer stays as found.
    if surface.subs(u, v).distance2(point) <= SAME_POINT * SAME_POINT {
        (u, v)
    } else {
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use truck_stepio::r#in::alias::{BSplineSurface, ElementarySurface, KnotVec, Plane};

    const POLE: Point3 = Point3::new(0.0, 0.0, 0.0);

    /// A bilinear patch whose side u = 0 is one point: S(0, v) is the pole
    /// for every v, inside the knot domain [0, 1] and far outside it.
    fn patch() -> Surface {
        let k = KnotVec::bezier_knot(1);
        let cps = vec![vec![POLE, POLE], vec![Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0)]];
        Surface::BSplineSurface(Box::new(BSplineSurface::new((k.clone(), k), cps)))
    }

    #[test]
    fn an_answer_inside_the_knot_domain_is_returned_untouched() {
        let s = patch();
        // Not even the searched point: inside the domain nothing is second-guessed.
        assert_eq!(settle(&s, POLE, Some((0.9, 0.9)), (0.3, 0.4)), (0.3, 0.4));
        // A few ulps over a bound is Newton's noise, not an escape.
        assert_eq!(settle(&s, POLE, None, (-1.0e-12, 1.0 + 1.0e-12)), (-1.0e-12, 1.0 + 1.0e-12));
    }

    #[test]
    fn a_pole_answer_far_outside_takes_the_previous_boundary_points_parameter() {
        // The #1536 reading: the free parameter at the pole came back -1.2e9.
        let s = patch();
        assert_eq!(s.subs(0.0, -1.2e9), POLE, "the extrapolated surface still passes through the pole");
        assert_eq!(settle(&s, POLE, Some((0.2, 1.0)), (0.0, -1.2e9)), (0.0, 1.0));
        assert_eq!(settle(&s, POLE, Some((0.2, 0.25)), (0.0, 7.0e8)), (0.0, 0.25));
    }

    #[test]
    fn a_pole_answer_with_no_usable_hint_goes_to_the_nearer_domain_bound() {
        let s = patch();
        assert_eq!(settle(&s, POLE, None, (0.0, -1.2e9)), (0.0, 0.0));
        assert_eq!(settle(&s, POLE, None, (0.0, 7.0e8)), (0.0, 1.0));
        // A hint that is itself outside the domain is not carried.
        assert_eq!(settle(&s, POLE, Some((0.0, -5.0)), (0.0, -1.2e9)), (0.0, 0.0));
    }

    #[test]
    fn an_outside_answer_that_is_not_a_pole_is_left_as_truck_found_it() {
        // S(1, 2) = (-1, 2, 0) is on the extrapolated surface only; the
        // domain's nearest parameter (1, 1) is a different point, so the
        // answer must not be moved onto it.
        let s = patch();
        let off = s.subs(1.0, 2.0);
        assert!(off.distance2(s.subs(1.0, 1.0)) > 1.0);
        assert_eq!(settle(&s, off, Some((1.0, 0.9)), (1.0, 2.0)), (1.0, 2.0));
    }

    #[test]
    fn a_surface_without_knots_is_never_touched() {
        let plane = Plane::new(POLE, Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0));
        let s = Surface::ElementarySurface(Box::new(ElementarySurface::Plane(plane)));
        assert!(knot_domain(&s).is_none());
        assert_eq!(settle(&s, POLE, Some((0.0, 0.0)), (-1.2e9, 55.0)), (-1.2e9, 55.0));
    }

    #[test]
    fn a_non_finite_answer_ends_cleanly() {
        let s = patch();
        for found in [(f64::NAN, 0.5), (0.0, f64::INFINITY), (f64::NEG_INFINITY, f64::NAN)] {
            let (u, v) = settle(&s, POLE, Some((0.2, 0.5)), found);
            // Either left as found, or moved to a parameter that is the point.
            let moved = u.is_finite() && v.is_finite();
            assert!(!moved || s.subs(u, v).distance2(POLE) <= SAME_POINT * SAME_POINT, "{found:?} -> ({u}, {v})");
        }
    }
}
