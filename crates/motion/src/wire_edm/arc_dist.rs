//! Exact distances and crossings between the spans of an analytic wire
//! profile (#3937): point to line or arc, span to span, and the meets of
//! two parallels at a corner.
//!
//! Provenance: textbook Euclidean geometry. The least distance between
//! a segment and an arc that do not meet is at an end of one of them, or
//! on the normal from the circle's centre to the segment; between two
//! arcs at an end, or on the line of the two centres (the critical points
//! of the distance between two circles). Line and circle crossings are
//! the quadratic of the substitution. Our own implementation.

use crate::geo2d::Point2;

use super::arc_span::{add, cross, dist, dot, mul, sub, unit, Span};

/// The point of segment `a`-`b` nearest `q`.
pub(super) fn foot(q: Point2, a: Point2, b: Point2) -> Point2 {
    let d = sub(b, a);
    let l2 = dot(d, d);
    if l2.is_nan() || l2 <= 0.0 {
        return a;
    }
    let t = (dot(sub(q, a), d) / l2).clamp(0.0, 1.0);
    add(a, mul(d, t))
}

/// The point of `s` nearest `q`, and its distance. On an arc whose centre
/// is `q` itself every point is equally near: the point with the least
/// counter-clockwise angle from +X wins (the stated tie rule, #3937).
pub(super) fn nearest(q: Point2, s: &Span) -> (f64, Point2) {
    match *s {
        Span::Line { a, b } => {
            let f = foot(q, a, b);
            (dist(q, f), f)
        }
        Span::Arc { c, r, a, b, .. } => {
            let Some(u) = unit(sub(q, c)) else {
                return (r, tie_point(s));
            };
            if s.holds_dir(q) {
                let p = add(c, mul(u, r));
                return (dist(q, p), p);
            }
            let (da, db) = (dist(q, a), dist(q, b));
            if db < da { (db, b) } else { (da, a) }
        }
    }
}

/// The arc's point at the least counter-clockwise angle from +X, seen from
/// its centre: +X itself when the sweep holds it, else the nearer end in
/// angle.
pub(super) fn tie_point(s: &Span) -> Point2 {
    let Span::Arc { c, r, a, b, .. } = *s else { return s.start() };
    let east = Point2::new(c.x + r, c.y);
    if s.holds_dir(east) {
        return east;
    }
    let ang = |p: Point2| (p.y - c.y).atan2(p.x - c.x).rem_euclid(std::f64::consts::TAU);
    if ang(b) < ang(a) { b } else { a }
}

pub(super) fn pt_span(q: Point2, s: &Span) -> f64 {
    nearest(q, s).0
}

/// Where the line through `a`-`b` (parameter `t`, 0 at `a`) meets the
/// circle about `c` of radius `r`.
fn line_circle(a: Point2, b: Point2, c: Point2, r: f64) -> Vec<(f64, Point2)> {
    let d = sub(b, a);
    let f = sub(a, c);
    let (qa, qb, qc) = (dot(d, d), 2.0 * dot(f, d), dot(f, f) - r * r);
    if qa.is_nan() || qa <= 0.0 {
        return Vec::new();
    }
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 || !disc.is_finite() {
        return Vec::new();
    }
    let sq = disc.sqrt();
    [(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)].into_iter().map(|t| (t, add(a, mul(d, t)))).collect()
}

/// Where two circles meet.
fn circle_circle(c1: Point2, r1: f64, c2: Point2, r2: f64) -> Vec<Point2> {
    let d = dist(c1, c2);
    if d.is_nan() || d <= 0.0 || d > r1 + r2 || d < (r1 - r2).abs() {
        return Vec::new();
    }
    let x = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
    let h = (r1 * r1 - x * x).max(0.0).sqrt();
    let u = mul(sub(c2, c1), 1.0 / d);
    let m = add(c1, mul(u, x));
    let n = Point2::new(-u.y, u.x);
    vec![add(m, mul(n, h)), sub(m, mul(n, h))]
}

/// Where the carrier lines / circles of two spans meet (ignoring their
/// ends): the candidates of a trim at a corner.
pub(super) fn carriers_meet(s1: &Span, s2: &Span) -> Vec<Point2> {
    match (*s1, *s2) {
        (Span::Line { a, b }, Span::Line { a: c, b: d }) => {
            let (r, s) = (sub(b, a), sub(d, c));
            let den = cross(r, s);
            if den == 0.0 || !den.is_finite() {
                return Vec::new();
            }
            let t = cross(sub(c, a), s) / den;
            vec![add(a, mul(r, t))]
        }
        (Span::Line { a, b }, Span::Arc { c, r, .. }) | (Span::Arc { c, r, .. }, Span::Line { a, b }) => {
            line_circle(a, b, c, r).into_iter().map(|(_, p)| p).collect()
        }
        (Span::Arc { c: c1, r: r1, .. }, Span::Arc { c: c2, r: r2, .. }) => circle_circle(c1, r1, c2, r2),
    }
}

/// Where the carriers of two spans come nearest when they do not meet: a
/// line that just misses a circle, two circles that just miss each other.
/// The point halfway across the miss, and the miss (mm).
pub(super) fn carriers_touch(s1: &Span, s2: &Span) -> Option<(Point2, f64)> {
    match (*s1, *s2) {
        (Span::Line { a, b }, Span::Arc { c, r, .. }) | (Span::Arc { c, r, .. }, Span::Line { a, b }) => {
            let d = sub(b, a);
            let l2 = dot(d, d);
            if l2.is_nan() || l2 <= 0.0 {
                return None;
            }
            let f = add(a, mul(d, dot(sub(c, a), d) / l2));
            let u = unit(sub(f, c))?;
            let on = add(c, mul(u, r));
            Some((mul(add(f, on), 0.5), dist(f, on)))
        }
        (Span::Arc { c: c1, r: r1, .. }, Span::Arc { c: c2, r: r2, .. }) => {
            let u = unit(sub(c2, c1))?;
            // The nearer pair of points on the line of the centres.
            let mut best: Option<(Point2, f64)> = None;
            for e1 in [1.0, -1.0] {
                for e2 in [1.0, -1.0] {
                    let (x1, x2) = (add(c1, mul(u, r1 * e1)), add(c2, mul(u, r2 * e2)));
                    let g = dist(x1, x2);
                    if best.is_none_or(|(_, b)| g < b) {
                        best = Some((mul(add(x1, x2), 0.5), g));
                    }
                }
            }
            best
        }
        (Span::Line { .. }, Span::Line { .. }) => None,
    }
}

fn seg_seg(a: Point2, b: Point2, c: Point2, d: Point2) -> f64 {
    let (d1, d2) = (cross(sub(d, c), sub(a, c)), cross(sub(d, c), sub(b, c)));
    let (d3, d4) = (cross(sub(b, a), sub(c, a)), cross(sub(b, a), sub(d, a)));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return 0.0;
    }
    [dist(a, foot(a, c, d)), dist(b, foot(b, c, d)), dist(c, foot(c, a, b)), dist(d, foot(d, a, b))]
        .into_iter()
        .fold(f64::INFINITY, f64::min)
}

fn seg_arc(a: Point2, b: Point2, arc: &Span) -> f64 {
    let Span::Arc { c, r, a: p, b: q, .. } = *arc else { return f64::INFINITY };
    let line = Span::Line { a, b };
    let mut best = pt_span(a, arc).min(pt_span(b, arc)).min(pt_span(p, &line)).min(pt_span(q, &line));
    for (t, x) in line_circle(a, b, c, r) {
        if (0.0..=1.0).contains(&t) && arc.holds_dir(x) {
            return 0.0;
        }
    }
    let f = foot(c, a, b);
    if let Some(u) = unit(sub(f, c)) {
        for side in [1.0, -1.0] {
            let x = add(c, mul(u, r * side));
            if arc.holds_dir(x) {
                best = best.min(dist(x, f));
            }
        }
    }
    best
}

fn arc_arc(s1: &Span, s2: &Span) -> f64 {
    let (Span::Arc { c: c1, r: r1, a: a1, b: b1, .. }, Span::Arc { c: c2, r: r2, a: a2, b: b2, .. }) = (*s1, *s2) else {
        return f64::INFINITY;
    };
    let mut best = pt_span(a1, s2).min(pt_span(b1, s2)).min(pt_span(a2, s1)).min(pt_span(b2, s1));
    if circle_circle(c1, r1, c2, r2).iter().any(|x| s1.holds_dir(*x) && s2.holds_dir(*x)) {
        return 0.0;
    }
    match unit(sub(c2, c1)) {
        Some(u) => {
            for e1 in [1.0, -1.0] {
                let x1 = add(c1, mul(u, r1 * e1));
                if !s1.holds_dir(x1) {
                    continue;
                }
                for e2 in [1.0, -1.0] {
                    let x2 = add(c2, mul(u, r2 * e2));
                    if s2.holds_dir(x2) {
                        best = best.min(dist(x1, x2));
                    }
                }
            }
        }
        // Concentric: |r1 - r2| wherever the two sweeps share a direction.
        None => {
            let shared = [a1, b1].iter().any(|p| s2.holds_dir(*p)) || [a2, b2].iter().any(|p| s1.holds_dir(*p));
            if shared {
                best = best.min((r1 - r2).abs());
            }
        }
    }
    best
}

/// The least distance between two spans; 0 where they meet.
pub(super) fn span_span(s1: &Span, s2: &Span) -> f64 {
    match (*s1, *s2) {
        (Span::Line { a, b }, Span::Line { a: c, b: d }) => seg_seg(a, b, c, d),
        (Span::Line { a, b }, arc @ Span::Arc { .. }) | (arc @ Span::Arc { .. }, Span::Line { a, b }) => seg_arc(a, b, &arc),
        (Span::Arc { .. }, Span::Arc { .. }) => arc_arc(s1, s2),
    }
}
