//! Tangent joints of a model profile's read-back arcs (#3215). A fillet
//! read from facet vertices meets its straight sides a few nanoradians
//! off tangent (the vertices stand on a nanometre lattice). The analytic
//! wire contour (#3937) would close each such joint with a pivot arc of
//! no length. So an arc whose end meets a straight side within
//! [`TANGENT_TOL`] has its centre moved onto that side's normal through
//! the joint; the moved arc must still hold every vertex of its run
//! within the fit tolerance, or the fitted arc stays as it was.
//!
//! Provenance: our own implementation. Plane geometry only: a circle
//! tangent to a line at a point has its centre on the line's normal
//! there; two such normals meet at the centre of the fillet.

use crate::geo2d::Point2;

use super::arc_span::{add, cross, dist, dot, left, mul, sub, unit, Span};
use super::model_arcs::judge;

/// How far (radians) an arc's end may stand off its straight neighbour's
/// direction and still be made tangent to it.
pub const TANGENT_TOL: f64 = 1e-4;

/// Make each arc of `spans` tangent to its straight neighbours (module
/// head). `runs[i]` is the vertex run of `walk` that arc `i` stands on.
pub(super) fn snap_tangents(spans: &mut [Span], runs: &[Option<(usize, usize)>], walk: &[Point2], bound: f64) {
    let n = spans.len();
    for i in 0..n {
        let (Span::Arc { c, r, a, b, ccw, .. }, Some((s, e))) = (spans[i], runs.get(i).copied().flatten()) else { continue };
        let side = |t: Point2| if ccw { left(t) } else { mul(left(t), -1.0) };
        // The arc's tangent at a point, and a straight neighbour's
        // direction with the normal toward the arc's centre.
        let tangent = |q: Point2| unit(sub(q, c)).map(|u| if ccw { left(u) } else { mul(left(u), -1.0) });
        let along = |sp: Span, t: Option<Point2>| match (sp, t) {
            (Span::Line { a: la, b: lb }, Some(t)) => unit(sub(lb, la)).filter(|u| cross(*u, t).abs() <= TANGENT_TOL && dot(*u, t) > 0.0).map(side),
            _ => None,
        };
        let na = along(spans[(i + n - 1) % n], tangent(a));
        let nb = along(spans[(i + 1) % n], tangent(b));
        let moved = match (na, nb) {
            (Some(na), Some(nb)) => {
                let d = cross(na, nb);
                if d.abs() > 1e-6 {
                    Some(add(a, mul(na, cross(sub(b, a), nb) / d)))
                } else if dot(na, nb) < 0.0 {
                    // Parallel sides: a half circle between them.
                    Some(mul(add(a, b), 0.5))
                } else {
                    None
                }
            }
            (Some(na), None) => Some(add(a, mul(na, dot(sub(c, a), na)))),
            (None, Some(nb)) => Some(add(b, mul(nb, dot(sub(c, b), nb)))),
            (None, None) => None,
        };
        let Some(c2) = moved else { continue };
        let r2 = match (na, nb) {
            (Some(_), Some(_)) => 0.5 * (dist(c2, a) + dist(c2, b)),
            (Some(_), None) => dist(c2, a),
            _ => dist(c2, b),
        };
        if !(r2.is_finite() && (r2 - r).abs() <= 10.0 * super::model_arcs::FIT_TOL) {
            continue;
        }
        let Some(run) = walk.get(s..=e) else { continue };
        if let Some((sweep, _)) = judge(run, c2, r2, ccw, bound, false) {
            spans[i] = Span::Arc { c: c2, r: r2, a, b, ccw, sweep };
        }
    }
}
