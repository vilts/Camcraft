//! A vertical wire's arc moves as the wire laws read them (#3937): each
//! recorded arc - about its lattice centre, its radius running from the
//! start's to the end's as the CL arc law states - cut into chords, and
//! each chord paired with the most the arc stands off it, so a reading
//! on the chord is taken down by that much and never passes an arc whose
//! kerf enters the product.
//!
//! Provenance: plane geometry (the sagitta of a chord, r - sqrt(r^2 -
//! (l/2)^2), widened by the radius change of a spiral). Our own code.

use crate::cl::Move;
use crate::geo2d::Point2;

/// The chord sagitta (mm) the laws cut an arc to: 25 nm. With the same
/// sag on the product's waste-side ring the two stay well inside the
/// kerf law's own tolerance (`KERF_TOL`, 0.1 um).
pub const ARC_SAG: f64 = 2.5e-5;

/// The most chords one arc move is cut into.
const MAX_CHORDS: f64 = 65_536.0;

/// The chord ends of the XY arc `m` after its start (its end last) and,
/// per chord, the most the arc stands off it. `None` for an arc that is
/// not a readable XY circle.
pub(super) fn chords(m: &Move) -> Option<Vec<(Point2, f64)>> {
    let arc = m.arc?;
    if !arc.plane.is_xy() {
        return None;
    }
    let c = arc.center;
    let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
    let (ra, rb) = ((a.x - c.x).hypot(a.y - c.y), (b.x - c.x).hypot(b.y - c.y));
    if !(ra.is_finite() && rb.is_finite() && ra > 0.0 && rb > 0.0) {
        return None;
    }
    let a0 = (a.y - c.y).atan2(a.x - c.x);
    let a1 = (b.y - c.y).atan2(b.x - c.x);
    let d = if arc.ccw { a1 - a0 } else { a0 - a1 }.rem_euclid(std::f64::consts::TAU);
    let d = if d > 0.0 { d } else { std::f64::consts::TAU };
    let r = ra.max(rb);
    let step = 2.0 * (1.0 - ARC_SAG / r).clamp(-1.0, 1.0).acos();
    let n = (d / step).ceil();
    if !(n.is_finite() && n <= MAX_CHORDS) {
        return None;
    }
    let n = (n as usize).max(1);
    let spiral = (rb - ra).abs();
    let sign = if arc.ccw { 1.0 } else { -1.0 };
    let mut out = Vec::with_capacity(n);
    let mut prev = a;
    for j in 1..=n {
        let f = j as f64 / n as f64;
        let q = if j == n {
            b
        } else {
            let t = a0 + sign * d * f;
            let rr = ra + (rb - ra) * f;
            Point2::new(c.x + rr * t.cos(), c.y + rr * t.sin())
        };
        let half = 0.5 * (q.x - prev.x).hypot(q.y - prev.y);
        let sag = r - (r * r - half * half).max(0.0).sqrt() + spiral;
        out.push((q, sag));
        prev = q;
    }
    Some(out)
}
