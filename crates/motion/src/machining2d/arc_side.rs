//! One-sided arc fit for a wall walk (#1579): when the walk knows on
//! which side of travel its part lies, the fitted circle may pass a
//! vertex only on the air side, never between the vertex and the wall.
//!
//! The two-sided fit (`arcs::arc_over`) lets a vertex stand up to half
//! the fit tolerance off its circle in either direction. Toward the part
//! that is a cut past a verified point, and the verifier forgives only
//! its fixed gouge allowance (0.01 mm), not the operation's tolerance -
//! so a coarse fit (0.1) on a tessellated round wall posted arcs that
//! the verify then held as gouges. Here the circle stays through the
//! run's two end vertices (the arc's endpoints are path points, so the
//! chaining stays exact) and its centre moves along their chord's
//! perpendicular bisector until no interior vertex lies on the part
//! side. Each vertex bounds the centre's offset `t` along that bisector
//! linearly, so the feasible offsets are one interval; the offset
//! nearest the two-sided circle is taken. No interval: no arc.
//!
//! Provenance: circle-through-two-points family and the power of a
//! point (elementary plane geometry); no vendor material consulted.

use crate::geo2d::Point2;

/// How far a vertex may stand on the part side of a fitted arc before
/// the fit counts it (#1579), in mm. Far above the rounding noise of a
/// true circle (1e-9 refused exact holes and bores whose vertices sat
/// 1e-12 off), ten times below the verifier's `GOUGE_TOL` (0.01) - so no
/// arc passes a vertex near what the verify would hold as a gouge.
pub(super) const ARC_SIDE_SLACK: f64 = 1e-3;

/// How many refused run ends a sided fit looks past before its run ends
/// (#1579): see `arcs::fit_arcs_sided`.
pub(super) const ARC_SIDE_LOOKAHEAD: usize = 4;

/// The circle a run takes on a wall walk whose air is left of travel
/// when `air_left` is `Some(true)` (`None`: the two-sided `(c, r)` as it
/// is). The centre is left of travel on a `ccw` arc, so the part is on
/// the centre's side exactly when the air is not. A vertex within
/// [`ARC_SIDE_SLACK`] of the two-sided circle (rounding noise on a true
/// circle) keeps that circle, so an exact bore or boss fits as before;
/// only a vertex past the slack moves the centre ([`one_sided`], an open
/// run) or refuses the arc (a `closing` ring, whose circle has no chord
/// to slide along). `None`: no arc.
pub(super) fn sided(run: &[Point2], c: Point2, r: f64, ccw: bool, closing: bool, air_left: Option<bool>) -> Option<(Point2, f64)> {
    let Some(air_left) = air_left else {
        return Some((c, r));
    };
    let part_in = air_left != ccw;
    if air_side_holds(run, c, r, part_in, ARC_SIDE_SLACK) {
        Some((c, r))
    } else if closing {
        None
    } else {
        let (a, b) = (*run.first()?, *run.last()?);
        one_sided(run, a, b, c, part_in)
    }
}

/// The circle through `a` and `b` nearest `(c, _)` that keeps every
/// point of `run` on the air side: inside or on the circle when the part
/// lies on the centre's side of the arc (`part_in`, an outside wall), on
/// or outside it otherwise (a pocket wall). `None` when no such circle
/// exists or the chord `a`-`b` is degenerate.
pub(super) fn one_sided(run: &[Point2], a: Point2, b: Point2, c: Point2, part_in: bool) -> Option<(Point2, f64)> {
    let m = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if !(len.is_finite() && len > 0.0) {
        return None;
    }
    let n = Point2::new(-dy / len, dx / len);
    let h2 = (len * 0.5).powi(2);
    // Centre c(t) = m + t n, radius^2 = h^2 + t^2. A point p is inside
    // or on the circle iff |p-m|^2 - h^2 <= 2 t (p-m).n - linear in t.
    // The end vertices are on every circle of the family: only the
    // interior ones bound `t` (their rounding noise would not).
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for p in run.iter().take(run.len().saturating_sub(1)).skip(1) {
        let (px, py) = (p.x - m.x, p.y - m.y);
        let q = px * px + py * py - h2;
        let k = px * n.x + py * n.y;
        if !(q.is_finite() && k.is_finite()) {
            return None;
        }
        // inside: 2tk >= q; outside: 2tk <= q.
        let (k, q) = if part_in { (k, q) } else { (-k, -q) };
        if k.abs() <= f64::EPSILON {
            // The bound does not depend on t: a point on the chord's own
            // line, which is on the air side only when q <= 0.
            if q > 0.0 {
                return None;
            }
        } else if k > 0.0 {
            lo = lo.max(q / (2.0 * k));
        } else {
            hi = hi.min(q / (2.0 * k));
        }
    }
    if lo > hi {
        return None;
    }
    let t0 = (c.x - m.x) * n.x + (c.y - m.y) * n.y;
    let t = t0.clamp(lo, hi);
    if !t.is_finite() {
        return None;
    }
    let center = Point2::new(m.x + t * n.x, m.y + t * n.y);
    let r = (h2 + t * t).sqrt();
    r.is_finite().then_some((center, r))
}

/// True when every point of `run` lies on the air side of the circle
/// `(c, r)` within `slack` (see [`one_sided`] for `part_in`): the check
/// a closing ring takes, whose circle has no chord to slide along.
pub(super) fn air_side_holds(run: &[Point2], c: Point2, r: f64, part_in: bool, slack: f64) -> bool {
    run.iter().all(|p| {
        let s = (p.x - c.x).hypot(p.y - c.y) - r;
        if part_in { s <= slack } else { s >= -slack }
    })
}

/// True when every vertex a sweep split into `k` pieces ends a piece on
/// (the run's `len * m / k`, `0 < m < k`, as `arcs::fit_arcs_sided`
/// splits) lies within [`ARC_SIDE_SLACK`] of the circle `(c, r)`.
pub(super) fn splits_on_circle(run: &[Point2], c: Point2, r: f64, k: usize) -> bool {
    let last = run.len().saturating_sub(1);
    (1..k).all(|m| {
        run.get(last * m / k)
            .is_some_and(|p| ((p.x - c.x).hypot(p.y - c.y) - r).abs() <= ARC_SIDE_SLACK)
    })
}
