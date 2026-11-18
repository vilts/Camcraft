//! Corner fillets (M11.1 B/C): contour corners and between-pass links
//! round a line-to-line direction change with ONE tangent arc (one
//! G2/G3 move). Provenance: the classical circle tangent to two
//! intersecting lines (analytic geometry; the corner-rounding practice
//! of CNC programming handbooks), emitted as RS-274 / ISO 6983 G2/G3
//! circular interpolation; clean-room, no vendor material.
//!
//! # The construction
//!
//! A corner `V` joins an incoming leg (unit direction `u`) to an
//! outgoing one (`w`), deflecting by `theta`. A fillet of radius `rho`
//! touches the legs `t = rho * tan(theta/2)` before and after `V` (the
//! tangent points `T1 = V - u t`, `T2 = V + w t`) and its center stands
//! `rho` off both legs on the inside of the turn. The arc lies inside
//! the triangle `V T1 T2`: it pulls the cutter center off the corner by
//! `e = rho * (sec(theta/2) - 1)` on the bisector, and never past the
//! legs. Every number is computed without a transcendental call
//! (`tan(theta/2) = sin/(1 + cos)`, `sec(theta/2) = sqrt(2/(1 + cos))`)
//! so the bytes are the same on every platform's libm (M4.2b's lesson).
//!
//! # The legality law (Prime Directive: never gouge, never silent)
//!
//! - **material side**: where the caller knows which side of travel the
//!   part is on (contours: `air_left`), a turn TOWARD the part is
//!   refused - its fillet would sit inside the turn, on the part's side
//!   of a path that already grazes it: a gouge by construction. A turn
//!   toward the air rounds; its arc stands farther from the part than
//!   the corner did, so the blend leaves corner REST, never a gouge,
//!   and that rest (`e`, the worst one) is stated in the notes for the
//!   stock sim to measure.
//! - **live geometry**: the whole triangle `V T1 T2` must sit on the
//!   legal side of the caller's cutter-center region (inside it for a
//!   pocket or bore, outside it for an outside wall): the chord `T1 T2`
//!   crosses no loop, touches no loop vertex, its midpoint is on the
//!   legal side, and no loop vertex stands strictly inside the triangle
//!   (an island poking into the corner). A refused radius is halved
//!   ([`SHRINK_TRIES`] times) before the corner is called blocked.
//! - **tool radius**: `rho <= r` - the rounding never claims more than
//!   the scale of the tool doing it.
//! - **leg room**: `t <= len/2` of each leg, so a leg shared by two
//!   corners feeds both and fillets never overlap.
//! - **tolerance / resolution**: `rho >= max(tolerance, MIN_BLEND_RADIUS)`;
//!   an arc under the op's own chord tolerance says nothing the corner
//!   did not, and below the resolution no control interpolates it.
//! - **shape**: line-to-line corners round here; a corner next to an
//!   arc rounds by the tangent-circle construction of `fillet_arc.rs`
//!   (same radius law, same region proof on the arc itself) or stays
//!   sharp, counted as curved. A full reversal has no tangent fillet,
//!   and a closed walk's seam - where it starts and ends - stays where
//!   the entry put it. Each refusal keeps its sharp corner and is counted by
//!   reason; nothing is silent.

use crate::geo2d::{Point2, Region};

use super::arcs::Seg;
use super::fillet_arc::judge_curved;
use super::fillet_tally::{FilletTally, Verdict, fillet_floor};
use super::ring::on_boundary;
use super::waterscan::chord_crosses_path;

/// `cos` of the smallest deflection that counts as a corner (0.5 deg):
/// straighter joints are a polyline's own tessellation noise, not a
/// direction change, and pass through untouched and uncounted.
const CORNER_COS: f64 = 0.999_961_923_064_171_4;

/// `1 + cos(theta)` below this is a full reversal (theta within ~45 urad
/// of pi): no tangent fillet exists.
const REVERSAL_EPS: f64 = 1e-9;

/// Halvings a blocked radius gets before the corner is called blocked.
pub const SHRINK_TRIES: usize = 4;

/// Geometric slack for the on-line / on-vertex tests: a tenth of a
/// micrometer per mm, far under any cut tolerance.
const EPS: f64 = 1e-9;

/// Where a fillet may stand: the cutter-center region the walk was
/// planned in, which side of it is legal, and - when the caller knows
/// it - which side of travel the part is on.
#[derive(Clone, Copy, Debug)]
pub struct Legal<'a> {
    pub region: &'a Region,
    /// `true`: legal centers are inside `region` (pocket floor, bore);
    /// `false`: outside it (an outside wall's grown region).
    pub inside: bool,
    /// `Some(true)`: air (no part) is LEFT of travel, so only left turns
    /// may round; `Some(false)`: right. `None`: both sides are swept
    /// floor (pocket passes) and the region test alone decides.
    pub air_left: Option<bool>,
}

#[derive(Clone, Copy)]
pub(super) enum El {
    Line(Point2, Point2),
    Arc { from: Point2, center: Point2, ccw: bool, to: Point2 },
}

impl El {
    pub(super) fn to(&self) -> Point2 {
        match *self {
            El::Line(_, b) => b,
            El::Arc { to, .. } => to,
        }
    }
    /// Unit travel direction at the start (`start`) or the end.
    pub(super) fn tangent(&self, start: bool) -> Option<Point2> {
        match *self {
            El::Line(a, b) => unit(b.x - a.x, b.y - a.y),
            El::Arc { from, center, ccw, to } => {
                let p = if start { from } else { to };
                let (dx, dy) = (p.x - center.x, p.y - center.y);
                if ccw { unit(-dy, dx) } else { unit(dy, -dx) }
            }
        }
    }
}

pub(super) fn unit(dx: f64, dy: f64) -> Option<Point2> {
    let m = (dx * dx + dy * dy).sqrt();
    (m > 0.0 && m.is_finite()).then(|| Point2::new(dx / m, dy / m))
}

pub(super) fn len(a: Point2, b: Point2) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

/// The walk as elements from `start`, zero-length pieces dropped (the
/// builder drops them too, so nothing posted changes) and exactly
/// collinear line runs merged (same motion, one leg).
fn elements(walk: &[Seg], start: Point2) -> Vec<El> {
    let mut els: Vec<El> = Vec::new();
    let mut at = start;
    for seg in walk {
        match seg {
            Seg::Line(qs) => {
                for &q in qs {
                    if q == at {
                        continue;
                    }
                    if let Some(El::Line(a, b)) = els.last().copied() {
                        let (ux, uy) = (b.x - a.x, b.y - a.y);
                        let (wx, wy) = (q.x - b.x, q.y - b.y);
                        let cross = ux * wy - uy * wx;
                        let scale = len(a, b) * len(b, q);
                        if ux * wx + uy * wy > 0.0 && cross.abs() <= EPS * EPS * scale {
                            if let Some(last) = els.last_mut() {
                                *last = El::Line(a, q);
                            }
                            at = q;
                            continue;
                        }
                    }
                    els.push(El::Line(at, q));
                    at = q;
                }
            }
            Seg::Arc { center, ccw, to } => {
                if *to != at {
                    els.push(El::Arc { from: at, center: *center, ccw: *ccw, to: *to });
                    at = *to;
                }
            }
        }
    }
    els
}

/// Signed distance-ish orientation of `p` against the directed line
/// `a -> b`, normalized by the edge length (mm).
fn side(a: Point2, b: Point2, p: Point2) -> f64 {
    let l = len(a, b);
    if l <= 0.0 {
        return 0.0;
    }
    ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)) / l
}

fn strictly_inside(p: Point2, a: Point2, b: Point2, c: Point2) -> bool {
    let (s1, s2, s3) = (side(a, b, p), side(b, c, p), side(c, a, p));
    (s1 > EPS && s2 > EPS && s3 > EPS) || (s1 < -EPS && s2 < -EPS && s3 < -EPS)
}

fn on_open_chord(p: Point2, a: Point2, b: Point2) -> bool {
    if len(p, a) <= EPS || len(p, b) <= EPS {
        return false;
    }
    super::arcs::chord_dev(p, a, b) <= EPS
}

/// The live-geometry half of the law: the triangle `v t1 t2` - which
/// holds the whole arc - stands on the legal side of `legal.region`.
fn triangle_clear(legal: &Legal, v: Point2, t1: Point2, t2: Point2) -> bool {
    let m = Point2::new((t1.x + t2.x) * 0.5, (t1.y + t2.y) * 0.5);
    let inside = legal.region.contains(m);
    let side_ok = on_boundary(legal.region, m) || inside == legal.inside;
    if !side_ok {
        return false;
    }
    for l in legal.region.loops() {
        let pts = l.points();
        if chord_crosses_path(t1, t2, pts, true) {
            return false;
        }
        if pts.iter().any(|&p| strictly_inside(p, v, t1, t2) || on_open_chord(p, t1, t2)) {
            return false;
        }
    }
    true
}

fn judge(a: &El, b: &El, r_max: f64, floor: f64, legal: &Legal) -> Verdict {
    let (Some(u), Some(w)) = (a.tangent(false), b.tangent(true)) else {
        return Verdict::Straight;
    };
    let c = u.x * w.x + u.y * w.y;
    let s = u.x * w.y - u.y * w.x;
    if !(c.is_finite() && s.is_finite()) || c >= CORNER_COS {
        return Verdict::Straight;
    }
    let (El::Line(p0, v), El::Line(_, p2)) = (*a, *b) else {
        return judge_curved(a, b, s > 0.0, c, r_max, floor, legal);
    };
    if 1.0 + c <= REVERSAL_EPS {
        return Verdict::Reversal;
    }
    let left = s > 0.0;
    if legal.air_left.is_some_and(|air_left| air_left != left) {
        return Verdict::TowardPart;
    }
    let tan_h = s.abs() / (1.0 + c);
    let sec_h = (2.0 / (1.0 + c)).sqrt();
    let room = 0.5 * len(p0, v).min(len(v, p2));
    let r_max = if r_max.is_finite() && r_max > 0.0 { r_max } else { 0.0 };
    let mut rho = r_max.min(room / tan_h);
    if !rho.is_finite() || rho < floor {
        return Verdict::Tiny;
    }
    for _ in 0..=SHRINK_TRIES {
        if rho < floor {
            break;
        }
        let t = rho * tan_h;
        let t1 = Point2::new(v.x - u.x * t, v.y - u.y * t);
        let t2 = Point2::new(v.x + w.x * t, v.y + w.y * t);
        let n = if left { Point2::new(-u.y, u.x) } else { Point2::new(u.y, -u.x) };
        let center = Point2::new(t1.x + n.x * rho, t1.y + n.y * rho);
        let finite = [t1.x, t1.y, t2.x, t2.y, center.x, center.y].iter().all(|x| x.is_finite());
        if finite && triangle_clear(legal, v, t1, t2) {
            return Verdict::Round(t1, t2, center, left, rho * (sec_h - 1.0));
        }
        rho *= 0.5;
    }
    Verdict::Blocked
}

/// Round every legal corner of `walk` (a [`Seg`] list starting at
/// `start`, as [`super::fit_arcs`] hands it; corners next to an arc by
/// the `fillet_arc` construction) with one
/// tangent arc of radius at most `r_max`, under `legal` and the
/// tolerance floor [`fillet_floor`]`(tol)`. The walk's first and last
/// points never move (leads and links still join where they did);
/// refused corners stay exactly as they were. Pure and bounded: one
/// pass over the corners, each judged in at most [`SHRINK_TRIES`] + 1
/// region scans. Degenerate input - zero-length pieces, collinear
/// runs, reversals, non-finite numbers - rounds nothing it cannot
/// prove and never panics.
pub fn fillet_corners(
    walk: &[Seg],
    start: Point2,
    r_max: f64,
    tol: f64,
    legal: &Legal,
) -> (Vec<Seg>, FilletTally) {
    let (out, tally, _) = fillet_corners_except(walk, start, r_max, tol, legal, &|_| false);
    (out, tally)
}

/// Where an arc of a rounded walk came from, when it is not an arc of
/// the walk handed in: the corner it rounds, or the end its arc had
/// before a corner arc trimmed it.
pub(super) enum Source {
    Corner(Point2),
    Trimmed(Point2),
}

/// An arc of the rounded walk (its end, center and sense) and its source.
pub(super) struct Traced {
    pub(super) to: Point2,
    pub(super) center: Point2,
    pub(super) ccw: bool,
    pub(super) source: Source,
}

/// [`fillet_corners`] with the corners `banned` names kept sharp (the
/// crossing law's refusal, #2317: counted as `crossed`), and the trace
/// of every arc the rounding added or trimmed. A ban changes no other
/// corner: each joint is judged on its own legs.
pub(super) fn fillet_corners_except(
    walk: &[Seg],
    start: Point2,
    r_max: f64,
    tol: f64,
    legal: &Legal,
    banned: &dyn Fn(Point2) -> bool,
) -> (Vec<Seg>, FilletTally, Vec<Traced>) {
    let mut trace = Vec::new();
    let els = elements(walk, start);
    let floor = fillet_floor(tol);
    let mut tally = FilletTally::default();
    // joints[j]: the fillet replacing the joint between els[j] and els[j+1].
    let mut joints: Vec<Option<(Point2, Point2, Point2, bool)>> = Vec::with_capacity(els.len());
    for pair in els.windows(2) {
        let verdict = match judge(&pair[0], &pair[1], r_max, floor, legal) {
            Verdict::Round(..) if banned(pair[0].to()) => Verdict::Crossed,
            v => v,
        };
        joints.push(tally.count(verdict));
    }
    // A closed walk's seam is a corner too - one the walk cannot round
    // without moving where it starts. Counted, never silent.
    if let (Some(last), Some(first)) = (els.last(), els.first())
        && els.len() > 1
        && last.to() == start
        && !matches!(judge(last, first, r_max, floor, legal), Verdict::Straight)
    {
        tally.corners += 1;
        tally.seam += 1;
    }
    if tally.rounded == 0 {
        // Nothing proved: the walk the caller handed in, verbatim.
        return (walk.to_vec(), tally, trace);
    }
    let mut out: Vec<Seg> = Vec::new();
    let mut line: Vec<Point2> = Vec::new();
    let mut at = start;
    for (i, el) in els.iter().enumerate() {
        let fillet = joints.get(i).copied().flatten();
        match *el {
            El::Line(..) => {
                // The cursor stands at this leg's (possibly trimmed)
                // start already: the previous fillet ended at its T2.
                let end = fillet.map_or(el.to(), |(t1, ..)| t1);
                if end != at {
                    line.push(end);
                    at = end;
                }
            }
            El::Arc { center, ccw, to, .. } => {
                if !line.is_empty() {
                    out.push(Seg::Line(std::mem::take(&mut line)));
                }
                // Trimmed to the fillet's T1 when its end corner rounds
                // (its start already sits at the previous fillet's T2).
                let end = fillet.map_or(to, |(t1, ..)| t1);
                if end != to {
                    trace.push(Traced { to: end, center, ccw, source: Source::Trimmed(to) });
                }
                out.push(Seg::Arc { center, ccw, to: end });
                at = end;
            }
        }
        if let Some((_, t2, c, ccw)) = fillet {
            if !line.is_empty() {
                out.push(Seg::Line(std::mem::take(&mut line)));
            }
            trace.push(Traced { to: t2, center: c, ccw, source: Source::Corner(el.to()) });
            out.push(Seg::Arc { center: c, ccw, to: t2 });
            at = t2;
        }
    }
    if !line.is_empty() {
        out.push(Seg::Line(line));
    }
    (out, tally, trace)
}
