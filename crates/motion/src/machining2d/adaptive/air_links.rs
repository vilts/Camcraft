//! Issue #1950: the joins between the passes of the air-side stock ring
//! plan (`air_rings`). Each pass is reached from the end of the last by
//! the microlift plan's own return law (`lift_link::LinkLaw`: a lifted
//! straight chord, a return over the retract plane, or a lead whose
//! graze joins the pass), proved on the stock as the passes before it
//! left it - else the ladder's retract hop. The return loop of
//! trochoidal and adaptive milling (Otkur & Lazoglu 2007, Int. J. Mach.
//! Tools Manuf. 47) taken above the floor. Ours; no vendor material.

use crate::geo2d::{Point2, Region};
use crate::machining2d::slots::chord_clears;

use super::front::{densify, dist};
use super::lift_link::{LinkLaw, RAMP_RUN, Return, legs};
use super::lift_split::Piece;
use super::lift_swept::Swept;
use super::air_uncross::Kept;
use super::raster::Raster;

/// A synthetic lead runs back from a pass start this many ramp runs.
const LEAD_RUNS: f64 = 3.0;

/// The read a join is proved with: tool radius, rim, step, bite.
pub(super) type Read = (f64, f64, f64, f64);

/// The joined plan: the passes (a pass whose lead grazed starts on that
/// lead), how each is reached, and the worst reading of the passes
/// walked in order (degrees of rim, and where).
pub(super) struct Joined {
    pub passes: Vec<Vec<Point2>>,
    pub joins: Vec<Option<Return>>,
    pub worst: (f64, Point2),
}

/// The joins of `passes` on the stock `ras0` (before the first pass),
/// the tool centre kept in `center`: `None` for the first pass and where
/// no return is legal (a retract hop). A return over the retract plane
/// lands only where the passes before it swept (`lift_swept`).
pub(super) fn air_joins(ras0: &Raster, passes: &[Vec<Point2>], center: &Region, (r, rim, step, bite): Read) -> Joined {
    let mut ras = ras0.clone();
    let mut swept = Swept::new(r);
    let mut links = Vec::new();
    let mut joins = Vec::with_capacity(passes.len());
    let mut out = Vec::with_capacity(passes.len());
    let mut worst = (0.0f64, Point2::new(0.0, 0.0));
    // The passes as they stand: a pass grown by its grazing lead is kept
    // only where the lead crosses no other pass (the crossing law).
    let mut kept = Kept::default();
    for (k, pass) in passes.iter().enumerate() {
        kept.add_as(pass, k);
    }
    for (k, pass) in passes.iter().enumerate() {
        let mut piece = Piece { pts: pass.clone(), lead: lead_of(pass, center), tail: Vec::new(), fine: None };
        let from = k.checked_sub(1).and_then(|j| passes.get(j)).and_then(|p| p.last()).copied();
        let join = match from {
            Some(a) if pass.len() >= 2 => {
                let law = LinkLaw { ras: &ras, center, links: &links, swept: &swept, r, rim, step, bite, orbit: (a, 0.0) };
                // Else the lead re-read on the stock as it stands: where
                // its end grazes, the graze joins the pass (walked and
                // read below like any cut) and the return lands on air.
                law.find(a, &[], &piece, None).or_else(|| law.high(&piece)).or_else(|| grown(&law, a, &mut piece, &mut kept, k)).map(|ret| (legs(a, &ret), ret))
            }
            _ => None,
        };
        joins.push(join.map(|(lg, ret)| {
            links.push(lg);
            ret
        }));
        let wk = ras.walk(&piece.pts, r, rim, step);
        if wk.0 > worst.0 {
            worst = wk;
        }
        swept.cut(&piece.pts);
        out.push(piece.pts);
    }
    Joined { passes: out, joins, worst }
}

/// The late join (`LinkLaw::late`) of pass `k`, on a copy of `piece`:
/// kept, and the piece grown by its lead, only where the grown pass
/// crosses no other pass; else `None` (the ladder's retract hop).
fn grown(law: &LinkLaw, a: Point2, piece: &mut Piece, kept: &mut Kept, k: usize) -> Option<Return> {
    let mut trial = piece.clone();
    let ret = law.late(a, &[], &mut trial)?;
    if kept.crosses_but(&trial.pts, k) {
        return None;
    }
    kept.add_as(&trial.pts, k);
    *piece = trial;
    Some(ret)
}

/// The straight run back from `pass`'s start against its first chord,
/// [`LEAD_RUNS`] ramp runs long, when that chord stays in `center`
/// (empty otherwise). The return law still reads it: its descent must
/// read no stock.
fn lead_of(pass: &[Point2], center: &Region) -> Vec<Point2> {
    let (Some(&p0), Some(&p1)) = (pass.first(), pass.get(1)) else { return Vec::new() };
    let l = dist(p0, p1);
    if l.is_nan() || l <= 1e-9 {
        return Vec::new();
    }
    let run = LEAD_RUNS * RAMP_RUN;
    let st = Point2::new(p0.x - (p1.x - p0.x) / l * run, p0.y - (p1.y - p0.y) / l * run);
    if !(center.contains(st) && chord_clears(center, st, p0)) {
        return Vec::new();
    }
    densify(&[st, p0], RAMP_RUN / 4.0, false)
}
