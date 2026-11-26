//! Rotary Parallel's two halves (#2970). The laid raster is a 3-axis
//! finish of the bar from above: its raster rides the top, its rings
//! the flanks, and nothing reaches the underside - a quarter of the
//! turn and more stayed uncut. The bar turns, so the generator plans
//! it twice: once in the laid frame (half A) and once in that frame
//! turned half a turn about the axis (half B). Each half keeps only
//! the cut whose ball CENTER stands in its own upper half, and half B
//! is carried back into half A's frame by the same rigid half turn.
//! A ball sphere that touched the model nowhere stays so under a
//! rotation; the radial pose that follows keeps every center.
//!
//! Provenance: our own design; a rigid half turn about a line and
//! clipping a segment at a plane are elementary geometry.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::ARC_FIT_TOL;

use super::gaps::{self, Cover};

/// Half A (`a`, planned in the laid frame over `local`) and half B
/// (`b`, planned in that frame turned half a turn) as one base path in
/// A's frame.
///
/// The seam: A keeps the centers down to half a ball radius under the
/// axis plane (its first ring past the equator), B those strictly under
/// A's lowest kept center on each side, read over the turned surface.
/// Each seam pass is cut once, and no seam step is wider than a ring
/// step of the ladder.
///
/// Past the seam no cut is lost and none is made twice (#2970 review):
/// a cut of A under the seam stays where no kept cut of B stands within
/// the pass spacing `d` (a keyway floor B cannot see), and a cut of B
/// over the seam is added where no kept cut of A stands within `d` (a
/// wall that faces down, which A cannot see). B is held to the Bottom
/// ([`over_bottom`]). The places no half reaches are named in the notes
/// ([`gaps::unreached`]); only a path with none says it goes all the
/// way round.
pub(super) fn both(a: &Toolpath, b: &Toolpath, local: &[Triangle], r: f64, d: f64, bottom: f64, tol: f64) -> Toolpath {
    let (x0, x1, rho) = local.iter().flat_map(|t| [t.a, t.b, t.c]).fold(
        (f64::INFINITY, f64::NEG_INFINITY, 0.0f64),
        |(l, h, rho), p| (l.min(p.x), h.max(p.x), rho.max((p.y * p.y + p.z * p.z).sqrt())),
    );
    let seat = Seat { reach: 0.5 * rho, x0: x0 + r, x1: x1 - r };
    let (_, lo) = upper(a, r, [-0.5 * r; 2], false, &seat);
    // B's side y >= 0 is A's y <= 0: the half turn swaps them.
    let (b_seam, _) = upper(b, r, [-lo[1], -lo[0]], true, &seat);
    let b_seam = over_bottom(&turned(&b_seam, r), r, bottom, tol);
    // "The same place": a centre within half a pass spacing.
    let near_b = Cover::of(&b_seam, r, 0.5 * d);
    let centre = move |t: Point3| Point3::new(t.x, t.y, t.z + r);
    let lim = -0.5 * r - r;
    let a = kept(a, &|t| t.z >= lim || !near_b.near(centre(t)), d);
    let near_a = Cover::of(&a, r, 0.5 * d);
    // B's cuts the seam set aside (in A's frame: centres at or over A's
    // lowest on their side), kept where A cuts nothing near.
    let b_extra = kept(&turned(b, r), &|t| t.z + r >= lo[side(t)] && !near_a.near(centre(t)), d);
    let b_extra = over_bottom(&b_extra, r, bottom, tol);
    let under = [&b_seam, &b_extra].iter().any(|p| p.moves.iter().any(|m| m.kind == MoveKind::Feed));
    let mut path = joined(joined(a, b_seam), b_extra);
    finish_notes(&mut path, local, r, d, bottom, tol, under);
    path
}

/// Half B alone (half A found no surface): turned into A's frame and
/// held to the Bottom, with the reach notes.
pub(super) fn only_b(b: &Toolpath, local: &[Triangle], r: f64, d: f64, bottom: f64, tol: f64) -> Toolpath {
    let mut path = over_bottom(&turned(b, r), r, bottom, tol);
    let under = path.moves.iter().any(|m| m.kind == MoveKind::Feed);
    finish_notes(&mut path, local, r, d, bottom, tol, under);
    path
}

/// The reach notes: each arc no pass reaches, or - when the path cuts
/// under the axis and leaves no such arc - the full-turn note.
fn finish_notes(path: &mut Toolpath, local: &[Triangle], r: f64, d: f64, bottom: f64, tol: f64, under: bool) {
    let gaps = gaps::unreached(local, &Cover::of(path, r, d + tol), r, bottom, tol);
    if gaps.is_empty() {
        if under {
            path.notes.push(UNDER.into());
        }
    } else {
        path.notes.extend(gaps);
    }
}

/// The note a path that cuts under the axis carries: the machine must
/// turn the part, and the Bottom is how to keep to the upper arc.
const UNDER: &str = "Rotary Parallel: the passes go all the way round the bar - the machine must turn the part on a rotary axis through the whole turn; set Bottom above the axis to cut only the upper arc";

/// The lowest ball center a half keeps, on each side of the frame's
/// vertical plane through the axis: `[y >= 0, y < 0]`. The two sides
/// are the two seams of the turn; a ladder's levels need not match on
/// both, so each seam reads its own.
type Floor = [f64; 2];

/// Where a seam may be read: off the vertical plane by `reach`, between
/// the model's end stations.
struct Seat {
    reach: f64,
    x0: f64,
    x1: f64,
}

impl Seat {
    fn holds(&self, p: Point3) -> bool {
        p.y.abs() >= self.reach && p.x > self.x0 && p.x < self.x1
    }
}

fn side(p: Point3) -> usize {
    usize::from(p.y < 0.0)
}

/// `base` (laid-frame raster, vertical-ball tips: the center stands
/// `r` above the tip) with every cutting move kept where its ball
/// center stands at local z >= the `floor` of its side (z > it when
/// `strict`). Beside the path stands the lowest center of a chord kept
/// whole, on each side (+infinity where none is), read only over the
/// turned surface: the center stands `seat.reach` or more off the
/// vertical plane and strictly between the model's end stations
/// `seat.x0..seat.x1`. The seams lie out on the turned surface; a ring
/// round an end face, beside it, is no seam. A piece cut at the plane
/// ends there by construction, so it never reads as a pass of the
/// ladder.
fn upper(base: &Toolpath, r: f64, floor: Floor, strict: bool, seat: &Seat) -> (Toolpath, Floor) {
    let keeps = move |p: Point3| {
        let lim = floor[side(p)] - r; // the tip of the lowest kept center
        if strict { p.z > lim } else { p.z >= lim }
    };
    let mut lo = [f64::INFINITY; 2];
    for m in base.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        for (a, b) in chords(m) {
            if keeps(a) && keeps(b) {
                for p in [a, b].into_iter().filter(|p| seat.holds(*p)) {
                    lo[side(p)] = lo[side(p)].min(p.z + r);
                }
            }
        }
    }
    (kept(base, &keeps, f64::INFINITY), lo)
}

pub(super) fn chords(m: &Move) -> Vec<(Point3, Point3)> {
    match m.arc {
        None => vec![(m.from, m.to)],
        Some(_) => m.segments3(ARC_FIT_TOL),
    }
}

/// `base` with every cutting move kept where `keeps` holds, read every
/// `step` along each chord (a chord the predicate crosses more than
/// once is split there; one crossing is found on the whole chord, as a
/// plane cut is). Links pass through: the radial relink rebuilds them
/// from the cut runs. Where the cut leaves the kept region and comes
/// back, a plunge marker stands between the two pieces, so the relink
/// reads two runs and links them over the retract cylinder, never with
/// a feed across the part.
fn kept(base: &Toolpath, keeps: &dyn Fn(Point3) -> bool, step: f64) -> Toolpath {
    let mut out: Vec<Move> = Vec::with_capacity(base.moves.len());
    let mut cut_end: Option<Point3> = None;
    for m in &base.moves {
        if m.kind != MoveKind::Feed {
            out.push(*m);
            cut_end = None;
            continue;
        }
        let cs: Vec<(Point3, Point3)> = chords(m).into_iter().flat_map(|(a, b)| split(a, b, keeps, step)).collect();
        let whole = cs.iter().all(|(a, b)| keeps(*a) && keeps(*b));
        let pieces: Vec<(Point3, Point3)> = if whole {
            vec![(m.from, m.to)]
        } else {
            let mut ps: Vec<(Point3, Point3)> = Vec::new();
            for (a, b) in cs.into_iter().filter_map(|(a, b)| part(a, b, keeps)) {
                // Pieces of one chord that meet ride as one.
                match ps.last_mut() {
                    Some(last) if last.1 == a && collinear(last.0, a, b) => last.1 = b,
                    _ => ps.push((a, b)),
                }
            }
            ps
        };
        for (a, b) in pieces {
            if let Some(e) = cut_end
                && e != a
            {
                out.push(Move::new(MoveKind::Plunge, e, a, m.feed));
            }
            out.push(if whole { *m } else { Move { from: a, to: b, arc: None, ..*m } });
            cut_end = Some(b);
        }
    }
    let mut path = Toolpath::new(out);
    path.notes = base.notes.clone();
    path
}

/// The chord `a..b` as itself when `keeps` changes at most once along
/// it (read every `step`), else as sub-chords of about `step`.
fn split(a: Point3, b: Point3, keeps: &dyn Fn(Point3) -> bool, step: f64) -> Vec<(Point3, Point3)> {
    let len = (b - a).dot(b - a).sqrt();
    if !(len.is_finite() && step.is_finite() && step > 0.0 && len > step) {
        return vec![(a, b)];
    }
    let n = ((len / step).ceil() as usize).clamp(1, 4096);
    let at = |i: usize| {
        let t = i as f64 / n as f64;
        Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t)
    };
    let flips = (0..n).filter(|i| keeps(at(*i)) != keeps(at(i + 1))).count();
    if flips <= 1 {
        return vec![(a, b)];
    }
    (0..n).map(|i| (if i == 0 { a } else { at(i) }, if i + 1 == n { b } else { at(i + 1) })).collect()
}

/// Whether `a`, `b`, `c` run on one line, `b` between the two.
fn collinear(a: Point3, b: Point3, c: Point3) -> bool {
    let (u, v) = (b - a, c - b);
    let x = u.cross(v);
    x.dot(x) <= 1e-18 * u.dot(u).max(1e-30) * v.dot(v).max(1e-30) && u.dot(v) > 0.0
}

/// The part of the segment `a..b` that `keeps`, when one end is kept
/// and the other not: the crossing found by bisection (both kept
/// regions here are bounded by one smooth surface, crossed once by a
/// chord far shorter than the bar). None when neither end is kept, or
/// only a point is.
fn part(a: Point3, b: Point3, keeps: &dyn Fn(Point3) -> bool) -> Option<(Point3, Point3)> {
    let (ina, inb) = (keeps(a), keeps(b));
    if ina && inb {
        return Some((a, b));
    }
    if !ina && !inb {
        return None;
    }
    let at = |t: f64| Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t);
    // The kept end at t = 0 of the search, the other at t = 1.
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    let kept_at = |t: f64| keeps(at(if ina { t } else { 1.0 - t }));
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if kept_at(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let q = at(if ina { lo } else { 1.0 - lo });
    let (p, e) = if ina { (a, q) } else { (q, b) };
    (p != e).then_some((p, e))
}

/// `base` (in A's frame, vertical-ball tips) kept where a radially
/// posed ball touches a round bar at world height `bottom - tol` or
/// above: the contact stands `r` in from the center along the radial.
/// Half B is held to the operation's Bottom this way - its own frame's
/// ladder law reads the Bottom upside down - so a Bottom over the
/// axis keeps the cut to the upper arc, and a Bottom at the bar's
/// underside lets the passes go all the way round.
fn over_bottom(base: &Toolpath, r: f64, bottom: f64, tol: f64) -> Toolpath {
    let keeps = move |t: Point3| {
        let (cy, cz) = (t.y, t.z + r);
        let rho = (cy * cy + cz * cz).sqrt();
        let contact = if rho > 1e-12 { cz - r * cz / rho } else { cz };
        contact >= bottom - tol
    };
    kept(base, &keeps, f64::INFINITY)
}

/// Half B's path in half A's frame: the frames share the axis line
/// (local x) and differ by half a turn about it, so a ball center
/// (x, y, z) of B is (x, -y, -z) of A. Tips are carried as their
/// centers (`r` up their own frame's z) and set back `r` below the
/// center in A. Arcs ride as their chords at `ARC_FIT_TOL`: the pose
/// that follows flattens them the same way. B's lifts off a pass
/// (straight up B's z into a rapid) run DOWN A's z once turned, where
/// the radial relink no longer reads them as lifts: they ride as the
/// links they are, rapids, and the relink rebuilds them radially.
fn turned(base: &Toolpath, r: f64) -> Toolpath {
    let t = |p: Point3| Point3::new(p.x, -p.y, -p.z - 2.0 * r);
    let ms = &base.moves;
    let mut out: Vec<Move> = Vec::with_capacity(ms.len());
    for (i, m) in ms.iter().enumerate() {
        let lift = m.kind == MoveKind::Feed
            && m.arc.is_none()
            && m.from.x == m.to.x
            && m.from.y == m.to.y
            && m.to.z > m.from.z
            && ms.get(i + 1).is_some_and(|n| n.kind == MoveKind::Rapid);
        if lift {
            out.push(Move::new(MoveKind::Rapid, t(m.from), t(m.to), None));
            continue;
        }
        match m.arc {
            None => out.push(Move { from: t(m.from), to: t(m.to), ..*m }),
            Some(_) => out.extend(
                m.segments3(ARC_FIT_TOL)
                    .into_iter()
                    .map(|(a, b)| Move { from: t(a), to: t(b), arc: None, ..*m }),
            ),
        }
    }
    let mut path = Toolpath::new(out);
    path.notes = base.notes.clone();
    path
}

/// Half A, then half B, as one base path. The step between them is a
/// rapid: the radial relink turns it into a link over the retract
/// cylinder like every other lift. A note both halves raise is
/// stated once.
fn joined(a: Toolpath, b: Toolpath) -> Toolpath {
    let mut moves = a.moves;
    if let (Some(last), Some(first)) = (moves.last().copied(), b.moves.first())
        && last.to != first.from
    {
        moves.push(Move::new(MoveKind::Rapid, last.to, first.from, None));
    }
    moves.extend(b.moves);
    let mut notes = a.notes;
    for n in b.notes {
        if !notes.contains(&n) {
            notes.push(n);
        }
    }
    let mut path = Toolpath::new(moves);
    path.notes = notes;
    path
}
