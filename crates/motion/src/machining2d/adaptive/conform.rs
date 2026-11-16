//! The bore exit's conforming contact and the grown walk's kinks (issue
//! #1050). Out of the entry bore the growth rings are circles about the
//! seed, so the tool (radius r) stands inside a cleared disc only a
//! little wider than itself: the stock wall it meets curves the same
//! way as its rim. On such a wall the contact arc grows with the rim's
//! radius faster than a straight wall's (`load::deg_at_rim`, the law
//! every reading is converted by), so a revolution read on the
//! planner's own rim (a planner cell and a half out, `quality::rim_for`)
//! can hold the bound while the same cut read on a coarser yardstick's
//! rim stands past it - the rotary bar's neck read 47.5 degrees on the
//! planner's D/120 stock and 55.5 (20.3 % D) on the suite's D/60 one.
//! The disc phase is therefore held on both rims: its own, and the rim
//! of a yardstick twice as coarse, converted back by the straight-wall
//! law, the worse of the two kept. Off the bore (fronts, wall runs) the
//! wall is straight or convex to the rim and the readings agree.
//!
//! The closing laps' in-cut kinks (where a corner fillet's last chord
//! lands on a straight wall run they read 5.1 to 6.2 degrees, past the
//! 5-degree break law) are spread like the star spiral's
//! (`tidy::smooth_walk`, Taubin 1995's umbrella operator) before the
//! walk is lifted and read again on the stock (`lift`). Only the laps:
//! the revolutions' own chords turn under the law, and smoothing them
//! (24 umbrella passes) would shrink every ring by their sagitta.
//!
//! Provenance: the engagement arc of peripheral milling (Altintas 2000,
//! *Manufacturing Automation*, ch. 2) on a circular wall, the in-process
//! stock reading of Stori & Wright (2000, J. Manuf. Systems 19(3)); the
//! measuring rim is `quality`'s own law. No vendor material consulted.

use crate::geo2d::Point2;
use crate::load::{deg_at_rim, load_at_rim};

use super::raster::Raster;
use super::{Planner, STAR_SMOOTH_DEG, tidy};

/// The coarser yardstick the disc phase is also held on: this many
/// planner cells (the suites measure on D/60 to D/120; the planner
/// plans on D/120).
const COARSE: f64 = 2.0;

impl Planner<'_> {
    /// The rim a yardstick [`COARSE`] times the planner's cell reads on.
    fn coarse_rim(&self) -> f64 {
        crate::quality::rim_for(2.0 * self.s.r, COARSE * self.ras.h)
    }

    /// A reading on the coarse rim as the planner's rim would read the
    /// same load (the straight-wall law both ways).
    fn coarse_as_planned(&self, deg: f64) -> f64 {
        let d = 2.0 * self.s.r;
        deg_at_rim(load_at_rim(deg, d, self.coarse_rim()), d, self.rim)
    }

    /// A disc-phase revolution walked on `ras` (from the planner's
    /// stock) and read on both rims: the worse reading, in the planner's
    /// own degrees, and where. Past `limit` the walk is abandoned (the
    /// trial has failed).
    pub(super) fn conform_walk(&self, ras: &mut Raster, rev: &[Point2], limit: f64) -> (f64, Point2) {
        let mut probe = ras.clone();
        let fine = ras.walk_until(rev, (self.s.r, self.rim, self.step), limit);
        if fine.0 > limit {
            return fine;
        }
        let d = 2.0 * self.s.r;
        let lim = if limit.is_finite() { deg_at_rim(load_at_rim(limit, d, self.rim), d, self.coarse_rim()) } else { f64::INFINITY };
        let (deg, at) = probe.walk_until(rev, (self.s.r, self.coarse_rim(), self.step), lim);
        let coarse = self.coarse_as_planned(deg);
        if coarse > fine.0 { (coarse, at) } else { fine }
    }

    /// Spread the walk's kinks past [`STAR_SMOOTH_DEG`] from `from` (the
    /// closing laps' first point) on, inside the center area; a lap
    /// never marked (`usize::MAX`) leaves the walk as it is.
    pub(super) fn smooth_grown(&mut self, from: usize) {
        if let Some(run) = self.walk.get_mut(from.saturating_sub(1)..) {
            tidy::smooth_walk(run, self.center, STAR_SMOOTH_DEG, 24);
        }
    }
}
