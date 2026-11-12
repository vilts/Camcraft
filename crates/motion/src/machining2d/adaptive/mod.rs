//! HSM adaptive clearing for one planar area (M11.4 C; closes the
//! reopened M2.1 trochoidal/corner-contact items and M2.4's 2D
//! Adaptive spec). The invariant is the load, not the step: every cut
//! the planner emits was measured on its in-process stock (`raster`,
//! the yardstick's own contact law, `quality`) before it was kept.
//!
//! Round 2: where the whole area is one star cell from the entry (the
//! entry is put in its kernel, `kernel`), the plan is ONE morph spiral
//! (`morph`, `star`: Held & Spielberger 2009; Bieterman & Sandstrom
//! 2003) from the orbit to the walls - no link, no crossing, every
//! revolution's step chosen on the stock. Round 3: every other area -
//! channels, islands (cut open by `slit`) - is ONE spiral too, over the
//! cleared area grown ring by ring (`grow`, `rings`). Round 4: that
//! spiral is cut only where it bites, its air returns lifted over the
//! floor (`lift`, `route`). Only where no spiral can be built do the
//! engagement fronts below plan it; a plan that would cross itself is
//! refused (`guard`, the verifier's law first).
//!
//! The fronts' plan, in order:
//! 1. **Bore and spiral** - from the entry's own orbit, each revolution
//!    morphs onto the next engagement front, the cutter centers `r - a`
//!    from the stock still standing (Stori & Wright 2000, J. Manuf.
//!    Systems 19(3), on the in-process stock), riding the walls where
//!    the front met them (`loops`), as ONE spiral (`spiral`: Held &
//!    Spielberger 2009); the step shrinks where the loop is tight.
//! 2. **Open rounds** (`rounds`) - once the cleared area's boundary is
//!    mostly wall, its nearest cutting run is cut each round, one front
//!    step on, entered and left on Dubins links (`link`); in a channel
//!    that is a crescent arc and a return loop - trochoidal motion
//!    (Otkur & Lazoglu 2007, "Trochoidal milling", Int. J. Mach. Tools
//!    Manuf. 47).
//! 3. **Wall lap** - the strip left on the walls in one rounded climb
//!    lap (`wall`).
//!
//! Where the bound cannot be held, the least overload up to
//! `OVERLOAD_CAP` x the load is cut and named; past it the stock is
//! refused and left (and `pocket_hsm` falls back to Offset rings when
//! too much is left). Every loop is capped; the plan cannot hang.

mod budget;
mod core;
mod front;
mod grow;
mod guard;
mod kernel;
mod lift;
mod lift_link;
mod lift_split;
mod link;
mod loops;
// Issue #5: the trochoidal fallback (`pocket_hsm`: spiral -> trochoid ->
// rings), its medial line, loops and plan.
mod medial;
mod morph;
mod near;
mod raster;
mod rings;
mod rounds;
mod route;
mod slit;
mod spiral;
mod star;
mod tidy;
mod troch_plan;
mod trochoid;
mod wall;

use crate::geo2d::{Point2, Region};
use crate::load::{deg_at_rim, load_at_rim};

pub(crate) use kernel::{central_point, preferred_seed, widest_point};
pub(crate) use star::corner_radius;
use star::orbit_radius;
pub(crate) use guard::first_crossing;
pub(crate) use troch_plan::{plan as plan_trochoid, seed as trochoid_seed};

pub(crate) use lift::Travel;
pub(crate) use lift_link::Return;
pub(crate) use loops::boundary_dist;
/// The plan's chord sagitta (`morph`): the finest tessellation the
/// pocket family posts, which `quality` walks arcs at.
pub(crate) use morph::SAG as PLAN_SAG;

use super::GenError;
use raster::Raster;

/// The worst contact angle a trial walk read, and where.
type Worst = (f64, Point2);
/// An open round on trial: the cut, its link, the stock, the reading.
type RoundTrial = (Vec<Point2>, Option<Vec<Point2>>, Raster, Worst);
/// A wall lap on trial, with the corner rest its rounding leaves.
type LapTrial = (Vec<Point2>, Option<Vec<Point2>>, Raster, Worst, Option<(Point2, f64)>);

/// Step ladder: fractions of the optimal width a round may try.
const STEPS: [f64; 8] = [1.0, 0.85, 0.7, 0.55, 0.45, 0.35, 0.28, 0.22];

/// The spiral's deeper ladder: out of a small bore the convex-path law
/// (Stori & Wright) wants steps far under the nominal width, and a
/// closed revolution there is cheap to plan.
const SPIRAL_STEPS: [f64; 11] = [1.0, 0.85, 0.7, 0.55, 0.45, 0.35, 0.28, 0.22, 0.15, 0.1, 0.07];

/// Where no step of the ladder holds the load, the least overload up
/// to this multiple of it is cut (and named) rather than left standing.
const OVERLOAD_CAP: f64 = 2.0;

/// A contact standing for less than this load (ae/D) is a graze.
const BITE_LOAD: f64 = 0.005;

/// Rounds are capped (the Prime Directive's "never hang").
const MAX_ROUNDS: usize = 3000;

/// Planner grid: the tool diameter over this - the heavy lane's
/// measuring grid, so the plan and the yardstick read one stock.
const GRID_PER_D: f64 = 120.0;

/// The morph spiral's last ring rounds the walls' corners at this
/// share of the tool radius (the named corner scallop).
pub(crate) const CORNER_PER_R: f64 = 0.5;

/// The growth rings' wall margin, tried widening (`grow`).
const MARGIN_SCALES: [f64; 3] = [1.0, 1.6, 2.4];

/// A star plan's walk is smoothed where it turns past this (degrees):
/// under the 5-degree in-cut break law with a margin.
const STAR_SMOOTH_DEG: f64 = 4.0;

/// What one planar area is planned with.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AdaptiveSpec {
    /// Tool radius.
    pub r: f64,
    /// Optimal load, ae/D.
    pub load: f64,
    /// Planning tolerance on the contact angle (fraction).
    pub tol: f64,
    pub climb: bool,
    /// Where the entry stood the tool, the bore it left (radius of the
    /// cleared disc, centered at `seed`) and the helix radius it orbited.
    pub seed: Point2,
    pub start: Point2,
    pub bore: f64,
    pub helix_r: f64,
    /// Lead / link arc radius (mm): the smoothing radius.
    pub smooth: f64,
}

/// The plan: cutter-center walks (each starts where the previous one
/// lifted; the first where the entry stood) and its notes.
#[derive(Clone, Debug, Default)]
pub(crate) struct AdaptivePlan {
    pub passes: Vec<Vec<Point2>>,
    /// Per pass: reached from the last by a microlift return (`lift`)
    /// rather than the ladder's own pass link (empty: none is).
    pub joins: Vec<Option<Return>>,
    pub notes: Vec<String>,
    /// Stock the plan leaves standing in the reach (mm^2, planner grid).
    pub rest: f64,
    /// Its travel, read on the planner's stock (`lift::travel`).
    pub travel: lift::Travel,
    /// Refused past the planning budget (`budget`), by this name: no
    /// passes, the area is the Offset rings'.
    pub refused: Option<String>,
}

struct Planner<'a> {
    center: &'a Region,
    s: AdaptiveSpec,
    ras: Raster,
    mat: Region,
    rim: f64,
    step: f64,
    seg: f64,
    /// The contact angle a kept cut may read on `rim` (degrees).
    bound: f64,
    /// The contact angle under which a pass is grazing, not biting.
    bite: f64,
    /// The most a fallback cut may read where no step holds the bound.
    cap: f64,
    walk: Vec<Point2>,
    plan: AdaptivePlan,
    worst: (f64, Point2),
    /// Where the growth rings' closing laps begin in the walk (`grow`).
    laps_from: usize,
    /// ... and where the last lap begins.
    fin_from: usize,
    /// The growth rings' planning work so far: the length and geometry
    /// of every revolution tried, held to the [`budget`] laws.
    work: budget::Work,
    /// Why the plan was refused outright (the pocket hands the area to
    /// the Offset rings by this name), or `None`.
    refused: Option<String>,
}

/// Plan the area `reach` (the material the cutter can reach) from the
/// legal center area `center`.
pub(crate) fn plan(reach: &Region, center: &Region, s: &AdaptiveSpec) -> Result<AdaptivePlan, GenError> {
    plan_within(reach, center, s, budget::Work::default())
}

/// [`plan`], its growth held to the `work` budget's limits.
fn plan_within(reach: &Region, center: &Region, s: &AdaptiveSpec, work: budget::Work) -> Result<AdaptivePlan, GenError> {
    if !(s.r > 0.0 && s.load > 0.0 && s.bore >= 0.0 && s.smooth >= 0.0 && s.tol > -0.5) {
        return Err(GenError::Invalid("adaptive plan needs positive radius and load".into()));
    }
    let h = 2.0 * s.r / GRID_PER_D;
    let ras = Raster::from_region(reach, h, s.r)?;
    // The reading law the measuring stock uses (`quality::rim_for`): the
    // rim a cell and a half outside the cutter, on the half facing the
    // motion, and the bound is the angle the optimal load's straight
    // wall reads on that rim (`load::deg_at_rim`).
    let d = 2.0 * s.r;
    let rim = crate::quality::rim_for(d, ras.h);
    let mut p = Planner {
        center,
        s: *s,
        ras,
        mat: reach.clone(),
        rim,
        step: s.r / 10.0,
        seg: s.r / 8.0,
        bound: deg_at_rim(s.load * (1.0 + s.tol), d, rim),
        bite: deg_at_rim(BITE_LOAD, d, rim),
        cap: deg_at_rim(s.load * OVERLOAD_CAP, d, rim),
        walk: vec![s.start],
        plan: AdaptivePlan::default(),
        worst: (0.0, s.start),
        laps_from: usize::MAX,
        fin_from: usize::MAX,
        work,
        refused: None,
    };
    if s.bore > 0.0 {
        p.ras.cut(s.seed, s.seed, s.bore);
        p.mat = p.mat.difference(&Region::sweep(&[vec![s.seed]], s.bore)?)?;
    }
    let ras0 = p.ras.clone();
    let star = star::star_cell(center, s)?;
    let mut fronts = false;
    if let Some(star) = &star {
        p.orbit();
        let from = p.walk.len();
        p.morph(star, orbit_radius(s))?;
        // The morph seams (the circle phase handing over to the blend,
        // the opening rows' radius ladder) kink the revolutions a few
        // degrees: spread out like the fronts' (`tidy::smooth_walk`),
        // the entry orbit left as it is (its helix lands on it).
        if let Some(spiral) = p.walk.get_mut(from.saturating_sub(1)..) {
            tidy::smooth_walk(spiral, center, STAR_SMOOTH_DEG, 24);
        }
    } else {
        // Round 3: the area grown as one ring family (channels, islands);
        // where it cannot be, the engagement fronts plan it.
        let (walk, notes, ras, worst) = (p.walk.clone(), p.plan.notes.len(), p.ras.clone(), p.worst);
        let mut grown = grow::Grown::Unfit;
        for (k, scale) in MARGIN_SCALES.iter().enumerate() {
            (p.walk, p.ras, p.worst) = (walk.clone(), ras.clone(), worst);
            p.plan.notes.truncate(notes);
            let ring0 = p.orbit();
            grown = p.grow(&ring0, (*scale, k + 1 == MARGIN_SCALES.len()))?;
            if grown != grow::Grown::Short {
                break;
            }
        }
        if let Some(why) = p.refused.take() {
            // Past the budget: no plan, the refusal named (`pocket_hsm`
            // hands the area to the Offset rings).
            return Ok(AdaptivePlan { refused: Some(why), notes: std::mem::take(&mut p.plan.notes), ..AdaptivePlan::default() });
        }
        if grown != grow::Grown::Done {
            (p.walk, p.ras, p.worst) = (walk, ras, worst);
            p.plan.notes.truncate(notes);
            let heading = p.core_spiral()?;
            p.open_rounds(heading)?;
            fronts = true;
        }
    }
    // Round 4: the spiral (grown or morphed) cut where it bites, its air
    // returns lifted (`lift`).
    let lifted = if fronts { None } else { p.lift_returns(&ras0)? };
    let mut out = std::mem::take(&mut p.plan);
    match lifted {
        Some(l) => {
            out.notes.push(lift::note(&l));
            out.rest = l.ras.standing() as f64 * l.ras.h * l.ras.h;
            p.worst = l.worst;
            (out.passes, out.joins) = (l.passes, l.joins);
        }
        None => {
            out.rest = p.ras.standing() as f64 * p.ras.h * p.ras.h;
            if p.walk.len() >= 2 {
                out.passes.push(std::mem::take(&mut p.walk));
            }
        }
    }
    out.travel = lift::travel(&ras0, &out.passes, &out.joins, (s.r, rim, p.step, p.bite));
    if fronts {
        // Kinks the morph seams and blend joints left are spread out
        // (inside the center area only), so the walk reads smooth.
        for pass in &mut out.passes {
            tidy::smooth_walk(pass, center, 6.0, 24);
        }
    }
    let worst = load_at_rim(p.worst.0, d, rim);
    if worst > s.load * (1.0 + s.tol.max(0.0)) {
        out.notes.push(format!(
            "adaptive: worst planned load {:.1}% of D at ({:.2}, {:.2}) exceeds the {:.1}% optimal load by {:.0}%",
            worst * 100.0,
            p.worst.1.x,
            p.worst.1.y,
            s.load * 100.0,
            (worst / s.load - 1.0) * 100.0
        ));
    }
    Ok(out)
}

impl Planner<'_> {
    fn width(&self, f: f64) -> f64 {
        2.0 * self.s.r * self.s.load * f
    }

    fn note_worst(&mut self, w: (f64, Point2)) {
        if w.0 > self.worst.0 {
            self.worst = w;
        }
    }

    fn take(&mut self, cut: &[Point2]) -> Result<(), GenError> {
        let sw = Region::sweep(&[cut.to_vec()], self.s.r)?;
        // Slivers under half a grid cell, vertices within a tenth of
        // one and dust under four cells go (`tidy`): the planner's own
        // resolution, below what the measuring stock can see.
        let h = self.ras.h;
        self.mat = tidy::tidy(&self.mat.difference(&sw)?, 0.25 * h, 0.1 * h, 4.0 * h * h)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod lift_tests;
#[cfg(test)]
mod troch_tests;
