//! The 3-D rough's HSM adaptive levels (M11.4 owed "Pocket3D port of
//! the same planner"): under [`PocketStrategy::Adaptive`] every level
//! whose target is a closed planar area is planned by the planar HSM
//! planner (`machining2d::adaptive`, through `pocket_hsm`) - the spiral
//! out of the entry bore, the growth revolutions and ring bands, the
//! microlift returns, the hop budget, the 5 degree kink law and the
//! proved arc output - exactly as the 2D Adaptive pocket plans its one
//! area, at the same load (ae/D). The ladder keeps everything else: the
//! level law and its computed islands (`rest.rs`), the rest-material
//! mask, the entry and link arms (`descent.rs`) - the plan only tells
//! the entry where it bored (the spot the first pass starts on and the
//! helix radius it fits there, `LevelPlan::params`) - and the verify
//! that follows.
//!
//! A level the planner refuses by name (an island or channel area past
//! its hop or rest budget, a plan that would cross itself) is cut by
//! the Offset rings at the load step and SAID per level, naming its Z;
//! the rest pass and the offset shells keep the rings too, said once:
//! their proofs are the planes' (a rest target is the simulation's
//! crumbs, entered by plunges; a shell lifts every vertex off the plane
//! the microlift returns were proved on). On the rotary wrap a level too
//! narrow for the asked helix is refused before planning, named with
//! both radii (issue #55, [`NARROW_HELIX`]).
//!
//! Provenance: `machining2d::adaptive`'s (Stori & Wright 2000; Held &
//! Spielberger 2009; Otkur & Lazoglu 2007) and the ladder's own; no
//! vendor material consulted.

use super::params::Pocket3DParams;

use crate::cl::{MoveKind, PathBuilder};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::entry::Entry3D;
use crate::machining2d::{
    ARC_FIT_TOL, GenError, PocketParams, PocketStrategy, ProofTally, Return, Seg, adaptive_passes, choose_entry,
    crossing_free, fit_proved, microlift_link, plan_passes, push_segs,
};

/// Said once per path where the rest pass runs under Adaptive.
pub const REST_RINGS: &str =
    "adaptive: the rest pass cuts Offset rings at the load step - its targets are what the simulation left, entered by plunges";

/// Said per developed level whose area the rotary wrap's seam cuts.
pub const SEAM_RINGS: &str =
    "adaptive: the developed level crosses the wrap's seam (its area is cut by the seam, not by a wall of the work)";

/// Said once per path where the offset shells run under Adaptive.
pub const SHELL_RINGS: &str =
    "adaptive: the offset shells cut Offset rings at the load step - the HSM plan's lifted returns are proved on the plane, not on the shell";

/// The rotary wrap's seam (issue #9): the developed cover runs from
/// `lo` to `hi` in s and repeats every `period`. A level's center area
/// whose component reaches a cover end within `reach` (the seam pad
/// plus the erosion off the end wall) is cut there by the seam, not by
/// a wall of the work - unless the component spans a whole period,
/// where the stock wraps fully and the duplicated lap covers every
/// azimuth (the seam pad law). Such a level is refused by name.
#[derive(Clone, Copy, Debug)]
pub struct Seam {
    pub lo: f64,
    pub hi: f64,
    pub period: f64,
    pub reach: f64,
}

impl Seam {
    /// Does any component of `center` cross the seam?
    pub fn crossed_by(&self, center: &Region) -> bool {
        let tol = 1e-6 * self.period.abs().max(1.0) + 0.05 * self.reach.abs();
        center.loops().iter().filter(|l| l.is_ccw()).any(|l| {
            let (a, b) = l
                .points()
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| (a.min(q.x), b.max(q.x)));
            let at_end = a <= self.lo + self.reach + tol || b >= self.hi - self.reach - tol;
            at_end && b - a < self.period - tol
        })
    }
}

/// One level's plan: its cutter-center walks, the microlift return each
/// pass after the first is reached by (`None`: the ladder's pass link),
/// the proved arc walks (`None`: the points), and the helix radius the
/// level's entry orbits.
pub(super) struct LevelPlan {
    pub passes: Vec<Vec<Point2>>,
    pub joins: Vec<Option<Return>>,
    pub arcs: Option<Vec<Vec<Seg>>>,
    pub helix_r: f64,
    /// The HSM plan planned this level: a pass it reached by no microlift
    /// return is reached by the retract hop (the plan's own hop budget),
    /// never by a keep-down chord at depth - the chord would be legal in
    /// the center area yet slot through stock the plan has not cleared.
    pub hsm: bool,
}

impl LevelPlan {
    fn rings(passes: Vec<Vec<Point2>>, helix_r: f64) -> Self {
        Self { passes, joins: Vec::new(), arcs: None, helix_r, hsm: false }
    }

    /// The level's params: the entry's helix narrowed to the bore the
    /// plan spirals out of (the 2-D pocket's `Motion::with_entry`).
    pub fn params(&self, p: &Pocket3DParams) -> Pocket3DParams {
        Pocket3DParams { helix_radius: self.helix_r, ..*p }
    }
}

/// The level's passes at plane `z` over the cutter-center area `center`:
/// the strategy's tiling, or under Adaptive the HSM plan - with the
/// planner's notes on the builder.
// One slot per law the level threads (the ladder's own shape): the
// builder, the area, the params, the plane, the hop, the rest and shell
// flags, and the wrap's seam (issue #9).
#[allow(clippy::too_many_arguments)]
pub(super) fn plan_level(
    b: &mut PathBuilder,
    center: &Region,
    p: &Pocket3DParams,
    z: f64,
    hop_z: f64,
    rest: bool,
    shell: bool,
    seam: Option<&Seam>,
) -> Result<LevelPlan, GenError> {
    let step = p.tiling_step();
    if p.strategy != PocketStrategy::Adaptive {
        return Ok(LevelPlan::rings(plan_passes(center, step, p.climb, p.strategy)?, p.helix_radius));
    }
    if rest || shell {
        b.note_once(if rest { REST_RINGS } else { SHELL_RINGS });
        let passes = plan_passes(center, step, p.climb, PocketStrategy::OffsetRings)?;
        return Ok(LevelPlan::rings(passes, p.helix_radius));
    }
    let what = match seam {
        Some(_) => format!("the developed level at radius {z:.3}"),
        None => format!("the level at Z {z:.3}"),
    };
    if seam.is_some_and(|sm| sm.crossed_by(center)) {
        return rings_named(b, center, p, SEAM_RINGS, &what);
    }
    // One level: the planner's budget counts its moves once.
    let pp = PocketParams {
        tool_diameter: p.tool_diameter,
        stepover: p.stepover,
        stock_to_leave: p.stock_to_leave,
        clearance_z: p.clearance_z,
        top_z: z,
        bottom_z: z,
        stepdown: p.stepdown,
        feed: p.feed,
        plunge_feed: p.plunge_feed,
        climb: p.climb,
        strategy: PocketStrategy::Adaptive,
        retract_z: p.retract_z,
        entry: p.entry,
        helix_radius: p.helix_radius,
        optimal_load: p.optimal_load,
        blend_turns: false,
        links: p.links,
    };
    if seam.is_some()
        && let Some(why) = narrowed_helix(center, &pp)
    {
        return rings_named(b, center, p, &format!("adaptive: {why}"), &what);
    }
    // On the wrap a hard planner refusal (no room for an entry, ...)
    // costs the level, not the op: rings cut it, said by name - the
    // planar ladder keeps the refusal it always had.
    let (passes, notes, s, room, joins) = match (adaptive_passes(center, &pp, &what, false), seam) {
        (Ok(planned), _) => planned,
        (Err(e), Some(_)) => return rings_named(b, center, p, &format!("adaptive: the HSM planner refused ({e})"), &what),
        (Err(e), None) => return Err(e),
    };
    // Issue #983: the planner's own fallback names the level; were a
    // path ever to hand back rings without that note, the level is
    // named here - a strategy switch is never silent.
    let fell = s != PocketStrategy::Adaptive;
    if fell && !notes.iter().any(|n| n.ends_with(&rings_tag(&what))) {
        b.note(format!("adaptive: the HSM plan was not posted{}", rings_tag(&what)));
    }
    for n in notes {
        b.note(n);
    }
    if fell {
        return Ok(LevelPlan::rings(passes, p.helix_radius));
    }
    let helix_r = room.as_ref().map_or(p.helix_radius, |e| e.helix_r);
    // The arc output (the 2-D pocket's law): fitted at the ring budget,
    // every arc proved inside the center area and against the kink law,
    // then the crossing law on the level's walks as they will be posted
    // - the microlift returns and the pass links between them.
    let mut proof = ProofTally::default();
    let runs: Vec<Vec<Seg>> = passes.iter().map(|w| fit_proved(w, ARC_FIT_TOL * 0.25, center, &mut proof)).collect();
    let start = passes.first().and_then(|w| w.first()).map_or(Point3::new(0.0, 0.0, hop_z), |e| Point3::new(e.x, e.y, hop_z));
    let (_, runs) = crossing_free(start, &passes, runs, &mut proof, |bb, runs| {
        for (pi, (pass, segs)) in passes.iter().zip(runs).enumerate() {
            let Some(&e) = pass.first() else { continue };
            match joins.get(pi) {
                Some(Some(ret)) if pi > 0 => microlift_link(bb, ret, z, hop_z, p.feed),
                _ => {
                    let at = bb.at();
                    bb.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop_z), None);
                    bb.push(MoveKind::Rapid, Point3::new(e.x, e.y, hop_z), None);
                    bb.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(p.plunge_feed));
                }
            }
            push_segs(bb, segs, z, p.feed);
        }
    });
    if let Some(n) = proof.note() {
        match seam {
            Some(_) => b.note(format!("{what}: {n}")),
            None => b.note(format!("Z {z:.3}: {n}")),
        }
    }
    Ok(LevelPlan { passes, joins, arcs: Some(runs), helix_r, hsm: true })
}

/// The tail every ring-cut level's note ends with (issue #983): the
/// level by name - its Z, or on the wrap its developed radius.
pub(super) fn rings_tag(what: &str) -> String {
    format!(" - Offset rings cut {what} instead")
}

/// The level cut by Offset rings at the load step instead of the HSM
/// plan, said by name with `why` (issue #983: every fallback arm of
/// [`plan_level`] goes through here, so none is silent).
fn rings_named(b: &mut PathBuilder, center: &Region, p: &Pocket3DParams, why: &str, what: &str) -> Result<LevelPlan, GenError> {
    b.note(format!("{why}{}", rings_tag(what)));
    let passes = plan_passes(center, p.tiling_step(), p.climb, PocketStrategy::OffsetRings)?;
    Ok(LevelPlan::rings(passes, p.helix_radius))
}

/// Said (with the radii) per developed level refused for a narrowed helix.
pub const NARROW_HELIX: &str = "the developed level is too narrow for the helix entry";

/// Issue #55: on the wrap, a level whose area the plan would narrow the
/// helix for (`machining2d::choose_entry`: the room about the plan's
/// entry spot holds less than the asked radius with room to spiral out)
/// is refused before planning, by name. Such an area is a thin band (a
/// short station window, a groove): there the spiral out of the narrowed
/// bore overloaded its own law (15.7 % D against 14.6 % + 5 % on a
/// 10 long window of the r7 bar) - the plan is never posted overloaded.
/// `None`: the helix fits (or the entry is no helix, or the planner's own
/// entry law decides - its refusals stay its own).
fn narrowed_helix(center: &Region, pp: &PocketParams) -> Option<String> {
    if pp.entry != Entry3D::Helix {
        return None;
    }
    // An entry-law error is not swallowed: the plan asks the same law
    // next and names its refusal (`plan_level`'s wrap arm).
    let mut scratch = Vec::new();
    let room = choose_entry(center, pp, &mut scratch).ok().flatten()?;
    (room.helix_r < pp.helix_radius - 1e-9).then(|| {
        format!(
            "{NARROW_HELIX} (radius {:.3} asked, {:.3} fits with room to spiral out)",
            pp.helix_radius, room.helix_r
        )
    })
}

/// The microlift return onto the next pass's start at plane `z`.
pub(super) fn microlift(b: &mut PathBuilder, ret: &Return, z: f64, hop_z: f64, feed: f64) {
    microlift_link(b, ret, z, hop_z, feed);
}

#[cfg(test)]
#[path = "hsm_fallback_tests.rs"]
mod fallback_tests;

#[cfg(test)]
mod tests {
    use super::{NARROW_HELIX, Seam, narrowed_helix};
    use crate::entry::Entry3D;
    use crate::geo2d::{Loop, Point2, Region};
    use crate::machining2d::{PocketParams, PocketStrategy};

    fn rect(x0: f64, x1: f64) -> Region {
        let pts = vec![Point2::new(x0, 0.0), Point2::new(x1, 0.0), Point2::new(x1, 10.0), Point2::new(x0, 10.0)];
        let Ok(l) = Loop::new(pts) else { panic!("rect loop") };
        let Ok(r) = Region::new(vec![l]) else { panic!("rect region") };
        r
    }

    /// Issue #9's seam law on the developed plane: a cover from 0 to 100
    /// (two laps of 50), reach 7. An area within reach of a cover end and
    /// short of a lap is cut by the seam; one standing inside, or one
    /// spanning a whole lap (the seam pad law), is not.
    #[test]
    fn the_seam_cuts_an_area_at_a_cover_end_short_of_a_lap() {
        let sm = Seam { lo: 0.0, hi: 100.0, period: 50.0, reach: 7.0 };
        assert!(sm.crossed_by(&rect(4.0, 20.0)), "at the low end");
        assert!(sm.crossed_by(&rect(80.0, 95.0)), "at the high end");
        assert!(!sm.crossed_by(&rect(30.0, 60.0)), "inside the cover");
        assert!(!sm.crossed_by(&rect(2.0, 98.0)), "a whole lap band");
    }

    fn band(width: f64) -> Region {
        let pts = vec![Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), Point2::new(40.0, width), Point2::new(0.0, width)];
        let Ok(l) = Loop::new(pts) else { panic!("band loop") };
        let Ok(r) = Region::new(vec![l]) else { panic!("band region") };
        r
    }

    fn pp(entry: Entry3D) -> PocketParams {
        PocketParams {
            tool_diameter: 6.0,
            stepover: 4.0,
            stock_to_leave: 0.5,
            clearance_z: 25.0,
            top_z: 7.0,
            bottom_z: 7.0,
            stepdown: 2.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            strategy: PocketStrategy::Adaptive,
            retract_z: None,
            entry,
            helix_radius: 1.0,
            optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
            blend_turns: false,
            links: crate::links::Links::ROUGH3D,
        }
    }

    /// Issue #55's door, before any planning: a 3 mm centre band (the r7
    /// bar's 10 long window) would narrow the r1 helix and is named; a
    /// 12 mm band holds it whole, and a plunge entry is never asked.
    #[test]
    fn a_band_too_narrow_for_the_helix_is_named_before_planning() {
        let why = narrowed_helix(&band(3.0), &pp(Entry3D::Helix));
        assert!(why.as_deref().is_some_and(|w| w.starts_with(NARROW_HELIX) && w.contains("radius 1.000 asked")), "{why:?}");
        assert_eq!(narrowed_helix(&band(12.0), &pp(Entry3D::Helix)), None);
        assert_eq!(narrowed_helix(&band(3.0), &pp(Entry3D::Plunge)), None);
    }
}
