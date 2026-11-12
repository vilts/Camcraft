//! Closed adaptive fronts from the entry bore: Stori & Wright (2000)
//! in-process-stock fronts joined as one spiral (Held & Spielberger 2009).

use crate::geo2d::Point2;

use super::front::{rotate_to, tail};
use super::raster::Raster;
use super::rings::circle;
use super::star::orbit_radius;
use super::{GenError, MAX_ROUNDS, Planner, SPIRAL_STEPS, Worst, loops, spiral};

/// A spiral revolution on trial: the ring it reaches, the revolution,
/// the stock it leaves, its worst reading.
type SpiralTrial = (Vec<Point2>, Vec<Point2>, Raster, Worst);

/// The spiral rides a cleared-area loop while at least this share of it
/// faces material.
const SPIRAL_FRONT: f64 = 0.6;

impl Planner<'_> {
    /// Phase 1: closed fronts around the bore, cut as one spiral.
    /// Returns the heading the spiral ends on.
    pub(super) fn core_spiral(&mut self) -> Result<Point2, GenError> {
        let s = self.s;
        let mut rings: Vec<Vec<Point2>> = Vec::new();
        // The bore's own orbit is the spiral's ring zero: the tool
        // leaves the entry on a turn, not a radial step. It is walked
        // once flat first - whatever floor the descent left (a helix
        // leaves its last pitch as a ramp) is the bore's to clear,
        // inside the entry's own circle.
        rings.push(self.orbit());
        // Each revolution is planned as what is emitted: the morph from
        // the last ring onto the next front loop, measured on the stock
        // the revolutions before it really left.
        for _ in 0..MAX_ROUNDS {
            let Some(prev) = rings.last().cloned() else { break };
            let mut kept: Option<SpiralTrial> = None;
            for (i, f) in SPIRAL_STEPS.iter().enumerate() {
                let room = loops::cleared_loops(&self.mat, s.r - self.width(*f), self.center, (s.climb, self.seg, s.smooth), (&self.ras, self.rim, 0.5 * self.bound, self.bite))?;
                // The spiral lasts while the cleared area has ONE loop
                // still facing material (crumbs under half a radius
                // aside - the open rounds take those) and that loop is
                // mostly cutting: once the walls carry most of it, the
                // open rounds' return links fly less air.
                let mut live = room.iter().filter(|l| l.front >= 0.5 * s.r);
                let (Some(l), None) = (live.next(), live.next()) else { break };
                if l.strong < SPIRAL_FRONT * l.total {
                    break;
                }
                let ring = rotate_to(&l.pts, prev[0]);
                let rev = spiral::revolution(&prev, &ring);
                let mut trial = self.ras.clone();
                let w = trial.walk(&rev, s.r, self.rim, self.step);
                if w.0 <= self.bound || i + 1 == SPIRAL_STEPS.len() {
                    kept = Some((ring, rev, trial, w));
                    break;
                }
            }
            let Some((ring, rev, trial, w)) = kept else { break };
            self.note_worst(w);
            self.ras = trial;
            self.take(&rev)?;
            self.push_walk(&rev);
            rings.push(ring);
        }
        // The last ring once whole: the spiral's outer edge is that loop.
        let last = rings.last().cloned().unwrap_or_default();
        if rings.len() > 1 {
            let mut trial = self.ras.clone();
            let w = trial.walk(&last, s.r, self.rim, self.step);
            self.note_worst(w);
            self.ras = trial;
            self.take(&last)?;
            self.push_walk(&last);
        }
        let heading = tail(&self.walk);
        Ok(heading)
    }

    /// The bore's own orbit, walked once flat (a helix leaves its last
    /// pitch as a ramp: that floor is the bore's to clear, inside the
    /// entry's own circle), or - out of a tool-size bore - said.
    pub(super) fn orbit(&mut self) -> Vec<Point2> {
        let s = self.s;
        let ring0 = circle(s.seed, orbit_radius(&s), s.start, s.climb, self.seg);
        if s.helix_r > 1e-6 {
            self.walk.extend(ring0.iter().skip(1).copied());
        } else {
            self.plan.notes.push(format!(
                "entry: the {} bore at ({:.2}, {:.2}) is tool-size - the first turn out of it exceeds the load (a Helix entry holds it)",
                if s.bore > 0.0 { "axial" } else { "missing" },
                s.seed.x,
                s.seed.y
            ));
        }
        ring0
    }
}
