//! The tangent touch of a corner arc (#2317): what the crossing law
//! (`arc_cross`) does not count as a crossing. A corner arc ends tangent
//! on its legs, and another feed often runs on a leg's own line (the
//! next ring's side, a link). The arc's center rides the 1e-6 mm
//! lattice, so its bracket dips some 1e-7 mm past that line and the
//! strict predicate reads a proper crossing where the arc only touches.
//!
//! The law here: a circle lies on one side of each of its tangents. Let
//! `E` be an end of the arc, `n` the unit radius there and `L` the
//! tangent line through `E`. Every point `X` of the arc's bracket (the
//! inscribed fan on the start radius, the end point itself, the
//! circumscribed hull) holds `(X - E) . n <= dr`, where `dr` is the
//! difference of the two end radii (the lattice step of the center).
//! So when a chord `q`, over the reach of the bracket along `L`, stays
//! within `d` of `L`, the arc passes `q` by `d + dr` at most. A pair is
//! a touch, not a crossing, only while `d + dr <=` [`TOUCH`]: the law
//! can hide no crossing deeper than [`TOUCH`] (1e-5 mm, a hundredth of
//! a micrometer). Anything it cannot measure is a crossing.
//!
//! Provenance: the tangent of a circle (Euclid, Elements III.16) and
//! the parametric clip of a segment to a slab (Liang-Barsky 1984);
//! clean-room, no vendor material consulted.

use crate::geo2d::Point2;

/// The deepest pass of a chord the law calls a touch, mm.
pub(super) const TOUCH: f64 = 1e-5;

/// One end of the arc: the point, the unit radius there and how far
/// the bracket reaches along the tangent from it.
#[derive(Clone, Copy)]
struct End {
    e: Point2,
    n: Point2,
    reach: f64,
}

/// A corner arc's two end tangents and its radius mismatch.
pub(super) struct Touch {
    ends: [End; 2],
    dr: f64,
}

impl Touch {
    /// The tangents of the arc about `center` whose bracket is `fan`
    /// and `hull` (both run from the arc's start to its end). `None`
    /// where a radius is not finite and positive.
    pub(super) fn of(center: Point2, fan: &[Point2], hull: &[Point2]) -> Option<Touch> {
        let (from, to) = (*fan.first()?, *fan.last()?);
        let end = |e: Point2| -> Option<(End, f64)> {
            let (dx, dy) = (e.x - center.x, e.y - center.y);
            let r = dx.hypot(dy);
            if !(r.is_finite() && r > 0.0) {
                return None;
            }
            let n = Point2::new(dx / r, dy / r);
            let along = |p: &Point2| ((p.x - e.x) * -n.y + (p.y - e.y) * n.x).abs();
            let reach = fan.iter().chain(hull).map(along).fold(0.0, f64::max);
            reach.is_finite().then_some((End { e, n, reach }, r))
        };
        let ((a, r0), (b, r1)) = (end(from)?, end(to)?);
        Some(Touch { ends: [a, b], dr: (r0 - r1).abs() })
    }

    /// The chord `a b` only touches the tangent at an end of the arc:
    /// over the bracket's reach it stays within [`TOUCH`] of that line,
    /// the radius mismatch counted in.
    pub(super) fn grazes(&self, a: Point2, b: Point2) -> bool {
        self.ends.iter().any(|end| {
            let at = |p: Point2| {
                let (dx, dy) = (p.x - end.e.x, p.y - end.e.y);
                (dx * -end.n.y + dy * end.n.x, dx * end.n.x + dy * end.n.y)
            };
            let ((sa, da), (sb, db)) = (at(a), at(b));
            // The part of the chord inside the slab |s| <= reach.
            let (lo, hi) = if sa == sb {
                if sa.abs() > end.reach {
                    return false;
                }
                (0.0, 1.0)
            } else {
                let (u, v) = ((-end.reach - sa) / (sb - sa), (end.reach - sa) / (sb - sa));
                (u.min(v).max(0.0), u.max(v).min(1.0))
            };
            // An empty clip or a NaN: not measured, so not a touch.
            if lo.is_nan() || hi.is_nan() || lo > hi {
                return false;
            }
            let d = |u: f64| (da + (db - da) * u).abs();
            d(lo).max(d(hi)) + self.dr <= TOUCH
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::arc_proof::bracket;
    use super::*;

    fn touch(center: Point2) -> Touch {
        // The bracket of a 0.5 mm corner arc from (0, 0.5) to (0.5, 0)
        // about (0.5, 0.5): tangent to the line y = 0 at its end.
        let c = Point2::new(0.5, 0.5);
        let Some((fan, hull)) = bracket(Point2::new(0.0, 0.5), c, true, Point2::new(0.5, 0.0)) else {
            panic!("bracket")
        };
        let Some(t) = Touch::of(center, &fan, &hull) else { panic!("touch") };
        t
    }

    /// A chord on the tangent line, or within the bound of it, is a
    /// touch; a chord past the bound, or one that cuts through the arc,
    /// is not.
    #[test]
    fn only_a_chord_on_the_end_tangent_is_a_touch() {
        let t = touch(Point2::new(0.5, 0.5));
        let on = |y0: f64, y1: f64| t.grazes(Point2::new(-20.0, y0), Point2::new(20.0, y1));
        assert!(on(0.0, 0.0), "on the tangent");
        assert!(on(7e-8, 7e-8) && on(-7e-8, -7e-8), "the lattice dip");
        assert!(!on(2e-5, 2e-5) && !on(-2e-5, -2e-5), "past the bound");
        // Tilted 1e-6 rad: still a touch over the reach of the arc...
        assert!(on(-2e-5, 2e-5));
        // ...and a real slope through the arc is not.
        assert!(!on(-0.2, 0.2));
        assert!(!t.grazes(Point2::new(0.2, -1.0), Point2::new(0.2, 1.0)), "a chord across the arc");
        assert!(!t.grazes(Point2::new(5.0, 0.0), Point2::new(9.0, 0.0)), "beyond the reach: not measured");
        assert!(!t.grazes(Point2::new(f64::NAN, 0.0), Point2::new(1.0, 0.0)));
    }

    /// The bound counts the radius mismatch in: an arc whose ends stand
    /// on radii more than the bound apart has no touch at all.
    #[test]
    fn a_radius_mismatch_past_the_bound_leaves_no_touch() {
        let t = touch(Point2::new(0.5, 0.5 + 3e-5));
        assert!(!t.grazes(Point2::new(-2.0, 0.0), Point2::new(2.0, 0.0)));
    }
}
