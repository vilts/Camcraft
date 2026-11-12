//! Growth rings (M11.4 C round 3): where the area is no round star cell
//! from the entry - a channel, an island, a long slot - it is still
//! cleared by ONE outward spiral, from the entry's orbit to the walls,
//! with no link, no lift and no crossing. With the cut climb-only and
//! staying down, that is the only crossing-free shape there is: every
//! revolution must enclose everything cut before it. The rings it morphs
//! between are the cleared area grown step by step:
//!
//!   R(n+1) = open_rho( (R(n) (+) disc(s)) n D(eps(n+1)) ),
//!
//! a geodesic dilation of the cleared area inside the reachable area
//! `D` (islands cut open by `slit`, so the fronts wrap each island as a
//! C and close on the slit's faces), kept `eps` off the walls with `eps`
//! shrinking every revolution and filleted by a morphological opening.
//! Dilation, intersection and opening all preserve inclusion, so the
//! rings are strictly nested - a gap of at least `min(s, deps)` between
//! neighbours - and every revolution, a blend between two neighbours,
//! lies in its own band: none can cross another. In a channel the front
//! is a flat face swept from wall to wall with filleted ends (the
//! crescent of trochoidal milling, Otkur & Lazoglu 2007, "Trochoidal
//! milling", Int. J. Mach. Tools Manuf. 47; Ibaraki et al. 2010, the
//! constant-engagement trochoid), the long sides riding the walls in the
//! cleared stock as the trochoid's return loop does. Provenance: the
//! spiral between nested rings, Held & Spielberger (2009, CAD 41(7));
//! pocket fronts as offsets of the cleared area and the island bridge,
//! Held (1991, *On the Computational Geometry of Pocket Machining*, LNCS
//! 500); the geodesic dilation and opening, Serra (1982, *Image Analysis
//! and Mathematical Morphology*); the step of each revolution chosen on
//! the in-process stock, Stori & Wright (2000). Schedules are ours.

use crate::geo2d::{Loop, Point2, Region};

use super::raster::Raster;
use super::rings::{ARC_TOL, LAND, NARROW, morph_rev, one_loop, open, pick, piece, widest_opening};
use super::{CORNER_PER_R, GenError, Planner, Worst, budget, orbit_radius, slit};

/// The wall margin the rings start at (mm).
const EPS0: f64 = 0.4;

/// The margin closes this much a revolution (mm: neighbouring wall runs
/// stay a few lattice units of the boolean engine apart) down to
/// [`RESERVE`], kept for the easing laps; past it the plan is made again
/// from a wider margin, or - the widest spent - closes by a [`TAIL`]
/// share of what is left above half the reserve.
const GAP: f64 = 0.0025;
const RESERVE: f64 = 0.05;
const TAIL: f64 = 0.1;

/// The last lap's own margin (mm); the easing laps split the reserve
/// down to it.
const FINAL: f64 = 0.005;

/// Each fillet level is this share of the last.
const LEVEL: f64 = 0.75;


/// Revolutions are capped (the Prime Directive's "never hang").
const MAX_REVS: usize = 800;

/// A revolution tries at most this many growth steps.
const STEP_TRIES: usize = 7;

/// The step may grow by at most this factor over the last.
const GROW: f64 = 1.3;

/// No front steps further than this many optimal widths a revolution.
const CAP: f64 = 1.5;

/// The fillet eases down by this much a revolution once grown (mm).
const EASE: f64 = 0.15;

/// A grown ring on trial: the step, the region, the ring (rotated to
/// the walk), the revolution, the stock, its worst reading, and whether
/// the reachable area is now whole.
type GrowTrial = (f64, Region, Vec<Point2>, Vec<Point2>, Raster, Worst, bool);

/// A grown ring whose truncated walk overloaded: a [`GrowTrial`] without
/// its stock and reading (both partial, so dropped).
type Tried = (f64, Region, Vec<Point2>, Vec<Point2>, bool);

/// A later (smaller) step replaces the kept overload only when its whole
/// revolution reads more than this share less: a near tie keeps the
/// larger step and the fewer revolutions.
const SLACK: f64 = 1.02;

/// How a growth plan ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Grown {
    /// The area is cleared.
    Done,
    /// The area cannot be grown as one ring family.
    Unfit,
    /// The wall margin ran out first: plan again from a wider one.
    Short,
    /// Past the planning budget (`budget`): the plan is refused by the
    /// name left in `Planner::refused`, never planned otherwise.
    Refused,
}


impl Planner<'_> {
    /// The growth-ring spiral over the whole center area (islands cut
    /// open), from the orbit `ring0` the walk stands on. `false` when the
    /// area cannot be grown as one ring family (the caller plans it
    /// otherwise).
    pub(super) fn grow(&mut self, ring0: &[Point2], (scale, last): (f64, bool)) -> Result<Grown, GenError> {
        let s = self.s;
        let a0 = orbit_radius(&s);
        let Some(dom) = slit::open_islands(self.center, s.seed, a0 + 0.5)? else { return Ok(Grown::Unfit) };
        // The last lap's corners: half the tool radius, or under the
        // narrowest half-width of the area.
        let Some(rho_c) = pick(CORNER_PER_R * s.r, |p| Ok(one_loop(&dom.offset(-p / NARROW)?)))? else { return Ok(Grown::Unfit) };
        let d0 = open(&dom, rho_c)?;
        if !one_loop(&d0) || !d0.contains(s.seed) {
            return Ok(Grown::Unfit);
        }
        // The wall margin (widened by the caller's ladder when spent).
        let clear = super::loops::boundary_dist(&d0, s.seed);
        // A channel too long for one spiral is refused before a single
        // revolution is planned (`budget`).
        let len = budget::channel_len(d0.area(), clear);
        let (least, revs) = budget::least_work(len, CAP * self.width(1.0), s.r);
        if least > budget::LEAST_WORK_MAX {
            self.refused = Some(budget::too_long(2.0 * (clear + s.r), len, revs));
            return Ok(Grown::Refused);
        }
        let eps0 = (EPS0 * scale).min(0.25 * s.r).min(clear - a0 - 0.01);
        if eps0 <= 4.0 * GAP {
            return Ok(Grown::Unfit);
        }
        // The fronts' fillet: as round as the narrowest channel lets it
        // be (a channel's front is then the trochoid's crescent, met by
        // the wall runs tangentially); the rings first grow as circles up
        // to it, so every ring is open at that radius.
        let d_hi = d0.offset_fine(-eps0, ARC_TOL)?;
        let Some(rho_hi) = widest_opening(&d_hi, (clear - eps0 - 0.01).min(s.r))? else { return Ok(Grown::Unfit) };
        // The fillet ladder: the widest the entry's room holds first (wide
        // areas grow with round fronts that meet the walls gently and
        // never squeeze into a channel), stepped down to `rho_hi` - the
        // narrowest channel's - each level grown whole before the next.
        let top = (clear - eps0 - 0.01).min(s.r).max(rho_hi);
        let mut levels = vec![top];
        while let Some(&l) = levels.last()
            && l * LEVEL > rho_hi
        {
            levels.push(l * LEVEL);
        }
        levels.retain(|&l| l > rho_hi + 1e-9);
        levels.push(rho_hi);
        let mut lvl = 0usize;
        let Some((mut reg, _)) = piece(&Region::new(vec![Loop::new(ring0[..ring0.len().saturating_sub(1)].to_vec())?])?, s.seed, s.climb)? else {
            return Ok(Grown::Unfit);
        };
        let (mut ring, mut a, mut eps) = (ring0.to_vec(), a0, eps0);
        let mut step = 0.25 * self.width(1.0);
        let mut overloaded = false;
        let mut grown = false;
        for rev_k in 0..MAX_REVS {
            if let Some(why) = self.work.over(s.r, rev_k) {
                self.refused = Some(why);
                return Ok(Grown::Refused);
            }
            let disc = a < levels[0] - 1e-9;
            // The margin closes as the fronts travel (it is spent when they
            // have gone the farthest reach, detours included), and never by
            // more than a share of itself: neighbouring wall runs stay apart.
            let eps_n = if disc {
                eps
            } else if eps - GAP >= RESERVE {
                eps - GAP
            } else if !last {
                // The margin is spent before the fronts are done: planned
                // again from a wider one (the caller's ladder).
                return Ok(Grown::Short);
            } else {
                eps - GAP.min(TAIL * (eps - 0.5 * RESERVE))
            };
            let d_eps = d0.offset_fine(-eps_n, ARC_TOL)?;
            self.work.add_geometry(&[&d_eps]);
            let rho_k = levels[lvl];
            // This level is whole when the cleared area is all of the
            // margin's area the level's fillet reaches from the seed.
            let whole = if disc { f64::INFINITY } else { piece(&open(&d_eps, rho_k)?, s.seed, s.climb)?.map_or(f64::INFINITY, |p| p.0.area()) };
            let trial = |p: &Planner<'_>, g: f64, limit: f64| -> Result<Option<GrowTrial>, GenError> {
                let y = if disc {
                    let c = super::rings::circle(s.seed, (a + g).min(levels[0]), ring[0], s.climb, p.seg);
                    Region::new(vec![Loop::new(c[..c.len() - 1].to_vec())?])?
                } else {
                    open(&reg.offset_fine(g, ARC_TOL)?.intersect(&d_eps)?, rho_k)?
                };
                p.work.add_geometry(&[&reg, &y]);
                let Some((yr, b)) = piece(&y, s.seed, s.climb)? else { return Ok(None) };
                let done = !disc && whole - yr.area() < 1e-4 * whole.max(1.0);
                // Landed on the ring tangentially (`LAND`): the seam
                // turns by the ring's chord angle, whatever the next
                // revolution's step.
                let (rev, b) = morph_rev(&ring, &b, LAND);
                p.work.add_len(super::front::len(&rev));
                let mut ras = p.ras.clone();
                let w = ras.walk_until(&rev, (s.r, p.rim, p.step), limit);
                Ok(Some((g, yr, b, rev, ras, w, done)))
            };
            let (kept, over) = self.step_search(trial, step)?;
            overloaded |= over;
            // Nothing grows any more: the last laps take what is left.
            let Some((g, yr, b, rev, ras, w, done)) = kept else { break };
            self.note_worst(w);
            self.ras = ras;
            self.push_walk(&rev);
            if disc {
                a = (a + g).min(levels[0]);
            }
            (reg, ring, eps, step) = (yr, b, eps_n, g.max(1e-3));
            if done {
                if lvl + 1 == levels.len() {
                    grown = true;
                    break;
                }
                lvl += 1;
            }
        }
        // The fillet eased down to the last lap's corner radius, a step a
        // revolution (the corner wedge each one cuts stays thin), then
        // onto the reachable area's own boundary and once round it.
        // The closing laps below ride the walls whole (`lift` keeps them).
        self.laps_from = self.walk.len();
        let mut rho = rho_hi;
        let laps = ((rho_hi - rho_c) / EASE).ceil().max(0.0);
        let de = (eps - FINAL) / (laps + 1.0);
        while grown && rho > rho_c + 1e-9 {
            rho = (rho - EASE).max(rho_c);
            eps -= de;
            let y = open(&d0.offset_fine(-eps, ARC_TOL)?, rho)?;
            let Some((_, b)) = piece(&y, s.seed, s.climb)? else { break };
            let (rev, b) = morph_rev(&ring, &b, 0.0);
            self.cut_run(&rev);
            ring = b;
        }
        // The last lap a hair off the walls: it never lies on another.
        let Some((_, fin)) = piece(&open(&d0.offset_fine(-FINAL.min(0.5 * eps), ARC_TOL)?, rho_c)?, s.seed, s.climb)? else {
            return Ok(Grown::Unfit);
        };
        let (rev, fin) = morph_rev(&ring, &fin, 0.0);
        self.fin_from = self.walk.len();
        self.cut_run(&rev);
        // Grown whole and eased, the last ring is microns off the final
        // one and the morph onto it is the last lap; a lap round it again
        // would lie on the morph's own end. Short of whole, the lap takes
        // what the morph's start left.
        if !grown {
            self.cut_run(&fin);
        }
        if overloaded {
            self.plan.notes.push(format!(
                "adaptive: growing the cleared area from ({:.2}, {:.2}) no step held the load somewhere - the least overload was cut",
                s.seed.x, s.seed.y
            ));
        }
        self.note_corners(&d0, rho_c);
        Ok(Grown::Done)
    }

    /// Append a run to the walk, dropping a repeat of the joint.
    pub(super) fn push_walk(&mut self, run: &[Point2]) {
        for &p in run {
            if self.walk.last().is_none_or(|&l| super::front::dist(l, p) > 1e-6) {
                self.walk.push(p);
            }
        }
    }

    /// Walk `run` on the stock whole and keep it.
    fn cut_run(&mut self, run: &[Point2]) {
        let mut ras = self.ras.clone();
        let w = ras.walk(run, self.s.r, self.rim, self.step);
        self.note_worst(w);
        self.ras = ras;
        self.push_walk(run);
    }

    /// The largest growth step (from `step` grown by [`GROW`] down, by
    /// bisection) whose revolution holds the load; where none does, the
    /// least overload by [`least_whole`], walked whole (`true`: named by
    /// the caller).
    fn step_search(
        &self,
        trial: impl Fn(&Planner<'_>, f64, f64) -> Result<Option<GrowTrial>, GenError>,
        step: f64,
    ) -> Result<(Option<GrowTrial>, bool), GenError> {
        let mut hi = (step * GROW).min(CAP * self.width(1.0));
        let (mut lo, mut cand) = (0.0f64, hi);
        let (mut good, mut tries): (Option<GrowTrial>, Vec<Tried>) = (None, Vec::new());
        for k in 0..STEP_TRIES {
            match trial(self, cand, self.bound)? {
                Some(t) if t.5.0 <= self.bound => {
                    lo = cand;
                    good = Some(t);
                    if k == 0 {
                        break;
                    }
                }
                Some((g, yr, b, rev, _, _, done)) => {
                    hi = cand;
                    tries.push((g, yr, b, rev, done));
                }
                None => hi = cand,
            }
            cand = if good.is_some() { 0.5 * (lo + hi) } else { hi * 0.6 };
            if cand < 1e-3 || (good.is_some() && hi - lo < 0.08 * hi) {
                break;
            }
        }
        if good.is_some() {
            return Ok((good, false));
        }
        // Issue #845: the tries stop at their first reading past the
        // bound, which wherever the overload does not grow with the step
        // reads the same for every step - the least overload is chosen on
        // each revolution's WHOLE walk instead. Its length was charged to
        // the growth budget when the trial built it (`add_len` before the
        // truncated walk), and no geometry is rebuilt, so the re-walks
        // add no growth work of their own (decision 845).
        let walk = (self.s.r, self.rim, self.step);
        let Some((i, ras, w)) = least_whole(&self.ras, tries.iter().map(|t| t.3.as_slice()), walk) else {
            return Ok((None, false));
        };
        match tries.into_iter().nth(i) {
            Some((g, yr, b, rev, done)) => Ok((Some((g, yr, b, rev, ras, w, done)), true)),
            None => Ok((None, false)),
        }
    }

    /// Name the corner scallop the last lap's rounding (`rho`) leaves:
    /// how far the center area's corners stand off `d0`.
    fn note_corners(&mut self, d0: &Region, rho: f64) {
        let pulled = self
            .center
            .loops()
            .iter()
            .flat_map(|l| l.points().iter())
            .filter(|&&v| !d0.contains(v))
            .map(|&v| (super::loops::boundary_dist(d0, v), v))
            .fold((0.0f64, Point2::new(0.0, 0.0)), |a, b| if b.0 > a.0 { b } else { a });
        if pulled.0 > 1e-3 {
            self.plan.notes.push(format!(
                "adaptive: wall corners rounded at R{rho:.2} to keep the load - up to {:.2} mm corner scallop (worst at ({:.2}, {:.2})) left for the finishing pass",
                pulled.0, pulled.1.x, pulled.1.y
            ));
        }
    }
}

/// Of overloaded revolutions (largest step first), the one whose WHOLE
/// walk on `ras` reads the least: its index, the stock it leaves and its
/// reading. A later revolution replaces the kept one only when it reads
/// more than [`SLACK`] less, and each walk is abandoned at the first
/// reading that can no longer win - so every revolution is walked at most
/// once more, and usually only part way.
fn least_whole<'a>(ras: &Raster, revs: impl Iterator<Item = &'a [Point2]>, walk: (f64, f64, f64)) -> Option<(usize, Raster, Worst)> {
    let mut best: Option<(usize, Raster, Worst)> = None;
    for (i, rev) in revs.enumerate() {
        let limit = best.as_ref().map_or(f64::INFINITY, |b| b.2.0 / SLACK);
        let mut r = ras.clone();
        let w = r.walk_until(rev, walk, limit);
        if best.is_none() || w.0 < limit {
            best = Some((i, r, w));
        }
    }
    best
}

#[cfg(test)]
#[path = "grow_tests.rs"]
mod tests;
