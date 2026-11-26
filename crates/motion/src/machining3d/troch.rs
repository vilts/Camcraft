//! 3D Adaptive's trochoidal method (issue #1591): under
//! [`AdaptiveMethod::Trochoidal`] each closed level area of the plain
//! ladder is cleared by the 2D method's trochoidal lanes
//! (`machining2d::adaptive_troch_level`) instead of the HSM plan
//! (`hsm`). Trochoidal milling is Otkur & Lazoglu 2007, Int. J. Mach.
//! Tools Manuf. 47, and Rauch, Duc & Hascoet 2009, IJMTM 49. The law
//! here is ours:
//!
//! - the level's cutter-centre area is the ladder's own (the level law
//!   and its erosion), so the loops stay in the level area and beside
//!   the model at every plane;
//! - stock a level leaves stands above its plane: a later bore or plunge
//!   starts at the plane this run PROVED cleared under its disc (the
//!   deepest trochoidal level whose cleared area holds the disc), else
//!   over the stock top;
//! - a level the method cannot cut is cut by the Spiral plan and SAID,
//!   naming its Z - never silent, never a wrong path;
//! - the rest pass, the offset shells and the rotary wrap keep the plan
//!   they had (their own notes say so).
//!
//! No vendor material consulted.

use camcraft_sim::verify::AvoidSpec;

use super::entry::{run_plain_with, run_rest_with};
use super::params::Pocket3DParams;

use crate::cl::{PathBuilder, Toolpath};
use crate::entry::disc_clears;
use crate::geo2d::{Point2, Region};
use crate::geometry::Triangle;
use crate::stock_sim::Columns;
use crate::machining2d::{
    AdaptiveMethod, AdaptiveTrochParams, DEFAULT_MAX_ENGAGEMENT, GenError, PocketParams, PocketStrategy, TrochLevel, WHO_3D,
    plan_troch_level, troch_refusal_text,
};

/// Said once per path cut by the method.
pub const TROCH_SETTINGS: &str = "3D Adaptive (Trochoidal): the loops enter by their own bore helix and link over the retract plane - the Entry, level link and pass link settings are not read on a trochoidal level";

/// 3D Adaptive under [`AdaptiveMethod::Trochoidal`] (#1591): the plain
/// ladder of [`super::pocket3d_prev`] with each level cleared by
/// trochoidal loops; `rest` adds the rest-closing pass (its targets keep
/// their Offset rings, said), `avoid` the "Do not touch" items (#2055).
/// A strategy other than Adaptive reads no method and runs as it always
/// did.
pub fn pocket3d_trochoidal(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    rest: bool,
    mask: Option<&Columns<'_>>,
    avoid: Option<&AvoidSpec>,
) -> Result<Toolpath, GenError> {
    let av = avoid.filter(|a| !a.is_empty());
    let method = AdaptiveMethod::Trochoidal;
    let mut path = match rest {
        false => run_plain_with(facets, footprint, p, false, mask, None, av, method),
        true => run_rest_with(facets, footprint, p, false, mask, av, method).map(|(t, _)| t),
    }?;
    if let Some(a) = av {
        let _ = camcraft_sim::verify::elevate_rapid_links(&mut path, a, None, camcraft_sim::verify::AVOID_PAD);
    }
    Ok(path)
}

/// One ladder run's trochoidal state.
pub(super) struct Run {
    on: bool,
    /// Each trochoidal level's plane and the area it swept clear, top down.
    cleared: Vec<(f64, Region)>,
    stock_top: f64,
    levels: usize,
    fell: usize,
    loops: usize,
    deg: f64,
}

impl Run {
    /// `plain`: the coarse planar ladder - no rest pass, shell or wrap.
    pub(super) fn new(method: AdaptiveMethod, p: &Pocket3DParams, plain: bool) -> Self {
        let ap = &p.links.approach;
        let stock_top = ap.entry_top(p.top_z).max(ap.stock_top.filter(|s| s.is_finite()).unwrap_or(p.top_z));
        Self {
            on: plain && method == AdaptiveMethod::Trochoidal && p.strategy == PocketStrategy::Adaptive,
            cleared: Vec::new(),
            stock_top,
            levels: 0,
            fell: 0,
            loops: 0,
            deg: 0.0,
        }
    }

    /// Is the method cutting this run?
    pub(super) fn on(&self) -> bool {
        self.on
    }

    /// Plan and post the level at `z` over `center`. `Ok(true)`: posted.
    /// `Ok(false)`: the method is off, or it refused the level - said,
    /// with its Z - and the caller's plan cuts it. Nothing is posted on
    /// a refusal: the level is built on a copy of the path.
    pub(super) fn level(&mut self, b: &mut PathBuilder, center: &Region, p: &Pocket3DParams, z: f64, hop_z: f64) -> Result<bool, GenError> {
        if !self.on {
            return Ok(false);
        }
        let pp = PocketParams {
            blend_radius: None,
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
            retract_z: Some(hop_z),
            entry: p.entry,
            helix_radius: p.helix_radius,
            optimal_load: p.optimal_load,
            blend_turns: false,
            links: p.links,
        };
        let cleared = |c: Point2, rad: f64| self.cleared.iter().rev().find(|(_, reg)| disc_clears(reg, c, rad)).map(|(zk, _)| *zk);
        let attempt = plan_troch_level(center, &pp, &AdaptiveTrochParams::default()).and_then(|lvl| {
            let mut trial = b.clone();
            let n0 = trial.moves_len();
            lvl.emit(&mut trial, &pp, z, hop_z, self.stock_top, &cleared)?;
            TrochLevel::check(trial.moves(), n0, center)?;
            Ok((lvl, trial))
        });
        match attempt {
            Ok((lvl, trial)) => {
                *b = trial;
                b.note_once(TROCH_SETTINGS);
                if let Some((mm2, share)) = lvl.rest() {
                    b.note(format!(
                        "{WHO_3D}: about {mm2:.1} mm2 ({:.0} %) of the level at Z {z:.3} is outside the loops' reach and stands above that plane",
                        share * 100.0
                    ));
                }
                let (n, deg) = lvl.loops();
                self.loops += n;
                self.deg = self.deg.max(deg);
                self.levels += 1;
                self.cleared.push((z, lvl.cleared().clone()));
                Ok(true)
            }
            // A kernel failure is no verdict on the level: it is the
            // op's refusal, as on every other ladder.
            Err(GenError::Geo(e)) => Err(GenError::Geo(e)),
            Err(e) => {
                let why = troch_refusal_text(&e);
                let why = why.strip_prefix(WHO_3D).map_or(why.as_str(), |w| w.trim_start_matches(':').trim_start());
                b.note(format!("{WHO_3D}: {why} - the Spiral plan cuts the level at Z {z:.3} instead"));
                self.fell += 1;
                Ok(false)
            }
        }
    }

    /// The run's own account, once.
    pub(super) fn say(&self, b: &mut PathBuilder) {
        if self.levels == 0 {
            return;
        }
        b.note(format!(
            "{WHO_3D}: {} level(s) cut by {} trochoidal loops, measured {:.1} deg engagement (limit {:.1} deg); {} level(s) cut by the Spiral plan",
            self.levels, self.loops, self.deg, DEFAULT_MAX_ENGAGEMENT, self.fell
        ));
    }
}

#[cfg(test)]
#[path = "troch_tests.rs"]
mod tests;
