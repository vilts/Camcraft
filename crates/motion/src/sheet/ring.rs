//! Ring helpers for the sheet cuts (#670, #1883): the kerf offset of one
//! contour and the polyline walks the emitter rides - nearest point,
//! a ring re-started at a point, a stretch along it.
//!
//! Provenance: the offset is the textbook polygon buffer (clipper2 via
//! `geo2d::Region::offset`); the walks are plain polyline geometry.
//! Our own code.

use crate::cl::ContourRole;
use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::GenError;

use super::nest::Contour;
use super::Voice;

pub fn dist(a: Point2, b: Point2) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

/// The torch-centre ring of a closed contour: its outline grown by `r`
/// (an outer edge - the waste is outside) or its hole shrunk by `r`
/// (a hole - the waste is inside), counter-clockwise. A hole the kerf
/// cannot enter, or a contour whose offset splits or closes a gap, is
/// refused by `label` - never an offset that would cut the part.
pub fn kerf_ring(c: &Contour, r: f64, label: &str, v: Voice) -> Result<Vec<Point2>, GenError> {
    let ring = Region::new(vec![Loop::new(c.pts.clone())?])?;
    let inner = c.role == ContourRole::Inner;
    let off = ring.offset(if inner { -r } else { r })?;
    let eq_d = 2.0 * (c.area / std::f64::consts::PI).sqrt();
    match off.loops() {
        [one] => Ok(one.points().to_vec()),
        [] if inner => Err(v.invalid(format!(
            "{label} is a hole of about Ø{eq_d:.3} mm - smaller than the kerf ({:.3} mm); the {} cannot cut it without cutting the part",
            2.0 * r,
            v.tool
        ))),
        _ if inner => Err(v.invalid(format!(
            "{label} is a hole that narrows to a gap under the kerf ({:.3} mm) - the {} cannot pass it without cutting the part",
            2.0 * r,
            v.tool
        ))),
        _ => Err(v.invalid(format!(
            "{label} has a gap or slot narrower than the kerf ({:.3} mm) - the {} cannot form it; widen it or use a narrower kerf",
            2.0 * r,
            v.tool
        ))),
    }
}

/// The nearest point of the closed ring to `p`: (edge index, foot).
pub fn nearest(ring: &[Point2], p: Point2) -> (usize, Point2) {
    let n = ring.len();
    let mut best = (0usize, ring.first().copied().unwrap_or(p), f64::INFINITY);
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
        let f = Point2::new(a.x + t * dx, a.y + t * dy);
        let d = dist(f, p);
        if d < best.2 {
            best = (i, f, d);
        }
    }
    (best.0, best.1)
}

/// The middle of the ring's longest edge: (edge index, point) - the
/// start a lead meets on a straight, never on a corner.
pub fn longest_mid(ring: &[Point2]) -> (usize, Point2) {
    let n = ring.len();
    let mut best = (0usize, -1.0f64);
    for i in 0..n {
        let l = dist(ring[i], ring[(i + 1) % n]);
        if l > best.1 {
            best = (i, l);
        }
    }
    let (a, b) = (ring[best.0], ring[(best.0 + 1) % n]);
    (best.0, Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5))
}

/// The closed walk of `ring` from `s` on edge `seg` (s .. back to s),
/// travelling counter-clockwise when `ccw` (the ring is CCW), else
/// clockwise. Coincident neighbours are dropped.
pub fn walk_from(ring: &[Point2], seg: usize, s: Point2, ccw: bool) -> Vec<Point2> {
    let n = ring.len();
    let mut out = vec![s];
    let push = |p: Point2, out: &mut Vec<Point2>| {
        if out.last().is_none_or(|q| dist(*q, p) > 1e-9) {
            out.push(p);
        }
    };
    if ccw {
        for k in 1..=n {
            push(ring[(seg + k) % n], &mut out);
        }
    } else {
        for k in 0..n {
            push(ring[(seg + n - k) % n], &mut out);
        }
    }
    push(s, &mut out);
    out
}

/// The first `len` mm along `walk` (from its start), as points after
/// the start; capped at the walk's own length.
pub fn along(walk: &[Point2], len: f64) -> Vec<Point2> {
    let mut out = Vec::new();
    let mut left = len;
    for w in walk.windows(2) {
        if left <= 1e-9 {
            break;
        }
        let l = dist(w[0], w[1]);
        if l <= left {
            out.push(w[1]);
            left -= l;
        } else {
            let t = left / l;
            out.push(Point2::new(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t));
            left = 0.0;
        }
    }
    out
}

/// The unit direction a -> b, or +X for coincident points.
pub fn unit(a: Point2, b: Point2) -> Point2 {
    let l = dist(a, b);
    if l > 1e-12 && l.is_finite() {
        Point2::new((b.x - a.x) / l, (b.y - a.y) / l)
    } else {
        Point2::new(1.0, 0.0)
    }
}

/// The area centroid of a ring (its vertex mean when it has no area).
pub fn centroid(ring: &[Point2]) -> Point2 {
    let n = ring.len();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (ring[i], ring[(i + 1) % n]);
        let c = p.x * q.y - q.x * p.y;
        a += c;
        cx += (p.x + q.x) * c;
        cy += (p.y + q.y) * c;
    }
    if a.abs() > 1e-12 {
        Point2::new(cx / (3.0 * a), cy / (3.0 * a))
    } else {
        let m = n.max(1) as f64;
        Point2::new(ring.iter().map(|p| p.x).sum::<f64>() / m, ring.iter().map(|p| p.y).sum::<f64>() / m)
    }
}
