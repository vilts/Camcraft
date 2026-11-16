//! Microlift returns (M11.4 C round 4): the growth rings' spiral, cut
//! climb-only and staying down, rides the whole cleared outline every
//! revolution - in a channel, a 0.25 mm advance per loop buys a wall
//! ride of twice the channel's length. Here the spiral is cut only
//! where it bites: its revolutions are split at their air stretches
//! (read on the planner's own stock, `raster`; `lift_split`), the biting
//! pieces kept exactly as planned, and each piece is reached from the
//! last by a lifted return - up off the floor by [`MICROLIFT`], straight
//! across stock already cleared, down onto the piece's air lead - the
//! return loop of trochoidal and adaptive milling taken above the floor
//! instead of along the walls (Otkur & Lazoglu 2007, "Trochoidal
//! milling", Int. J. Mach. Tools Manuf. 47, the cutting arc and its
//! non-cutting return; Pateloup, Duc & Ray 2004, "Corner optimization
//! for pocket machining", Int. J. Mach. Tools Manuf. 44, links taken
//! off the cut to keep the cutting path's dynamics; Ibaraki et al.
//! 2010, the constant-engagement trochoid's return arc).
//!
//! The pieces are re-ordered front by front: a piece waits only for
//! the pieces before it in the spiral that come within reach of its
//! reading (two rims), so every piece meets the stock it was planned
//! on, and the next piece is the nearest one ready - a channel's front
//! is cut loop after loop, its return across the channel, before the
//! next front is visited. Every piece is the spiral's own (the spiral
//! never crosses itself, so no two pieces cross); every return is a
//! straight chord inside the center area, its swept disc on no stock
//! and no wall, crossing no other return and no entry orbit, landing
//! where the rim reads no stock - or the ladder's own retract hop where
//! none is. The easing laps are split the same way on the stock the
//! pieces left; the last lap is cut whole. The re-ordered plan is
//! walked on a fresh stock and kept only if it reads no worse than the
//! spiral ([`keeps`]: no tolerance, issue #982 - a piece is cut on
//! longer chords only where they leave the stock its own chords leave,
//! `lift_coarse`, so the two readings differ by stock, never by the
//! chords). Ours; the schedule, the pads and the lift are our choices.

use crate::geo2d::{Point2, Region};

use super::front::{densify, dist, len};
use super::lift_coarse::{coarsen, cut};
use super::lift_link::{Leg, LinkLaw, Return, extend, free_map, landing, legs};
use super::lift_split::{Piece, depends, split};
use super::lift_swept::Swept;
use super::raster::Raster;
use super::{GenError, Planner, Worst};

/// Return lift above the crossed floor (mm): clear of its grid crumbs
/// and the cut's cusp, far under any stepdown.
pub(crate) const MICROLIFT: f64 = 0.3;

/// The nearest ready pieces a straight return is tried to, in order.
const TRIES: usize = 16;

/// The stock a piece is read on is within this many rims of it.
const REACH_RIMS: f64 = 2.0;

/// Extra stock share allowed for air slivers no longer ridden, plus [`CRUMB_CELLS`].
const CRUMBS: f64 = 0.25;
const CRUMB_CELLS: f64 = 200.0;

/// Past this many retract hops the plan stops scheduling (round 5): it
/// is over the pocket's hop budget (`pocket_hsm::HOP_BUDGET`) and will
/// be handed to the Offset rings by name, so routing every remaining
/// piece over the free-space map would only spend time.
const HOP_CAP: usize = super::super::pocket_hsm::HOP_BUDGET;

/// The last lap descends along this much of the walk before it (mm).
const LAST_LEAD: f64 = 3.0;

/// What the lifted plan is read with.
#[derive(Clone, Copy, Debug)]
pub(super) struct LiftSpec {
    pub r: f64,
    pub rim: f64,
    pub step: f64,
    /// Under this reading a segment is air.
    pub bite: f64,
    /// The reading a kept cut may take (`Planner::bound`).
    pub bound: f64,
    /// Where the closing laps begin and where the last one does (walk
    /// vertex indices): the growth before them is split on the spiral's
    /// own stock, the easing laps on the stock the pieces left, the last
    /// lap is kept whole.
    pub laps: (usize, usize),
    /// The entry's helix (center, radius; 0 for a plunge): every level's
    /// descent sweeps its circle, so no return may cross it.
    pub orbit: (Point2, f64),
}

/// The lifted plan: the pieces in cutting order, how each is reached - a
/// microlift return, or the ladder's hop (`None`; the first is the
/// entry's) - its worst reading and the stock it leaves, and the air it
/// no longer rides at the floor / lifts over (XY mm).
pub(super) struct Lifted {
    pub passes: Vec<Vec<Point2>>,
    pub joins: Vec<Option<Return>>,
    pub worst: Worst,
    pub ras: Raster,
    pub dropped: f64,
    pub lift: f64,
    pub hops: usize,
    /// Pieces reached by riding the spiral's own air at the floor.
    pub rides: usize,
    /// Returns taken over the retract plane.
    pub highs: usize,
    /// Every return so far, leg by leg.
    links: Vec<Vec<Leg>>,
    /// What the pieces cut so far swept, exactly: a rapid's landing is
    /// proved on it (`lift_swept`, issue #1169).
    swept: Swept,
}

/// Split `walk` at its air stretches and re-order the pieces with lifted
/// returns; `None` when there is nothing to lift (one piece).
pub(super) fn lift(walk: &[Point2], ras0: &Raster, center: &Region, s: &LiftSpec) -> Result<Option<Lifted>, GenError> {
    let Some(&w0) = walk.first() else { return Ok(None) };
    let last = walk.len() - 1;
    let ease = s.laps.0.min(last);
    let fin = s.laps.1.clamp(ease, last);
    let grown = &walk[..=ease];
    let mut pieces = split(grown, &ras0.clone().walk_marks(grown, (s.r, s.rim, s.step)), (s.r, s.bite), true);
    if pieces.len() < 2 {
        return Ok(None);
    }
    coarsen(&mut pieces, &walk[ease..]);
    let mut out = Lifted { passes: Vec::new(), joins: Vec::new(), worst: (0.0, w0), ras: ras0.clone(), dropped: 0.0, lift: 0.0, hops: 0, rides: 0, highs: 0, links: Vec::new(), swept: Swept::new(s.r) };
    schedule(&mut out, pieces, center, s, true);
    if out.hops > HOP_CAP {
        return Ok(Some(out));
    }
    // The easing laps, split on the stock the pieces left; the last lap
    // whole (the walls left as the plan names them).
    let easing = &walk[ease..=fin];
    let mut laps = split(easing, &out.ras.clone().walk_marks(easing, (s.r, s.rim, s.step)), (s.r, s.bite), false);
    if fin < last {
        // Its lead: the last easing lap's own end (cut by then, so the
        // descent onto the last lap reads only what that lap left).
        let mut k = fin;
        while k > ease && len(&walk[k..=fin]) < LAST_LEAD {
            k -= 1;
        }
        let lead = densify(&walk[k..=fin], 0.25, false);
        laps.push(Piece { pts: walk[fin..].to_vec(), lead, tail: Vec::new(), fine: None });
    }
    schedule(&mut out, laps, center, s, false);
    let kept: f64 = out.passes.iter().map(|p| len(p)).sum();
    out.dropped = (len(walk) - kept).max(0.0);
    Ok(Some(out))
}

/// The ride from `at`, where a piece with air `tail` ended, onto `p`,
/// when the spiral runs on to it there: the tail (and `p`'s lead) - the
/// walk's own air, cut at the floor; `None` where it does not. A piece
/// starting where the tool stands (the growth's last piece handing over
/// to the first easing lap, both in cut) is the walk itself running on:
/// ridden with no air between, never a retract and a plunge on one spot
/// (issue #1169: the reader counts that plunge a descent into stock).
pub(super) fn ride(at: Point2, tail: &[Point2], p: &Piece) -> Option<Vec<Point2>> {
    let near = |a: Point2, b: Point2| dist(a, b) <= 1e-9;
    if near(at, p.pts[0]) {
        return Some(Vec::new());
    }
    let (&t0, &t1) = (tail.first()?, tail.last()?);
    if near(t0, p.pts[0]) {
        return Some(Vec::new());
    }
    if near(t1, p.pts[0]) {
        return Some(tail[1..].to_vec());
    }
    let (&l0, _) = p.lead.split_first()?;
    near(t1, l0).then(|| tail[1..].iter().chain(p.lead[1..].iter()).copied().collect())
}

/// Cut `pieces` onto `out` in their order law, nearest ready first, each
/// reached by a legal microlift - straight, else routed through the
/// cleared area - where one is (else the ladder's hop); `entry`: the
/// first piece is the entry's own (no link).
fn schedule(out: &mut Lifted, mut pieces: Vec<Piece>, center: &Region, s: &LiftSpec, entry: bool) {
    let deps = depends(&pieces, REACH_RIMS * s.rim + 0.2);
    let mut done = vec![false; pieces.len()];
    // The air after the piece just cut: a return climbs along it.
    let mut tail: Vec<Point2> = Vec::new();
    for k in 0..pieces.len() {
        if out.hops > HOP_CAP {
            break;
        }
        let at = out.passes.last().and_then(|p| p.last()).copied();
        let mut ride_on: Option<Vec<Point2>> = None;
        let next = match at {
            Some(at) if !(entry && k == 0) => {
                let mut ready: Vec<usize> = (0..pieces.len()).filter(|&j| !done[j] && deps[j].iter().all(|&i| done[i])).collect();
                ready.sort_by(|&a, &b| dist(at, pieces[a].pts[0]).total_cmp(&dist(at, pieces[b].pts[0])).then(a.cmp(&b)));
                let Some(&first) = ready.first() else { break };
                let law = LinkLaw { ras: &out.ras, center, links: &out.links, swept: &out.swept, r: s.r, rim: s.rim, step: s.step, bite: s.bite, orbit: s.orbit };
                let mut pick = ready.iter().take(TRIES).find_map(|&j| law.find(at, &tail, &pieces[j], None).map(|r| (j, r)));
                // Else the walk's own order: its next piece (always ready),
                // ridden on to through the spiral's own air where it runs
                // on from here, else reached over the retract plane or by
                // a routed return.
                let low = ready.iter().copied().min().unwrap_or(first);
                let rode = if pick.is_none() { ride(at, &tail, &pieces[low]).map(|r| (low, r)) } else { None };
                if pick.is_none() && rode.is_none() {
                    pick = law.high(&pieces[low]).map(|r| (low, r));
                    if pick.is_some() {
                        out.highs += 1;
                    }
                }
                if pick.is_none() && rode.is_none() {
                    // Round 6: the lead re-read on the re-ordered stock,
                    // its graze joined to the cut (`LinkLaw::late`).
                    pick = law.late(at, &tail, &mut pieces[low]).map(|r| (low, r));
                    if let Some((_, r)) = &pick
                        && r.high
                    {
                        out.highs += 1;
                    }
                }
                if pick.is_none() && rode.is_none() {
                    let map = free_map(&out.ras);
                    pick = law.find(at, &tail, &pieces[low], Some(&map)).map(|r| (low, r));
                }
                let first = low;
                match (pick, rode) {
                    (Some((j, ret)), _) => {
                        let lg = legs(at, &ret);
                        out.lift += lg.iter().map(|l| dist(l.0, l.1)).sum::<f64>();
                        out.links.push(lg);
                        out.joins.push(Some(ret));
                        j
                    }
                    (None, Some((j, path))) => {
                        out.rides += 1;
                        ride_on = Some(path);
                        j
                    }
                    (None, None) => {
                        // The hop plunges where the rim reads no stock.
                        let st = landing(&out.ras, &out.swept, &pieces[first], (s.r, s.rim));
                        extend(&mut pieces[first], st);
                        out.hops += 1;
                        out.joins.push(None);
                        first
                    }
                }
            }
            _ => {
                // The entry's piece (or a first piece after nothing).
                out.joins.push(None);
                0
            }
        };
        // A ride is walked like any cut (it is the spiral's own air).
        if let (Some(path), Some(at)) = (&ride_on, at) {
            let mut run = vec![at];
            run.extend_from_slice(path);
            run.push(pieces[next].pts[0]);
            let w = out.ras.walk(&run, s.r, s.rim, s.step);
            out.swept.cut(&run);
            if w.0 > out.worst.0 {
                out.worst = w;
            }
        }
        // On its longer chords only where they read within the bound and
        // leave the stock its own chords leave (`lift_coarse::cut`).
        let p = &mut pieces[next];
        let w = cut(&mut out.ras, p, s);
        if w.0 > out.worst.0 {
            out.worst = w;
        }
        out.swept.cut(&p.pts);
        match (ride_on, out.passes.last_mut()) {
            (Some(path), Some(last)) => {
                last.extend(path);
                last.extend(p.pts.iter().copied());
                last.dedup_by(|a, b| dist(*a, *b) <= 1e-9);
            }
            _ => out.passes.push(p.pts.clone()),
        }
        tail.clone_from(&p.tail);
        done[next] = true;
    }
}

impl Planner<'_> {
    /// The spiral (`grow`, `morph`) cut with microlift returns, kept when
    /// its walk on a fresh stock reads no worse than the spiral's and
    /// leaves no more than grid crumbs over it; else `None` (the spiral
    /// posts as planned) with the reason noted.
    pub(super) fn lift_returns(&mut self, ras0: &Raster) -> Result<Option<Lifted>, GenError> {
        if self.laps_from == usize::MAX {
            // No closing lap was marked: the walk is posted whole.
            return Ok(None);
        }
        let laps = (self.laps_from.saturating_sub(1), self.fin_from.saturating_sub(1));
        let s = LiftSpec { r: self.s.r, rim: self.rim, step: self.step, bite: self.bite, bound: self.bound, laps, orbit: (self.s.seed, self.s.helix_r) };
        let Some(l) = lift(&self.walk, ras0, self.center, &s)? else { return Ok(None) };
        if l.hops > HOP_CAP {
            // Over the hop budget: posted as lifted so the pocket's budget
            // law names it and the Offset rings cut instead - never the
            // half-scheduled plan.
            return Ok(Some(l));
        }
        let (worst, left) = (l.worst.0, l.ras.standing() as f64);
        let planned = self.ras.standing() as f64;
        if !keeps(worst, self.worst.0, self.bound) || left > planned * (1.0 + CRUMBS) + CRUMB_CELLS {
            self.plan.notes.push(format!(
                "adaptive: the microlift plan read {worst:.1} deg over the spiral's {:.1} and left {:.1} mm2 over {:.1} - the spiral is posted as planned",
                self.worst.0.max(self.bound),
                left * self.ras.h * self.ras.h,
                planned * self.ras.h * self.ras.h
            ));
            return Ok(None);
        }
        Ok(Some(l))
    }
}

/// The acceptance law's reading half (issue #982): the lifted plan is
/// kept only when its worst reading is not over the spiral's own, or the
/// bound where the spiral read under it. No tolerance: readings are whole
/// rim samples, exact in binary, and one sample over is an overload the
/// spiral did not plan (the `1e-9` is float slack, far under a sample).
pub(super) fn keeps(lifted: f64, spiral: f64, bound: f64) -> bool {
    lifted <= spiral.max(bound) + 1e-9
}

/// Per-level travel (XY mm): cut, floor air, lifted; and the moves posted.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Travel {
    pub cut: f64,
    pub air: f64,
    pub lift: f64,
    pub moves: usize,
    /// Passes reached by the ladder's retract hop rather than a microlift
    /// return (the entry's own first pass aside).
    pub hops: usize,
}

/// The travel of `passes` (reached by `joins`) walked in order from
/// `ras0`, read by `(r, rim, step, bite)`: a segment reading over the
/// bite is cut, the rest floor air.
pub(super) fn travel(ras0: &Raster, passes: &[Vec<Point2>], joins: &[Option<Return>], (r, rim, step, bite): (f64, f64, f64, f64)) -> Travel {
    let mut ras = ras0.clone();
    let mut t = Travel::default();
    for (k, pass) in passes.iter().enumerate() {
        if let (Some(Some(ret)), Some(from)) = (joins.get(k), k.checked_sub(1).and_then(|j| passes[j].last())) {
            let lg = legs(*from, ret);
            t.lift += lg.iter().map(|l| dist(l.0, l.1)).sum::<f64>();
            t.moves += lg.len();
        } else if k > 0 {
            t.hops += 1;
        }
        for (w, m) in pass.windows(2).zip(ras.walk_marks(pass, (r, rim, step))) {
            if m > bite {
                t.cut += dist(w[0], w[1]);
            } else {
                t.air += dist(w[0], w[1]);
            }
        }
        t.moves += pass.len().saturating_sub(1);
    }
    t
}

/// The plan's note: how many returns are lifted, over how much.
pub(super) fn note(l: &Lifted) -> String {
    format!(
        "adaptive: {} microlift returns ({MICROLIFT:.1} mm or more over the cleared floor, {:.0} mm) replace {:.0} mm of wall riding in cut; {} over the retract plane, {} rides on at the floor, {} retract hops",
        l.joins.iter().filter(|j| j.is_some()).count(),
        l.lift,
        l.dropped,
        l.highs,
        l.rides,
        l.hops
    )
}
