//! The ring ramp (#1558): a planned zig-zag ramp flown as helical arcs
//! about a ring's centre instead of its polygon's chords - only where
//! the planned prefix provably lies on that ring, in the ring's own
//! sense, and every arc spans a chord no posted resolution can collapse.
//! Anything else posts the chords ([`push_zig`]), as before #1558.
//!
//! Provenance: our own law over RS-274 / ISO 6983 circular
//! interpolation (G2/G3 with a Z word is a helix); the full-circle
//! reading of an arc whose endpoints coincide is the standard's own
//! (a G2/G3 whose end equals its start is one whole turn). No vendor
//! material consulted (Clean-Room Protocol).

use std::f64::consts::PI;

use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::push_zig;
use crate::machining2d::ARC_FIT_TOL;

/// The shortest XY chord an arc move may span, mm. The coarsest
/// length a shipped post writes is four-decimal inch (2.54 um per
/// axis); two endpoints whose larger axis difference is at least one
/// unit can never round to the same point, and a chord of `sqrt(2)`
/// units (3.6 um) guarantees that. A control reads an arc whose
/// posted end equals its start as a FULL circle, so anything shorter
/// is posted straight. 10 um is that bound with margin.
pub const MIN_ARC_CHORD: f64 = 0.01;

/// How far a planned prefix vertex may stand off the ring, mm: the
/// 1 um lattice's half-diagonal (0.71 um) plus float noise.
pub const ON_RING_TOL: f64 = 2e-6;

/// The XY chord of a sweep `phi` on a circle of radius `rho`.
#[must_use]
pub fn arc_chord(rho: f64, phi: f64) -> f64 {
    2.0 * rho * (phi * 0.5).sin().abs()
}

/// Whether the planned prefix `pre` lies on the ring about `center`
/// of radius `rho` in the sense `sense` (+1 CCW, -1 CW): every vertex
/// but the last within [`ON_RING_TOL`] of the circle; the last (cut
/// part-way along a chord of the ring's polygon) inside it by at most
/// that polygon's own sagitta ([`ARC_FIT_TOL`]); every chord stays within
/// that sagitta too (plus lattice tolerance); its first step turning
/// in `sense`; and the leg's arc end (`a0 + sense * phi`) landing on the
/// prefix's end within the arc-versus-chord length difference of a
/// polygon held to [`ARC_FIT_TOL`] (under `2 pi / 3` of it, plus the
/// sagitta). A non-circular ladder, or a ring flown the wrong way,
/// fails one of these.
fn on_ring(pre: &[Point2], center: Point2, rho: f64, sense: f64, end: Point2) -> bool {
    let r = |p: &Point2| (p.x - center.x).hypot(p.y - center.y);
    let (Some(first), Some(last)) = (pre.first(), pre.last()) else {
        return false;
    };
    let body = &pre[..pre.len().saturating_sub(1)];
    if !body.iter().all(|p| (r(p) - rho).abs() <= ON_RING_TOL) {
        return false;
    }
    let rl = r(last);
    if !(rl <= rho + ON_RING_TOL && rl >= rho - ARC_FIT_TOL - ON_RING_TOL) {
        return false;
    }
    if !pre.windows(2).all(|w| {
        let dx = w[1].x - w[0].x;
        let dy = w[1].y - w[0].y;
        let len = dx.hypot(dy);
        if len == 0.0 {
            return true;
        }
        // The closest point also covers a final chord cut short of the circle.
        let ux = dx / len;
        let uy = dy / len;
        let along = ((center.x - w[0].x) * ux + (center.y - w[0].y) * uy).clamp(0.0, len);
        let nearest = Point2::new(w[0].x + along * ux, w[0].y + along * uy);
        rho - r(&nearest) <= ARC_FIT_TOL + ON_RING_TOL
    }) {
        return false;
    }
    let Some(next) = pre.get(1) else {
        return false;
    };
    let cross = (first.x - center.x) * (next.y - center.y) - (first.y - center.y) * (next.x - center.x);
    let turn = cross * sense;
    if turn.is_nan() || turn <= 0.0 {
        return false;
    }
    (end.x - last.x).hypot(end.y - last.y) <= 4.0 * ARC_FIT_TOL
}

/// [`push_zig`] on a ring (#1558): the same `n` out-and-back legs over
/// the same travelled length (the chord prefix `pre`'s own, so the ramp
/// angle is unchanged), flown as helical arcs about `center` - out in
/// the ring's own sense `ccw`, back against it - instead of the
/// polygon's chords. Each leg is split into half turns at most (the
/// helix entry's own pieces). The chords are posted instead - never a
/// guessed arc - when the ring is degenerate, when `pre` is not on the
/// ring in that sense ([`on_ring`]: a non-circular ladder can never be
/// flown as arcs, whatever its flag), or when a piece's chord is under
/// [`MIN_ARC_CHORD`] (a micro-ramp on a tiny last level, which a post
/// could round to a full circle).
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_zig_ring(
    b: &mut PathBuilder,
    pre: &[Point2],
    center: Point2,
    ccw: bool,
    n: usize,
    z_from: f64,
    z_to: f64,
    feed: f64,
) {
    let lap: f64 = pre.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
    let total = 2.0 * lap * n as f64;
    let Some(&e) = pre.first() else {
        return;
    };
    let rho = (e.x - center.x).hypot(e.y - center.y);
    let phi = lap / rho;
    if !(total.is_finite() && total > 0.0 && rho.is_finite() && rho > 1e-9 && phi.is_finite() && phi < 2.0 * PI) {
        push_zig(b, pre, n, z_from, z_to, feed);
        return;
    }
    let pieces = ((phi / PI).ceil() as usize).clamp(1, 2);
    let step = lap / pieces as f64;
    let a0 = (e.y - center.y).atan2(e.x - center.x);
    let sense = if ccw { 1.0 } else { -1.0 };
    let at_angle = |ang: f64| Point2::new(center.x + rho * ang.cos(), center.y + rho * ang.sin());
    let end = at_angle(a0 + sense * phi);
    if !on_ring(pre, center, rho, sense, end) || arc_chord(rho, phi / pieces as f64) < MIN_ARC_CHORD {
        push_zig(b, pre, n, z_from, z_to, feed);
        return;
    }
    let mut s = 0.0;
    for k in 0..n {
        for out in [true, false] {
            for j in 1..=pieces {
                s += step;
                let at = if out { j } else { pieces - j };
                let last = k + 1 == n && !out && j == pieces;
                let q = if at == 0 {
                    e
                } else {
                    let p = at_angle(a0 + sense * phi * at as f64 / pieces as f64);
                    Point2::new(lattice(p.x), lattice(p.y))
                };
                let z = if last { z_to } else { z_from - (z_from - z_to) * (s / total) };
                b.push_arc(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed), center, if out { ccw } else { !ccw });
            }
        }
    }
}
