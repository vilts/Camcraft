//! The XY trace of an XZ / YZ (G18 / G19) arc for law (b)'s whole-move
//! wall reading (#946), the vertical-plane twin of [`super::arc_wall`]'s
//! XY circle. Specced from elementary plane geometry (the extremes of
//! `c + r cos`/`c + r sin` over an angle interval) and the linear
//! interpolation error bound `|f(s) - lerp(s)| <= max|f''| / 8` (any
//! numerical-analysis text); no vendor material.
//!
//! An arc in XZ or YZ turns in a vertical plane, so its XY footprint is
//! not a circle:
//! - **Planar** (the normal coordinate - Y for G18, X for G19 - is the
//!   same at both ends): the footprint is ONE straight segment along X
//!   (G18) or Y (G19), from the least to the greatest value the in-plane
//!   coordinate takes over the sweep. Those are the endpoints' own
//!   values, or `c +- r` where the sweep passes the angle that turns the
//!   coordinate back. It is read as that segment, exactly - the joint
//!   reading (#839) saw only points of it, up to a chord length apart.
//!   An interior extreme is read at the radius band's outer edge
//!   ([`ARC_PAD`] past the larger endpoint radius: centres sit on the
//!   1 um lattice), so the segment never reads shorter than the cut.
//! - **Helical** (the normal coordinate changes, linearly in the sweep):
//!   the footprint is a curve. It is read as a polyline of at most
//!   [`MAX_ARC_CHORDS`] pieces through points of the arc, each piece
//!   padded by how far the true footprint can stand off it: the
//!   in-plane path has `|f''| = r d^2` per piece of sweep `d` and the
//!   normal coordinate none, so the interpolation bound gives
//!   `r d^2 / 8`, plus the radius band's width. Read against
//!   `floor + pad`, the polyline never reads farther than the truth.

use super::arc_bulge::ARC_PAD;
use super::arc_wall::Trace;
use crate::cl::{ArcMove, ArcPlane, Move, MAX_ARC_CHORDS};
use crate::geo2d::Point2;
use std::f64::consts::{FRAC_PI_2, PI};

/// The sagitta a helical footprint's polyline is cut to, mm: fine
/// enough that the pad it adds is far under any verdict tolerance,
/// coarse enough that a working arc needs few pieces (MAX_ARC_CHORDS
/// caps the rest; the pad then grows to what the cap leaves).
const HELIX_SAG: f64 = 1e-4;

/// The trace of a vertical-plane arc `arc` on `m` (the caller has
/// checked the plane is not XY).
pub(super) fn trace(m: &Move, arc: ArcMove) -> Trace {
    let pl = arc.plane;
    let (cu, cv) = pl.center_uv(arc.center);
    let (u0, v0, w0) = pl.uvw(m.from);
    let (u1, v1, w1) = pl.uvw(m.to);
    let (ra, rb) = ((u0 - cu).hypot(v0 - cv), (u1 - cu).hypot(v1 - cv));
    if !(cu.is_finite() && cv.is_finite() && ra.is_finite() && rb.is_finite() && w0.is_finite() && w1.is_finite()) {
        return Trace::Unread;
    }
    let from = Point2::new(m.from.x, m.from.y);
    let to = Point2::new(m.to.x, m.to.y);
    let a0 = (v0 - cv).atan2(u0 - cu);
    let a1 = (v1 - cv).atan2(u1 - cu);
    let d = if arc.ccw { a1 - a0 } else { a0 - a1 }.rem_euclid(2.0 * PI);
    // The chord law's degeneracies (`cl::arc_chords`): it cuts these as
    // the straight move, so that is what is read.
    if ra.max(rb) <= f64::EPSILON || !d.is_finite() || d <= 0.0 {
        return Trace::Seg(from, to);
    }
    let r_lo = (ra.min(rb) - ARC_PAD).max(0.0);
    let r_hi = ra.max(rb) + ARC_PAD;
    // The sweep in ccw order: from `s` through `d` radians.
    let s = if arc.ccw { a0 } else { a1 };
    if w0 == w1 {
        planar(matches!(pl, ArcPlane::Xz), (cu, cv), (u0, v0, u1, v1), w0, s, d, r_hi)
    } else {
        helical(m, arc, (cu, cv), (a0, d), (w0, w1), (r_lo, r_hi))
    }
}

/// Whether the angle `t` lies in the ccw sweep `[s, s + d]`.
fn in_sweep(t: f64, s: f64, d: f64) -> bool {
    (t - s).rem_euclid(2.0 * PI) <= d
}

/// The planar footprint: one segment along the in-plane coordinate that
/// is horizontal - `v` (X) for XZ, `u` (Y) for YZ - at the normal
/// coordinate `w`.
fn planar(xz: bool, (cu, cv): (f64, f64), (u0, v0, u1, v1): (f64, f64, f64, f64), w: f64, s: f64, d: f64, r_hi: f64) -> Trace {
    // XZ: (u, v, w) = (z, x, y), the horizontal is v = cv + r sin;
    // YZ: (u, v, w) = (y, z, x), the horizontal is u = cu + r cos.
    let (c, e0, e1, hi_at, lo_at) = if xz { (cv, v0, v1, FRAC_PI_2, -FRAC_PI_2) } else { (cu, u0, u1, 0.0, PI) };
    let mut lo = e0.min(e1);
    let mut hi = e0.max(e1);
    if in_sweep(hi_at, s, d) {
        hi = hi.max(c + r_hi);
    }
    if in_sweep(lo_at, s, d) {
        lo = lo.min(c - r_hi);
    }
    if xz {
        Trace::Seg(Point2::new(lo, w), Point2::new(hi, w))
    } else {
        Trace::Seg(Point2::new(w, lo), Point2::new(w, hi))
    }
}

/// The helical footprint: a padded polyline (module doc).
fn helical(m: &Move, arc: ArcMove, (cu, cv): (f64, f64), (a0, d): (f64, f64), (w0, w1): (f64, f64), (r_lo, r_hi): (f64, f64)) -> Trace {
    let pl = arc.plane;
    let r = r_hi - ARC_PAD; // the chord law's radius, max(ra, rb)
    // Sweep per piece for a sagitta of HELIX_SAG: r d^2 / 8 <= sag.
    let step = (8.0 * HELIX_SAG / r_hi).sqrt();
    let want = (d / step).ceil();
    let n = if want.is_finite() && want >= 1.0 { (want as usize).min(MAX_ARC_CHORDS) } else { 1 };
    let piece = d / n as f64;
    let pad = r_hi * piece * piece / 8.0 + (r_hi - r_lo);
    let at = |j: usize| {
        if j == 0 {
            return Point2::new(m.from.x, m.from.y);
        }
        if j == n {
            return Point2::new(m.to.x, m.to.y);
        }
        let f = j as f64 / n as f64;
        let th = a0 + if arc.ccw { d * f } else { -d * f };
        let p = pl.point(cu + r * th.cos(), cv + r * th.sin(), w0 + (w1 - w0) * f);
        Point2::new(p.x, p.y)
    };
    let pts: Vec<Point2> = (0..=n).map(at).collect();
    Trace::Poly(pts, pad)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::MoveKind;
    use crate::geometry::Point3;
    use crate::verify::laws::dist2_segs;

    /// A vertical arc of radius `r` in `plane` about word-order centre
    /// `c`, from angle `t0` to `t1` (uv frame), the normal coordinate
    /// running `w0` to `w1`.
    fn varc(plane: ArcPlane, c: (f64, f64), r: f64, (t0, t1): (f64, f64), (w0, w1): (f64, f64), ccw: bool) -> Move {
        let cw = Point2::new(c.0, c.1);
        let (cu, cv) = plane.center_uv(cw);
        let at = |t: f64, w: f64| plane.point(cu + r * t.cos(), cv + r * t.sin(), w);
        Move::new(MoveKind::Feed, at(t0, w0), at(t1, w1), Some(100.0)).with_arc_in(cw, ccw, plane)
    }

    /// Dense samples of the arc's XY footprint.
    fn dense(m: &Move) -> Vec<Point2> {
        let Some(arc) = m.arc else { panic!("an arc") };
        (0..=20_000)
            .filter_map(|j| arc.point_at(m.from, m.to, j as f64 / 20_000.0))
            .map(|p| Point2::new(p.x, p.y))
            .collect()
    }

    fn dist(t: &Trace, p: Point2, q: Point2) -> f64 {
        // Bisect the floor at which `near` flips: the trace's distance
        // as the law sees it (the pad included).
        let (mut lo, mut hi) = (0.0, 100.0);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if t.near(p, q, mid) { hi = mid } else { lo = mid }
        }
        hi
    }

    /// The reading is never farther than the true footprint (dense
    /// sampling) and within a few microns of it, for planar and helical
    /// arcs in both planes, both ways, over and under a half turn.
    #[test]
    fn the_vertical_footprint_never_reads_farther_than_the_truth() {
        let mut seed = 0x2545_f491_u64;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 1_000_000) as f64 / 1_000_000.0
        };
        for plane in [ArcPlane::Xz, ArcPlane::Yz] {
            for (ts, ccw) in [((0.3, 1.4), true), ((2.0, 0.1), false), ((-1.0, 3.5), true), ((4.0, 0.5), true)] {
                for ws in [(2.0, 2.0), (2.0, 5.0)] {
                    let m = varc(plane, (3.0, -2.0), 10.0, ts, ws, ccw);
                    let t = Trace::of(&m);
                    assert!(!matches!(t, Trace::Unread), "a read trace");
                    let pts = dense(&m);
                    for _ in 0..100 {
                        let p = Point2::new(-25.0 + 50.0 * rnd(), -25.0 + 50.0 * rnd());
                        let q = Point2::new(-25.0 + 50.0 * rnd(), -25.0 + 50.0 * rnd());
                        let read = dist(&t, p, q);
                        let truth = pts.iter().map(|&x| dist2_segs(x, x, p, q).sqrt()).fold(f64::INFINITY, f64::min);
                        assert!(read <= truth + 1e-9, "{plane:?} {ws:?}: read {read} farther than truth {truth}");
                        assert!(truth - read < 0.01, "{plane:?} {ws:?}: read {read} vs truth {truth}");
                    }
                }
            }
        }
    }

    /// A planar arc that passes the coordinate's turn reads out to it,
    /// past both endpoints - the stretch the endpoints' segment misses.
    #[test]
    fn a_planar_arc_reads_out_to_its_turn() {
        // XZ, (u, v) = (z, x): pi/2 is x's maximum, c.x + r.
        let m = varc(ArcPlane::Xz, (0.0, 0.0), 10.0, (1.0, 2.0), (3.0, 3.0), true);
        let Trace::Seg(a, b) = Trace::of(&m) else { panic!("a segment") };
        assert!((b.x - 10.0).abs() <= 2.0 * ARC_PAD && b.x >= 10.0, "{b:?}");
        assert!(a.y == 3.0 && b.y == 3.0);
    }

    /// Degenerate arcs are read without a hang or a panic: zero radius
    /// and zero sweep as the straight move, a NaN centre fails closed,
    /// a huge helix is bounded by the chord cap.
    #[test]
    fn degenerate_vertical_arcs_are_read_safely() {
        let p = Point3::new(1.0, 2.0, 3.0);
        let zero = Move::new(MoveKind::Feed, p, p, Some(100.0)).with_arc_in(Point2::new(1.0, 3.0), true, ArcPlane::Xz);
        assert!(matches!(Trace::of(&zero), Trace::Seg(..)));
        let mut nan = zero;
        nan.arc = Some(ArcMove::in_plane(Point2::new(f64::NAN, 0.0), true, ArcPlane::Yz));
        assert!(matches!(Trace::of(&nan), Trace::Unread));
        let huge = varc(ArcPlane::Yz, (0.0, 0.0), 1e9, (0.0, 3.0), (0.0, 1.0), true);
        let Trace::Poly(pts, pad) = Trace::of(&huge) else { panic!("a polyline") };
        assert!(pts.len() <= MAX_ARC_CHORDS + 1 && pad.is_finite());
    }
}
