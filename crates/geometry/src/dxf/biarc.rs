//! The biarc between two points with given unit tangents, and the exact closest point on it.
//! Provenance: our own implementation of the equal-tangent-length biarc (K. M. Bolton,
//! "Biarc curves", Computer-Aided Design 7(2), 1975) and plane geometry of the circle.
use super::types::Primitive;
use crate::geo2d::Point2;
use std::f64::consts::TAU;

/// One output segment, as the DXF reader emits it.
#[derive(Clone, Copy, Debug)]
pub(super) enum Seg {
    Line {
        a: Point2,
        b: Point2,
    },
    /// `sweep` is signed: counter-clockwise positive.
    Arc {
        c: Point2,
        r: f64,
        a0: f64,
        sweep: f64,
    },
}

impl Seg {
    pub fn length(&self) -> f64 {
        match *self {
            Seg::Line { a, b } => (b.x - a.x).hypot(b.y - a.y),
            Seg::Arc { r, sweep, .. } => r * sweep.abs(),
        }
    }

    pub fn primitive(self) -> Primitive {
        match self {
            Seg::Line { a, b } => Primitive::Line { start: a, end: b },
            Seg::Arc { c, r, a0, sweep } => Primitive::Arc {
                center: c,
                radius: r,
                start_angle: a0,
                sweep_angle: sweep,
            },
        }
    }

    /// The distance from `q` and the arc length from this segment's start to its closest point.
    fn closest(&self, q: Point2) -> (f64, f64) {
        match *self {
            Seg::Line { a, b } => {
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let len2 = dx * dx + dy * dy;
                let t = (((q.x - a.x) * dx + (q.y - a.y) * dy) / len2).clamp(0.0, 1.0);
                let (x, y) = (a.x + dx * t, a.y + dy * t);
                ((q.x - x).hypot(q.y - y), t * len2.sqrt())
            }
            Seg::Arc { c, r, a0, sweep } => {
                let phi = (q.y - c.y).atan2(q.x - c.x);
                let turned = if sweep >= 0.0 {
                    (phi - a0).rem_euclid(TAU)
                } else {
                    (a0 - phi).rem_euclid(TAU)
                };
                if turned <= sweep.abs() {
                    let rho = (q.x - c.x).hypot(q.y - c.y);
                    return ((rho - r).abs(), r * turned);
                }
                let at = |angle: f64| Point2::new(c.x + r * angle.cos(), c.y + r * angle.sin());
                let (s, e) = (at(a0), at(a0 + sweep));
                let ds = (q.x - s.x).hypot(q.y - s.y);
                let de = (q.x - e.x).hypot(q.y - e.y);
                if ds <= de {
                    (ds, 0.0)
                } else {
                    (de, self.length())
                }
            }
        }
    }
}

/// The distance from `q` to the chain `segs` and the arc length to its closest point.
pub(super) fn closest(segs: &[Seg], q: Point2) -> (f64, f64) {
    let mut best = (f64::INFINITY, 0.0);
    let mut offset = 0.0;
    for seg in segs {
        let (d, s) = seg.closest(q);
        if d < best.0 {
            best = (d, offset + s);
        }
        offset += seg.length();
    }
    best
}

/// The arc from `p` with unit tangent `t` to `q`; a line when its radius passes `max_radius`.
fn arc(p: Point2, t: (f64, f64), q: Point2, max_radius: f64) -> Option<Seg> {
    let (vx, vy) = (q.x - p.x, q.y - p.y);
    let len2 = vx * vx + vy * vy;
    if len2.is_nan() || len2 <= 0.0 {
        return None;
    }
    // The left normal n; the centre is p + n s with |centre - q| = |s|.
    let (nx, ny) = (-t.1, t.0);
    let nv = nx * vx + ny * vy;
    if nv == 0.0 || len2 / (2.0 * nv.abs()) > max_radius {
        return Some(Seg::Line { a: p, b: q });
    }
    let s = len2 / (2.0 * nv);
    let c = Point2::new(p.x + nx * s, p.y + ny * s);
    let sweep = 2.0 * nv.atan2(t.0 * vx + t.1 * vy);
    let seg = Seg::Arc {
        c,
        r: s.abs(),
        a0: (p.y - c.y).atan2(p.x - c.x),
        sweep,
    };
    (seg.length().is_finite() && sweep != 0.0).then_some(seg)
}

/// The biarc from `p0` (tangent `t0`) to `p1` (tangent `t1`) with equal tangent lengths.
/// Two lines that lie on one line become one line. `None` when no such biarc exists.
pub(super) fn biarc(
    p0: Point2,
    t0: (f64, f64),
    p1: Point2,
    t1: (f64, f64),
    max_radius: f64,
) -> Option<Vec<Seg>> {
    let (vx, vy) = (p1.x - p0.x, p1.y - p0.y);
    let vv = vx * vx + vy * vy;
    let (tx, ty) = (t0.0 + t1.0, t0.1 + t1.1);
    let vt = vx * tx + vy * ty;
    let denom = 2.0 * (1.0 - (t0.0 * t1.0 + t0.1 * t1.1));
    let d = if denom.abs() < 1e-12 {
        let v1 = vx * t1.0 + vy * t1.1;
        if v1.is_nan() || v1 <= 0.0 {
            return None;
        }
        vv / (4.0 * v1)
    } else {
        (-vt + (vt * vt + denom * vv).sqrt()) / denom
    };
    if !(d > 0.0 && d.is_finite()) {
        return None;
    }
    let m1 = (p0.x + t0.0 * d, p0.y + t0.1 * d);
    let m2 = (p1.x - t1.0 * d, p1.y - t1.1 * d);
    let joint = Point2::new(0.5 * (m1.0 + m2.0), 0.5 * (m1.1 + m2.1));
    let (jx, jy) = (m2.0 - m1.0, m2.1 - m1.1);
    let jl = jx.hypot(jy);
    if !(jl > 0.0 && jl.is_finite()) {
        return None;
    }
    let first = arc(p0, t0, joint, max_radius)?;
    let second = arc(joint, (jx / jl, jy / jl), p1, max_radius)?;
    if matches!(first, Seg::Line { .. }) && matches!(second, Seg::Line { .. }) {
        return Some(vec![Seg::Line { a: p0, b: p1 }]);
    }
    Some(vec![first, second])
}
