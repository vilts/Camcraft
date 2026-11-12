//! The XY trace of one move for law (b)'s whole-move wall reading
//! (#905): a straight move's segment, or an XY arc's true circle, with
//! the exact least distance from that trace to a wall edge (an XZ / YZ
//! arc's footprint is [`super::arc_wall_plane`]'s, #946). Specced from
//! elementary plane geometry (point-circle distance along the radius,
//! line-circle intersection); no sampling.
//!
//! Distance from a segment `pq` to an arc. For a point `P` inside the
//! arc's wedge (its angle within the sweep) the nearest arc point lies
//! on the radius through `P`, at `||CP| - r|`. For a point outside the
//! wedge it is an arc endpoint. So the least distance is the least of:
//! - each arc endpoint to the segment;
//! - over every piece of the segment inside the wedge, the radial
//!   distance. `|CP|` is convex along a segment, so on a piece it takes
//!   every value between its least (the foot of the perpendicular from
//!   `C`, clamped to the piece) and its greatest (an end of the piece),
//!   and the radial distance is the gap between that range and the
//!   circle's radius band.
//!
//! The radius band is `[min(|Ca|, |Cb|), max(|Ca|, |Cb|)]` widened by
//! [`super::arc_bulge::ARC_PAD`]: centres sit on the 1 um lattice while
//! endpoints are exact, so the circle the controller cuts can stand that
//! far off either endpoint radius. Reading the band, never one radius,
//! keeps the distance from ever reading farther than the truth.

use super::arc_bulge::ARC_PAD;
use super::laws::{dist2_segs, orient, proper_cross};
use crate::cl::Move;
use crate::geo2d::Point2;
use std::f64::consts::PI;
use std::ops::Not;

/// What law (b)'s wall reading measures of one move in XY.
pub(super) enum Trace {
    /// A straight move, or an arc the chord law reads as its straight
    /// chord (zero radius, zero sweep).
    Seg(Point2, Point2),
    /// An XY arc (a helix included: its XY trace is the arc).
    Arc(XyArc),
    /// A helical XZ / YZ arc's footprint: a polyline through points of
    /// it, every piece read `pad` nearer than it stands (#946,
    /// [`super::arc_wall_plane`]).
    Poly(Vec<Point2>, f64),
    /// An arc whose centre or radius is not a number: the law fails
    /// closed on it.
    Unread,
}

/// One XY arc as the machine cuts it.
pub(super) struct XyArc {
    c: Point2,
    /// The radius band (see the module doc).
    r_lo: f64,
    r_hi: f64,
    /// Endpoints in counter-clockwise order: the arc runs ccw from `a`
    /// to `b` through `sweep` radians (a cw arc is read the other way).
    a: Point2,
    b: Point2,
    a0: f64,
    sweep: f64,
}

impl Trace {
    /// The trace of `m`. An arc in XZ or YZ has a footprint that is not
    /// a circle: [`super::arc_wall_plane`] reads it (#946).
    pub(super) fn of(m: &Move) -> Trace {
        let from = Point2::new(m.from.x, m.from.y);
        let to = Point2::new(m.to.x, m.to.y);
        let Some(arc) = m.arc else { return Trace::Seg(from, to) };
        if !arc.plane.is_xy() {
            return super::arc_wall_plane::trace(m, arc);
        }
        let c = arc.center;
        let (ra, rb) = ((from.x - c.x).hypot(from.y - c.y), (to.x - c.x).hypot(to.y - c.y));
        if !(c.x.is_finite() && c.y.is_finite() && ra.is_finite() && rb.is_finite()) {
            return Trace::Unread;
        }
        let (a, b) = if arc.ccw { (from, to) } else { (to, from) };
        let a0 = (a.y - c.y).atan2(a.x - c.x);
        let a1 = (b.y - c.y).atan2(b.x - c.x);
        let sweep = (a1 - a0).rem_euclid(2.0 * PI);
        // The chord law's own degeneracies (`cl::arc_chords`): it cuts
        // these as the straight chord, so that is what is read.
        if ra.max(rb) <= f64::EPSILON || !sweep.is_finite() || sweep <= 0.0 {
            return Trace::Seg(from, to);
        }
        Trace::Arc(XyArc {
            c,
            r_lo: (ra.min(rb) - ARC_PAD).max(0.0),
            r_hi: ra.max(rb) + ARC_PAD,
            a,
            b,
            a0,
            sweep,
        })
    }

    /// Whether the trace comes nearer than `floor` to the edge `p`-`q`.
    /// A straight trace keeps the law's own squared comparison, float for
    /// float.
    pub(super) fn near(&self, p: Point2, q: Point2, floor: f64) -> bool {
        match self {
            Trace::Seg(a, b) => dist2_segs(*a, *b, p, q) < floor * floor,
            // A NaN distance reads near: the law fails closed.
            Trace::Arc(arc) => arc.dist_seg(p, q).ge(&floor).not(),
            Trace::Poly(pts, pad) => {
                let f = floor + pad;
                pts.windows(2).any(|w| dist2_segs(w[0], w[1], p, q).ge(&(f * f)).not())
            }
            Trace::Unread => true,
        }
    }

    /// Whether the trace properly crosses the edge `p`-`q` (passes from
    /// one side to the other through its interior, not touching).
    pub(super) fn crosses(&self, p: Point2, q: Point2) -> bool {
        match self {
            Trace::Seg(a, b) => proper_cross(*a, *b, p, q),
            Trace::Arc(arc) => arc.crosses(p, q),
            Trace::Poly(pts, _) => pts.windows(2).any(|w| proper_cross(w[0], w[1], p, q)),
            Trace::Unread => false,
        }
    }
}

impl XyArc {
    /// Whether the direction of `p` from the centre lies within the
    /// sweep (ends included).
    fn in_wedge(&self, p: Point2) -> bool {
        let th = (p.y - self.c.y).atan2(p.x - self.c.x);
        (th - self.a0).rem_euclid(2.0 * PI) <= self.sweep
    }

    fn rho(&self, p: Point2) -> f64 {
        (p.x - self.c.x).hypot(p.y - self.c.y)
    }

    /// Least distance from the segment `p`-`q` to the arc (module doc).
    pub(super) fn dist_seg(&self, p: Point2, q: Point2) -> f64 {
        let mut best = dist2_segs(self.a, self.a, p, q).min(dist2_segs(self.b, self.b, p, q)).sqrt();
        let at = |t: f64| Point2::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t);
        // Where the segment crosses the lines through the centre and each
        // arc end: between two such breaks it is wholly in or out of the
        // wedge.
        // A fixed buffer, no allocation per edge: an unused break repeats
        // 1.0, a zero-length piece that is never in the wedge alone.
        let mut ts = [0.0, 1.0, 1.0, 1.0];
        for (k, e) in [self.a, self.b].into_iter().enumerate() {
            let (o0, o1) = (orient(self.c, e, p), orient(self.c, e, q));
            if (o0 > 0.0) != (o1 > 0.0) && o0 != o1 {
                ts[2 + k] = (o0 / (o0 - o1)).clamp(0.0, 1.0);
            }
        }
        ts.sort_by(f64::total_cmp);
        let (dx, dy) = (q.x - p.x, q.y - p.y);
        let dd = dx * dx + dy * dy;
        let foot = if dd > f64::EPSILON {
            ((self.c.x - p.x) * dx + (self.c.y - p.y) * dy) / dd
        } else {
            0.0
        };
        for w in ts.windows(2) {
            let (t0, t1) = (w[0], w[1]);
            if !self.in_wedge(at(0.5 * (t0 + t1))) && !(t0 == t1 && self.in_wedge(at(t0))) {
                continue;
            }
            let lo = self.rho(at(foot.clamp(t0, t1)));
            let hi = self.rho(at(t0)).max(self.rho(at(t1)));
            let gap = if hi < self.r_lo {
                self.r_lo - hi
            } else if lo > self.r_hi {
                lo - self.r_hi
            } else {
                0.0
            };
            best = best.min(gap);
        }
        best
    }

    /// A transversal crossing of the segment's interior with the arc's
    /// interior, on the circle through the band's middle.
    fn crosses(&self, p: Point2, q: Point2) -> bool {
        let r = 0.5 * (self.r_lo + self.r_hi);
        let (dx, dy) = (q.x - p.x, q.y - p.y);
        let (fx, fy) = (p.x - self.c.x, p.y - self.c.y);
        let qa = dx * dx + dy * dy;
        if qa <= f64::EPSILON {
            return false;
        }
        let qb = 2.0 * (fx * dx + fy * dy);
        let qc = fx * fx + fy * fy - r * r;
        let disc = qb * qb - 4.0 * qa * qc;
        if disc.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return false; // a miss or a tangent touch is no crossing
        }
        let s = disc.sqrt();
        [(-qb - s) / (2.0 * qa), (-qb + s) / (2.0 * qa)].into_iter().any(|t| {
            let x = Point2::new(p.x + dx * t, p.y + dy * t);
            t > 0.0 && t < 1.0 && self.in_wedge(x) && {
                let th = (x.y - self.c.y).atan2(x.x - self.c.x);
                let off = (th - self.a0).rem_euclid(2.0 * PI);
                off > 0.0 && off < self.sweep
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::MoveKind;
    use crate::geometry::Point3;

    /// The exact distance agrees with a dense sampling of the arc and
    /// the segment (never farther than the samples; within their
    /// spacing of them: up to 0.044 mm on the 4.5 rad arc, which meets
    /// the 1024-chord cap), for arcs both ways and over and under a half
    /// turn, against segments in, across and outside the wedge.
    #[test]
    fn the_exact_distance_matches_a_dense_sampling() {
        let c = Point2::new(3.0, -2.0);
        let r = 10.0;
        let mut seed = 0x9e37_79b9_u64;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 1_000_000) as f64 / 1_000_000.0
        };
        for (s0, s1, ccw) in [(0.3, 1.4, true), (2.0, 0.1, false), (-1.0, 3.5, true), (4.0, 0.5, true)] {
            let at = |a: f64| Point3::new(c.x + r * f64::cos(a), c.y + r * f64::sin(a), 0.0);
            let m = Move::new(MoveKind::Feed, at(s0), at(s1), Some(100.0)).with_arc(c, ccw);
            let Trace::Arc(arc) = Trace::of(&m) else { panic!("an xy arc") };
            let pts: Vec<Point2> = m.chords(1e-5).into_iter().flat_map(|(a, b)| [a, b]).collect();
            for _ in 0..200 {
                let p = Point2::new(c.x - 25.0 + 50.0 * rnd(), c.y - 25.0 + 50.0 * rnd());
                let q = Point2::new(c.x - 25.0 + 50.0 * rnd(), c.y - 25.0 + 50.0 * rnd());
                let exact = arc.dist_seg(p, q);
                let sampled = pts.iter().map(|&x| dist2_segs(x, x, p, q).sqrt()).fold(f64::INFINITY, f64::min);
                assert!(exact <= sampled + 1e-9, "exact {exact} reads farther than sampled {sampled}");
                assert!(sampled - exact < 0.03, "exact {exact} vs sampled {sampled} for {p:?}-{q:?}");
            }
        }
    }
}
