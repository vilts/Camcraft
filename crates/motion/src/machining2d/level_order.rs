//! The walking order of the start-side ring pieces (#3316) and the two
//! proofs it posts by. Each connected piece of the area is cut on its
//! own: ring 0, then each ring away from the start side, the walls last,
//! the nearest piece of a ring next. A piece joins the pass before it
//! only by a chord that stays in the area, crosses no planned cut, and
//! runs through cut material all along (the centre inside the swath of a
//! cut already walked, [`SWATH`] radii from it). Else the pass ends, and
//! the next pass starts ON a cut already walked, with a lead-in chord of
//! the same law to the piece (so its re-entry plunges into a cleared
//! column); the ladder's own pass link lifts or proves the hop. A cut
//! that takes the full tool width is counted and named, never silent.
//! Provenance: our own order and checks over the level curves of
//! `level_rings`; no vendor material consulted.

use crate::geo2d::{Point2, Region};

use super::face::GenError;
use super::level_flanks::{Material, full_width_at};
use super::pass_start::StartSide;
use super::pocket_hsm::components;
use super::ring_links::{RingLinks, edge_route};
use super::ring::dist;
use super::slots::chord_clears;
use super::waterscan::chord_crosses_path;

/// The level of the walls: cut after every ring.
pub(super) const WALL: u32 = u32::MAX;

/// How near (in tool radii) a join or lead-in chord's centre must stay to
/// a cut already walked: inside that cut's swath, a tenth of a radius in.
const SWATH: f64 = 0.9;

/// One ring piece: its ring index (0 on the start side, [`WALL`] for the
/// walls), its points in walking order, and whether it is a whole ring
/// (walked closed, from the vertex nearest the tool).
pub(super) struct Piece {
    pub(super) level: u32,
    pub(super) pts: Vec<Point2>,
    pub(super) closed: bool,
    /// A ring of a part with no start side, cut core first as Auto cuts
    /// it: its first ring bores in like Auto's, so it is not judged here.
    pub(super) core: bool,
}

/// The passes of one level and the warnings they owe the op page.
pub(super) struct LevelPlan {
    pub(super) passes: Vec<Vec<Point2>>,
    pub(super) notes: Vec<String>,
}

/// A walk already cut and its bounding box.
struct Cut {
    pts: Vec<Point2>,
    lo: Point2,
    hi: Point2,
}

impl Cut {
    fn new(pts: Vec<Point2>) -> Cut {
        let mut lo = Point2::new(f64::INFINITY, f64::INFINITY);
        let mut hi = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in &pts {
            lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        Cut { pts, lo, hi }
    }

    /// Is `q` within `d` of this walk?
    fn near(&self, q: Point2, d: f64) -> bool {
        if q.x < self.lo.x - d || q.x > self.hi.x + d || q.y < self.lo.y - d || q.y > self.hi.y + d {
            return false;
        }
        match self.pts.as_slice() {
            [p] => dist(*p, q) <= d,
            pts => pts.windows(2).any(|w| seg_dist(q, w[0], w[1]) <= d),
        }
    }
}

/// Order `pieces` into passes over `center` for a tool of radius
/// `tool_r`, with the warnings `side` owes.
pub(super) fn order(pieces: Vec<Piece>, center: &Region, tool_r: f64, side: &StartSide) -> Result<LevelPlan, GenError> {
    if !(tool_r.is_finite() && tool_r > 0.0) {
        return Err(GenError::Invalid("the tool radius must be positive and finite".into()));
    }
    let comps = components(center)?;
    let mut groups: Vec<Vec<Piece>> = (0..comps.len().max(1)).map(|_| Vec::new()).collect();
    for p in pieces {
        let c = comp_of(&p.pts, &comps).min(groups.len() - 1);
        groups[c].push(p);
    }
    let cores = groups.iter().filter(|g| g.iter().any(|p| p.core)).count();
    let seq = sequence(groups);
    let planned: Vec<&[Point2]> = seq.iter().map(|p| p.pts.as_slice()).collect();
    let reach = SWATH * tool_r;
    let step = 0.25 * tool_r;
    // #3788: the full-width law is the flanks law of the meter.
    let material = Material::new(center, side.stock.as_ref(), tool_r)?;
    // #3747: round a part a join may run through air too, and along the
    // edge of the area where that edge is in air.
    let air = match side.around_part {
        true => Some(RingLinks::new(side.stock.as_ref(), tool_r, None)?),
        false => None,
    };
    let air = air.as_ref();
    let mut done: Vec<Cut> = Vec::new();
    let mut passes: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    let mut wall_first = false;
    let mut narrow: Option<(usize, Point2)> = None;
    for p in &seq {
        if p.core {
        } else if p.level == 0 {
            wall_first = wall_first || on_wall(&p.pts, side.stock.as_ref(), tool_r, step);
        } else if let Some(at) = full_width_at(&samples(&p.pts, step), &material, tool_r, |q, d| done.iter().rev().any(|c| c.near(q, d))) {
            narrow = Some(narrow.map_or((1, at), |(n, first)| (n + 1, first)));
        }
        let join = match (cur.last(), p.pts.first()) {
            (Some(&e), Some(&s)) if linkable(e, s, center, &planned, &done, reach, step, air) => Some((e, s, Vec::new())),
            (Some(&e), Some(&s)) => air.and_then(|_| {
                let walks: Vec<&[Point2]> = planned.iter().copied().chain(done.iter().map(|c| c.pts.as_slice())).collect();
                let mut via = edge_route(e, s, center, reach, &walks, |a, b| linkable(a, b, center, &planned, &done, reach, step, air))?;
                via.pop();
                Some((e, s, via))
            }),
            _ => None,
        };
        match join {
            Some((e, s, via)) => {
                if e == s {
                    cur.extend(p.pts.iter().skip(1).copied());
                } else {
                    let mut leg = vec![e];
                    leg.extend(via.iter().copied());
                    leg.push(s);
                    done.push(Cut::new(leg));
                    cur.extend(via);
                    cur.extend(p.pts.iter().copied());
                }
            }
            None => {
                if cur.len() >= 2 {
                    passes.push(std::mem::take(&mut cur));
                }
                let lead = match p.pts.first() {
                    Some(&s) if p.level > 0 => lead_in(s, center, &planned, &done, reach, step, air),
                    _ => None,
                };
                if let (Some(q), Some(&s)) = (lead, p.pts.first()) {
                    done.push(Cut::new(vec![q, s]));
                    cur = vec![q];
                    cur.extend(p.pts.iter().copied());
                } else {
                    // #3748: a later pass that no lead-in reaches takes the
                    // pass link's own entry - the user's - and no automatic
                    // ramp (the #3567 dead-end ramp is retired).
                    cur = p.pts.clone();
                }
            }
        }
        done.push(Cut::new(p.pts.clone()));
    }
    if cur.len() >= 2 {
        passes.push(cur);
    }
    let mut notes = Vec::new();
    if cores > 0 {
        notes.push(side.core_note(cores));
    }
    if wall_first && let Some(n) = side.wall_first_note() {
        notes.push(n);
    }
    if let Some((n, at)) = narrow {
        notes.push(side.narrow_note(n, at.x, at.y));
    }
    Ok(LevelPlan { passes, notes })
}

/// The component of `center` a piece belongs to: the one that holds (or
/// stands nearest to) its middle vertex.
fn comp_of(pts: &[Point2], comps: &[Region]) -> usize {
    let Some(&m) = pts.get(pts.len() / 2) else {
        return 0;
    };
    let gap = |c: &Region| {
        if c.contains(m) {
            0.0
        } else {
            c.loops().iter().flat_map(|l| ring_segs(l.points())).map(|(a, b)| seg_dist(m, a, b)).fold(f64::INFINITY, f64::min)
        }
    };
    comps
        .iter()
        .enumerate()
        .min_by(|a, b| gap(a.1).total_cmp(&gap(b.1)))
        .map_or(0, |(i, _)| i)
}

/// Each group's rings in ascending level, the nearest piece next; a
/// whole ring starts at its vertex nearest the tool and is walked closed.
fn sequence(groups: Vec<Vec<Piece>>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut pos: Option<Point2> = None;
    for g in groups {
        let mut levels: Vec<u32> = g.iter().map(|p| p.level).collect();
        levels.sort_unstable();
        levels.dedup();
        let mut left: Vec<Option<Piece>> = g.into_iter().map(Some).collect();
        for lev in levels {
            loop {
                let mut best: Option<(f64, usize, usize)> = None;
                for (k, slot) in left.iter().enumerate() {
                    let Some(p) = slot.as_ref().filter(|p| p.level == lev) else { continue };
                    let (d, v) = pos.map_or((0.0, 0), |at| entry_of(p, at));
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, k, v));
                    }
                }
                let Some((_, k, v)) = best else { break };
                let Some(mut p) = left.get_mut(k).and_then(Option::take) else { break };
                if p.closed && v < p.pts.len() {
                    p.pts.rotate_left(v);
                    if let Some(&f) = p.pts.first() {
                        p.pts.push(f);
                    }
                }
                pos = p.pts.last().copied();
                out.push(p);
            }
        }
    }
    out
}

/// Where the tool enters `p` from `at`, and how far it travels to it.
fn entry_of(p: &Piece, at: Point2) -> (f64, usize) {
    if p.closed {
        p.pts
            .iter()
            .enumerate()
            .map(|(i, &q)| (dist(at, q), i))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap_or((f64::INFINITY, 0))
    } else {
        p.pts.first().map_or((f64::INFINITY, 0), |&q| (dist(at, q), 0))
    }
}

/// The keep-down join law of the start-side plan: inside the area,
/// crossing no planned cut and no cut join, and through cut material at
/// every sample (the centre within `reach` of a walk already cut) - or,
/// round a part (#3747), through air.
#[allow(clippy::too_many_arguments)]
fn linkable(e: Point2, s: Point2, center: &Region, planned: &[&[Point2]], done: &[Cut], reach: f64, step: f64, air: Option<&RingLinks>) -> bool {
    if e == s {
        return true;
    }
    chord_clears(center, e, s)
        && !planned.iter().any(|w| chord_crosses_path(e, s, w, false))
        && !done.iter().any(|c| chord_crosses_path(e, s, &c.pts, false))
        && samples(&[e, s], step).iter().all(|&q| air.is_some_and(|a| a.in_air(q)) || done.iter().rev().any(|c| c.near(q, reach)))
}

/// The vertex of a cut already walked nearest the piece start `s` (a
/// vertex, so the chord meets that cut at a shared point), when a chord
/// from it to `s` obeys the join law: the pass then starts on cut floor,
/// its column cleared.
fn lead_in(s: Point2, center: &Region, planned: &[&[Point2]], done: &[Cut], reach: f64, step: f64, air: Option<&RingLinks>) -> Option<Point2> {
    let mut near: Vec<(f64, Point2)> = done
        .iter()
        .filter(|c| c.near(s, reach))
        .flat_map(|c| c.pts.iter().map(|&q| (dist(q, s), q)))
        .filter(|(d, _)| *d <= reach && *d > 0.0)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.into_iter().take(8).map(|(_, q)| q).find(|&q| linkable(q, s, center, planned, done, reach, step, air))
}

/// A start ring stands on a wall when its disc lies wholly in the stock
/// somewhere (with no footprint known, everything is stock).
pub(super) fn on_wall(pts: &[Point2], stock: Option<&Region>, tool_r: f64, step: f64) -> bool {
    let Some(s) = stock else { return true };
    samples(pts, step).iter().any(|&q| s.contains(q) && edge_dist(q, s) >= 0.9 * tool_r)
}

/// Points along the walk `pts` no farther apart than `step`, both ends
/// included.
fn samples(pts: &[Point2], step: f64) -> Vec<Point2> {
    let mut out: Vec<Point2> = pts.first().copied().into_iter().collect();
    for w in pts.windows(2) {
        let n = (dist(w[0], w[1]) / step).ceil().clamp(1.0, 1e5) as usize;
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push(Point2::new(w[0].x + t * (w[1].x - w[0].x), w[0].y + t * (w[1].y - w[0].y)));
        }
    }
    out
}

fn edge_dist(q: Point2, r: &Region) -> f64 {
    r.loops().iter().flat_map(|l| ring_segs(l.points())).map(|(a, b)| seg_dist(q, a, b)).fold(f64::INFINITY, f64::min)
}

fn ring_segs(p: &[Point2]) -> impl Iterator<Item = (Point2, Point2)> + '_ {
    (0..p.len()).map(move |i| (p[i], p[(i + 1) % p.len()]))
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 && dd.is_finite() { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}
