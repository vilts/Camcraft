//! Lit pieces and leads for the sheet cuts (#670, shared by #1883): a
//! straight move or an arc to a point, the arc-fitted ride of a closed
//! walk, and a straight or quarter-arc lead into or out of a point on
//! the waste side, shortened while it finds no room beside the part.
//!
//! Provenance: moved out of `plasma::emit` without change of behaviour
//! (leads are plain circle geometry; Tormach's PathPilot guidance for
//! the straight G41/G42 entry). Arc linearization samples each arc by
//! `Move::chords`. Our own code.

use crate::cl::{Move, MoveKind, PathBuilder};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::{fit_arcs, Seg};

use super::laws::FIT_TOL;
use super::ring::dist;

/// How many times a lead that finds no room is halved before refusing.
pub const LEAD_TRIES: u32 = 4;

/// One lit piece: a straight move to a point, or an arc to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Piece {
    Line(Point2),
    Arc { to: Point2, center: Point2, ccw: bool },
}

impl Piece {
    /// Where the piece ends.
    #[must_use]
    pub fn to(self) -> Point2 {
        match self {
            Piece::Line(to) | Piece::Arc { to, .. } => to,
        }
    }
}

fn segs_to_pieces(segs: Vec<Seg>, out: &mut Vec<Piece>) {
    for s in segs {
        match s {
            Seg::Line(qs) => out.extend(qs.into_iter().map(Piece::Line)),
            Seg::Arc { center, ccw, to } => out.push(Piece::Arc { to, center, ccw }),
        }
    }
}

/// The ride of a walk as pieces: arcs where it is round.
#[must_use]
pub fn ride(walk: &[Point2]) -> Vec<Piece> {
    let mut out = Vec::new();
    segs_to_pieces(fit_arcs(walk, FIT_TOL), &mut out);
    out
}

#[must_use]
pub fn cross(a: Point2, b: Point2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Does every point of the stretch keep the kerf off the part?
#[must_use]
pub fn clear(forbidden: Option<&Region>, pts: &[Point2]) -> bool {
    forbidden.is_none_or(|f| pts.iter().all(|p| !f.contains(*p)))
}

/// Points along a quarter-arc lead (for the room test).
fn arc_samples(c: Point2, a: Point2, b: Point2, ccw: bool) -> Vec<Point2> {
    let (a0, a1) = ((a.y - c.y).atan2(a.x - c.x), (b.y - c.y).atan2(b.x - c.x));
    let mut sweep = a1 - a0;
    if ccw && sweep < 0.0 {
        sweep += std::f64::consts::TAU;
    } else if !ccw && sweep > 0.0 {
        sweep -= std::f64::consts::TAU;
    }
    let r = dist(c, a);
    (0..=16)
        .map(|k| {
            let t = a0 + sweep * f64::from(k) / 16.0;
            Point2::new(c.x + r * t.cos(), c.y + r * t.sin())
        })
        .collect()
}

/// One lead to lay: at `s` along travel `t` with waste normal `n`, this
/// long, straight or a quarter arc, the lead-in (`entry`, arriving at
/// `s`) or the lead-out (leaving it).
#[derive(Clone, Copy, Debug)]
pub struct LeadAsk {
    pub s: Point2,
    pub t: Point2,
    pub n: Point2,
    pub len: f64,
    pub straight: bool,
    pub entry: bool,
}

/// Lay a lead. Returns the far end and the piece set, halving the
/// length while it finds no room (a straight lead under control
/// compensation is tried once - its length is the entry rule's); `None`
/// when even the shortest try reaches the part. `note` is the voice of
/// the shortening note (`plasma`, `waterjet`).
pub fn lead(ask: LeadAsk, forbidden: Option<&Region>, tries: u32, notes: &mut Vec<String>, (note, label): (&str, &str)) -> Option<(Point2, Vec<Piece>)> {
    let LeadAsk { s, t, n, len, straight, entry } = ask;
    if len <= 0.0 {
        return Some((s, Vec::new()));
    }
    let mut l = len;
    for k in 0..tries.max(1) {
        let (far, piece, samples) = if straight {
            let far = Point2::new(s.x + n.x * l, s.y + n.y * l);
            let samples: Vec<Point2> = (0..=8)
                .map(|i| {
                    let f = f64::from(i) / 8.0;
                    Point2::new(s.x + (far.x - s.x) * f, s.y + (far.y - s.y) * f)
                })
                .collect();
            (far, if entry { Piece::Line(s) } else { Piece::Line(far) }, samples)
        } else {
            let r = 2.0 * l / std::f64::consts::PI;
            let c = Point2::new(s.x + n.x * r, s.y + n.y * r);
            let sign = if entry { -1.0 } else { 1.0 };
            let far = Point2::new(c.x + sign * t.x * r, c.y + sign * t.y * r);
            let ccw = cross(t, n) > 0.0;
            let (a, b) = if entry { (far, s) } else { (s, far) };
            (far, Piece::Arc { to: b, center: c, ccw }, arc_samples(c, a, b, ccw))
        };
        if clear(forbidden, &samples) {
            if k > 0 {
                notes.push(format!(
                    "{note}: {label} lead-{} shortened to {l:.3} mm - the stated {len:.3} mm found no room beside the part",
                    if entry { "in" } else { "out" }
                ));
            }
            return Some((far, vec![piece]));
        }
        l *= 0.5;
    }
    None
}

/// Push one piece at height `z`. With `linearize`, an arc is pushed as
/// its chords at that tolerance (`Move::chords`), for a control that
/// reads no G2/G3.
pub fn push_piece(b: &mut PathBuilder, q: Piece, z: f64, feed: Option<f64>, linearize: Option<f64>) {
    match (q, linearize) {
        (Piece::Line(to), _) => b.push(MoveKind::Feed, Point3::new(to.x, to.y, z), feed),
        (Piece::Arc { to, center, ccw }, None) => b.push_arc(MoveKind::Feed, Point3::new(to.x, to.y, z), feed, center, ccw),
        (Piece::Arc { to, center, ccw }, Some(tol)) => {
            let m = Move::new(MoveKind::Feed, b.at(), Point3::new(to.x, to.y, z), feed).with_arc(center, ccw);
            for (_, q) in m.chords(tol) {
                b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), feed);
            }
            // The last chord ends on `to` exactly (chords keep the
            // endpoints); this push is a no-op unless it did not.
            b.push(MoveKind::Feed, Point3::new(to.x, to.y, z), feed);
        }
    }
}
