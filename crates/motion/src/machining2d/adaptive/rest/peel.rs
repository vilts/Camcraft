//! One cut depth of rest Adaptive (issue 2031): the remaining material
//! peeled from the cleared side, area by area, one engagement front at
//! a time, every pass walked on the conservative stock raster BEFORE it
//! is kept (`pass`).
//!
//! Provenance: the engagement front of the in-process stock - the
//! cutter centers that bite exactly the step (J. A. Stori and P. K.
//! Wright, "Constant engagement tool path generation for convex
//! geometries", J. Manufacturing Systems 19(3), 2000), taken as offsets
//! of the remaining material inside the legal center area (M. Held, "On
//! the Computational Geometry of Pocket Machining", LNCS 500, Springer
//! 1991); the material a larger cutter leaves a smaller one after Z.
//! Yao, S. K. Gupta and D. S. Nau, Computer-Aided Design 35(9), 2003.
//! The contact angle is the immersion angle of peripheral milling (Y.
//! Altintas, "Manufacturing Automation", 2nd ed., Cambridge 2012, ch.
//! 2), read on the raster as `quality` reads it. The step ladder, the
//! phases, the budget and every refusal are ours; no other CAM
//! product's code or output was consulted.
//!
//! Prime Directive: a pass is kept only when its whole feed - lead and
//! link included - read at most the optimal load on stock that is never
//! read small, stays inside the legal center area and crosses no
//! earlier feed of the depth. Where that cannot be had the depth is
//! refused by name; nothing is cut on a guess.

use crate::geo2d::{Point2, Region};
use crate::load::{deg_at_rim, load_at_rim};
use crate::machining2d::GenError;
use crate::machining2d::pocket_hsm::{PLAN_TOL, components};

use super::super::front::dist;
use super::super::raster::Raster;
use super::super::{BITE_LOAD, GRID_PER_D, STEPS};
use super::budget;
use super::canon::Canon;
use super::entry::Entries;
use super::leads::Anchor;
use super::pass::Fail;
use super::runs::{Run, RunSpec, runs};
use super::stock::Guards;

/// Most passes one depth may hold (the Prime Directive's "never hang").
pub(super) const PASSES_MAX: usize = 2000;

/// Most feed length (tool radii) one depth's trials may walk.
pub(super) const WORK_MAX: f64 = 60_000.0;

/// Most runs one phase of one area may give up on before it moves on.
const DEAD_MAX: usize = 256;

/// The share of the rest material one depth may leave standing in
/// reach (grid crumbs) before its coverage is refused.
pub(super) const COVER_MAX: f64 = 0.02;

/// A plunge is proved clear this far (mm) past the cutter, or four grid
/// cells where that is more.
pub(super) const ENTRY_MARGIN: f64 = 0.1;

/// An area is planned in cells at most this many tool diameters wide
/// (issue 2109): the uniform-grid partition of a plane region (M. de
/// Berg et al., "Computational Geometry", 3rd ed., Springer 2008). A
/// neck's two mouths are then cut phase by phase each in turn, not
/// alternately with a retract between (the two bays' 10 mm neck: 23
/// plunges a depth down to 10).
const TILE_PER_D: f64 = 4.0;

/// One cut depth to plan.
pub(super) struct Level<'a> {
    /// The legal cutter-center area.
    pub center: &'a Region,
    /// What the cutter covers from it.
    pub reach: &'a Region,
    /// The bound on the material standing at this depth (`stock`).
    pub material: &'a Region,
    /// Residue thinner than this is not planned for (`RestStock::film`).
    pub film: f64,
    pub r: f64,
    /// Optimal load, ae/D.
    pub load: f64,
    pub climb: bool,
    pub z: f64,
    pub guards: Guards,
}

/// The plan of one depth: feed polylines, each entered by a plunge at
/// its first point and left by a retract after its last.
#[derive(Clone, Debug, Default)]
pub(super) struct LevelPlan {
    pub chains: Vec<Vec<Point2>>,
    /// Rest material planned on (mm^2, read large).
    pub area: f64,
    /// Share of it left standing in reach.
    pub left: f64,
    /// Worst load a kept pass read (ae/D).
    pub worst: f64,
    pub passes: usize,
    /// Feed in cut and out of cut (mm), and the retract hops
    /// (`budget`).
    pub cut: f64,
    pub air: f64,
    pub hops: usize,
    /// Rest material left (mm^2, within the coverage bound) where no
    /// step held the load, and where: said on the path, never cut over
    /// the load.
    pub held_back: Option<(f64, Point2)>,
}

pub(super) struct Peel<'a> {
    pub(super) lv: &'a Level<'a>,
    pub(super) canon: Canon,
    pub(super) entries: Entries<'a>,
    /// Where the last kept pass ended (the tool stands there, down).
    pub(super) cur: Option<Point2>,
    /// Where earlier passes landed on a wall, and where they left one.
    pub(super) anchors: Vec<Anchor>,
    pub(super) ends: Vec<super::canon::WallAt>,
    /// The stock, read large: what every load reading stands on.
    pub(super) ras: Raster,
    /// What is still to cut: the stock less its film.
    pub(super) goal: Raster,
    /// The same, as a region, cut as the passes are kept.
    pub(super) mat: Region,
    pub(super) rim: f64,
    pub(super) bound: f64,
    seg: f64,
    /// Where the last kept pass started (the same front, one step on).
    pub(super) anchor: Option<Point2>,
    /// The last kept pass from where its run starts on, and that wall
    /// vertex: what the next pass may retrace (`leads`).
    pub(super) back: Option<(Vec<Point2>, super::canon::WallAt)>,
    pub(super) plan: LevelPlan,
    pub(super) worst_deg: f64,
    pub(super) work: f64,
    /// The most serious reason a run of a last phase was not cut.
    unheld: Option<Fail>,
}

/// The radii the room is opened by, phase by phase, widest first: from
/// `widest` down by 15% a phase - at most a step and a half of the
/// optimal width `w`, so the arc that peels a corner never bites much
/// deeper than the phase before - to an eighth of the tool radius, then
/// sharp (which is what clears a corner).
pub(super) fn radii(r: f64, w: f64, widest: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut rho = widest;
    while rho > r / 8.0 && out.len() < 256 {
        out.push(rho);
        rho -= (0.15 * rho).min(1.5 * w).max(1e-6);
    }
    out.push(0.0);
    out
}

/// Plan one depth. No material in reach is an empty plan.
pub(super) fn plan(lv: &Level<'_>) -> Result<LevelPlan, GenError> {
    let d = 2.0 * lv.r;
    let mut ras = Raster::from_region(lv.reach, d / GRID_PER_D, lv.r)?;
    // Only what the cutter can touch: beyond its reach stand the walls.
    let bound = lv.material.intersect(lv.reach)?;
    // Conservative: every cell the bound touches stands (`raster::conservative`).
    let grow = ras.conservative_growth();
    let mut goal = ras.clone();
    ras.keep(&bound.offset(grow)?.intersect(lv.reach)?);
    // What the passes are planned to cut: the bound less its film (an
    // opening by half the film), read large the same way.
    let half = 0.5 * lv.film.clamp(0.0, 0.25 * lv.r);
    let body = if half > 0.0 { bound.offset(-half)?.offset(half)? } else { bound.clone() };
    let mat = body.offset(grow)?.intersect(lv.reach)?;
    goal.keep(&mat);
    let initial = goal.standing();
    if initial == 0 {
        return Ok(LevelPlan::default());
    }
    let areas = components(&mat)?;
    let (h, seg) = (ras.h, lv.r / 8.0);
    let rim = crate::quality::rim_for(d, h);
    let margin = ENTRY_MARGIN.max(4.0 * h);
    let entries = Entries::new(lv.center, &bound, ras.clone(), (lv.r, grow, margin, seg), lv.guards.entry)?;
    // What the passes are planned to cut, as it stands before them: the
    // yardstick of the travel budget.
    let goal0 = goal.clone();
    if !entries.any() {
        return Err(GenError::Invalid(format!(
            "rest Adaptive has no entry with proved clearance at Z{:.3}: the remaining stock proves no position clear for the cutter's whole footprint",
            lv.z
        )));
    }
    let mut p = Peel {
        lv,
        canon: Canon::new(lv.center, seg),
        entries,
        cur: None,
        anchors: Vec::new(),
        ends: Vec::new(),
        ras,
        goal,
        mat,
        rim,
        // The plain Adaptive's planning margin: it covers the grid phase.
        bound: deg_at_rim(lv.load * (1.0 + PLAN_TOL), d, rim),
        seg,
        anchor: None,
        back: None,
        plan: LevelPlan { area: initial as f64 * h * h, ..LevelPlan::default() },
        worst_deg: 0.0,
        work: 0.0,
        unheld: None,
    };
    // Area by area, the nearest next: an area is finished before the
    // tool leaves it. An area is the box of one piece of the material,
    // grown by the tool diameter; a piece that stands free of the walls
    // has no wall to roll off, and its rooms are not opened.
    let mut todo: Vec<(Point2, Point2, bool)> = Vec::new();
    for a in &areas {
        let Some((lo, hi)) = a.bounds() else { continue };
        let free = a.offset(2.0 * h)?.difference(lv.reach)?.is_empty();
        let (lo, hi) = (Point2::new(lo.x - d, lo.y - d), Point2::new(hi.x + d, hi.y + d));
        // In cells no wider than `TILE_PER_D` diameters: a long area's
        // phases are run near where the tool stands, not alternated
        // between its far ends by a retract each (issue 2109).
        let tile = TILE_PER_D * d;
        let (nx, ny) = (((hi.x - lo.x) / tile).ceil().clamp(1.0, 64.0), ((hi.y - lo.y) / tile).ceil().clamp(1.0, 64.0));
        let (w, ht) = ((hi.x - lo.x) / nx, (hi.y - lo.y) / ny);
        for i in 0..nx as usize {
            for j in 0..ny as usize {
                let (x, y) = (lo.x + w * i as f64, lo.y + ht * j as f64);
                todo.push((Point2::new(x, y), Point2::new(x + w, y + ht), free));
            }
        }
    }
    while !todo.is_empty() {
        let from = p.cur.or_else(|| lv.center.bounds().map(|b| b.0)).unwrap_or_default();
        let far = |a: &(Point2, Point2, bool)| dist(from, Point2::new(0.5 * (a.0.x + a.1.x), 0.5 * (a.0.y + a.1.y)));
        let Some(k) = (0..todo.len()).min_by(|&a, &b| far(&todo[a]).total_cmp(&far(&todo[b]))) else { break };
        let (lo, hi, free) = todo.swap_remove(k);
        p.rounds((lo, hi), free)?;
    }
    p.plan.left = p.goal.standing() as f64 / initial as f64;
    p.plan.worst = load_at_rim(p.worst_deg, d, rim);
    if p.plan.left > COVER_MAX {
        // What stopped a run names the refusal; else the coverage does.
        return Err(p.unheld.and_then(|why| p.refusal(why)).unwrap_or_else(|| {
            GenError::Invalid(format!(
                "rest Adaptive cannot prove its coverage at Z{:.3}: {:.0}% of the rest material in reach would be left standing - choose Waterline or Offset rings",
                lv.z,
                p.plan.left * 100.0
            ))
        }));
    }
    let t = budget::travel(goal0, &p.plan.chains, lv.r, 2.0 * lv.r * BITE_LOAD);
    budget::check(&t, areas.len(), lv.z)?;
    (p.plan.cut, p.plan.air, p.plan.hops) = (t.cut, t.air, t.hops);
    if let Some(Fail::Load(_, at)) = p.unheld {
        p.plan.held_back = Some((p.goal.standing() as f64 * h * h, at));
    }
    Ok(p.plan)
}

impl Peel<'_> {
    /// The rounds of one area: per phase, the nearest run that still
    /// bites, at the widest step of the ladder that holds the load.
    fn rounds(&mut self, area: (Point2, Point2), free: bool) -> Result<(), GenError> {
        let r = self.lv.r;
        // No wider than the center area can hold a disc of.
        let widest = self.lv.center.bounds().map_or(r, |(lo, hi)| 0.5 * (hi.x - lo.x).min(hi.y - lo.y)).min(16.0 * r);
        let phases = if free { vec![0.0] } else { radii(r, 2.0 * r * self.lv.load, widest) };
        for (phase, &rho) in phases.iter().enumerate() {
            // Runs no step could cut in this phase: never offered again
            // in it (the plan cannot spin on them).
            let mut dead: Vec<Point2> = Vec::new();
            let mut failed: Option<Fail> = None;
            loop {
                if self.plan.passes >= PASSES_MAX || self.work > WORK_MAX * r {
                    return Err(GenError::Invalid(format!(
                        "rest Adaptive ran past its planning budget at Z{:.3} ({} passes kept, {:.0} mm of trial feed) - choose Waterline or Offset rings",
                        self.lv.z, self.plan.passes, self.work
                    )));
                }
                if self.mat.is_empty() {
                    return Ok(());
                }
                if dead.len() >= DEAD_MAX {
                    break;
                }
                let mut kept = None;
                let mut tried: Option<(Point2, Fail)> = None;
                for f in STEPS {
                    let e = r - 2.0 * r * self.lv.load * f;
                    let sp = RunSpec {
                        center: self.lv.center,
                        canon: &self.canon,
                        ras: &self.goal,
                        r,
                        graze: 2.0 * r * BITE_LOAD,
                        seg: self.seg,
                        margin: 0.5 * r,
                        ride: 0.0,
                        rho,
                        climb: self.lv.climb,
                    };
                    // (A narrower step bites a subset: none at the widest, none at all.)
                    let here = |run: &Run| run.v.iter().any(|v| v.p.x >= area.0.x && v.p.x <= area.1.x && v.p.y >= area.0.y && v.p.y <= area.1.y);
                    let cands: Vec<Run> = runs(&self.mat, e, &sp)?.into_iter().filter(here).collect();
                    let Some(run) = self.nearest(cands, &dead) else { break };
                    let Some(start) = run.v.first().map(|v| v.p) else { continue };
                    match self.attempt(&run) {
                        Ok(t) => {
                            kept = Some(t);
                            break;
                        }
                        Err(why) => {
                            tried = Some((tried.map_or(start, |t| t.0), tried.map_or(why, |t| t.1.worse(why))));
                            // (A narrower step cuts a subset of nothing.)
                            if why == Fail::Idle {
                                break;
                            }
                        }
                    }
                }
                match (kept, tried) {
                    (Some(t), _) => self.commit(t)?,
                    (None, Some((start, why))) => {
                        dead.push(start);
                        failed = Some(failed.map_or(why, |f| f.worse(why)));
                    }
                    // Nothing bites any more at this radius.
                    (None, None) => break,
                }
            }
            // What the last (sharp) phase could not cut is left standing:
            // the coverage bound judges it (`plan`).
            if phase + 1 == phases.len()
                && let Some(why) = failed
            {
                self.unheld = Some(self.unheld.map_or(why, |u| u.worse(why)));
            }
        }
        Ok(())
    }

    /// The run to cut next: the one starting nearest where the last
    /// pass started (the same front, one step on) and where the tool
    /// stands; none of the dead ones.
    fn nearest(&self, cands: Vec<Run>, dead: &[Point2]) -> Option<Run> {
        let r = self.lv.r;
        let origin = self.lv.center.bounds().map_or(Point2::new(0.0, 0.0), |b| b.0);
        let (anchor, cur) = (self.anchor.unwrap_or(origin), self.cur.unwrap_or(origin));
        let near = |run: &Run, q: Point2| {
            let pts = run.v.iter().take(if run.closed { usize::MAX } else { 1 });
            pts.map(|v| dist(v.p, q)).fold(f64::INFINITY, f64::min)
        };
        cands
            .into_iter()
            .filter(|run| run.v.len() >= 2 && !dead.iter().any(|&d| near(run, d) < 0.5 * r))
            .map(|run| (near(&run, anchor) + 0.25 * near(&run, cur), run))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|b| b.1)
    }
}
