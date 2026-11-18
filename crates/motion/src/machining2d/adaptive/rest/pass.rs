//! One rest pass on trial (issue 2031): reached each way `leads`
//! offers, proved inside the legal center area, asked the verifier's
//! crossing law against the depth's earlier feeds, then WALKED on the
//! stock raster - kept only if the whole feed read at most the optimal
//! load and it cut something. Provenance: the measured-not-asserted law
//! of the Adaptive planner (`adaptive`, `raster`; the contact angle of
//! Y. Altintas, "Manufacturing Automation", 2nd ed., Cambridge 2012,
//! ch. 2); the trial order and the refusals are ours.

use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::load::load_at_rim;
use crate::machining2d::{GenError, chord_clears};

use super::super::front::len;
use super::super::guard::first_crossing;
use super::super::raster::Raster;
use super::super::tidy;
use super::leads::{Anchor, Lead};
use super::peel::Peel;
use super::runs::Run;

/// A pass that clears fewer cells than this cut nothing worth a pass.
const MIN_CELLS: usize = 4;

/// Why a run could not be kept, most serious last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Fail {
    /// Held the load but cut nothing: crumbs, not a refusal.
    Idle,
    Crossing(Point2),
    Entry(Point2),
    Load(f64, Point2),
}

impl Fail {
    fn rank(&self) -> u8 {
        match self {
            Fail::Idle => 0,
            Fail::Crossing(_) => 1,
            Fail::Entry(_) => 2,
            Fail::Load(..) => 3,
        }
    }

    /// The more serious of two; of two overloads, the lighter (what the
    /// ladder could do at best).
    pub(super) fn worse(self, other: Fail) -> Fail {
        match (self, other) {
            (Fail::Load(a, _), Fail::Load(b, _)) if b < a => other,
            (Fail::Load(..), Fail::Load(..)) => self,
            _ if other.rank() > self.rank() => other,
            _ => self,
        }
    }
}

/// A pass on trial that held: its feed, the stock it leaves, its worst
/// reading.
pub(super) struct Trial {
    lead: Lead,
    ras: Raster,
    goal: Raster,
    worst: f64,
    start: Point2,
}

impl Peel<'_> {
    /// Try `run` with each way of reaching it; the first whose whole
    /// feed is legal, crosses nothing, holds the load and cuts is kept.
    pub(super) fn attempt(&mut self, run: &Run) -> Result<Trial, Fail> {
        let r = self.lv.r;
        let Some(start) = run.v.first().map(|v| v.p) else { return Err(Fail::Idle) };
        let mut fail = Fail::Entry(start);
        for lead in self.leads(run) {
            let pts = &lead.pts;
            // Legal: every chord inside the center area (a chord of a
            // wall is, by construction).
            let legal = |w: &[Point2]| self.canon.wall_chord(w[0], w[1]) || chord_clears(self.lv.center, w[0], w[1]);
            if let Some(w) = pts.windows(2).find(|w| !legal(w)) {
                fail = fail.worse(Fail::Entry(w[1]));
                continue;
            }
            if let Some(x) = self.crossing(pts) {
                fail = fail.worse(Fail::Crossing(x));
                continue;
            }
            let mut ras = self.ras.clone();
            self.work += len(pts);
            let (deg, at) = ras.walk_until(pts, (r, self.rim, r / 10.0), self.bound);
            if deg > self.bound {
                fail = fail.worse(Fail::Load(deg, at));
                // Overloaded on the run itself: no other way of reaching
                // it changes that.
                let lead = &pts[..lead.own.min(pts.len())];
                if lead.windows(2).all(|w| seg_dist(at, w[0], w[1]) > 0.25 * r) {
                    break;
                }
                continue;
            }
            let mut goal = self.goal.clone();
            pts.windows(2).for_each(|w| goal.cut(w[0], w[1], r));
            if self.goal.standing().saturating_sub(goal.standing()) < MIN_CELLS {
                return Err(Fail::Idle);
            }
            return Ok(Trial { lead, ras, goal, worst: deg, start });
        }
        Err(fail)
    }

    /// Where the feed `pts` would cross itself or an earlier feed of
    /// this depth (the verifier's crossing law, asked first).
    fn crossing(&self, pts: &[Point2]) -> Option<Point2> {
        let bb = bounds(pts);
        // (A feed that starts where an earlier one ended shares that end
        // point with it, which the law reads as a touch.)
        let mut walks: Vec<Vec<Point2>> =
            self.plan.chains.iter().filter(|c| overlap(bounds(c), bb)).cloned().collect();
        walks.push(pts.to_vec());
        first_crossing(&walks)
    }

    pub(super) fn commit(&mut self, t: Trial) -> Result<(), GenError> {
        let h = self.ras.h;
        let Lead { pts, hop, start, end, run_at, run_wall, .. } = t.lead;
        self.back = run_wall.map(|at| (pts[run_at.min(pts.len())..].to_vec(), at));
        let sw = Region::sweep(std::slice::from_ref(&pts), self.lv.r)?;
        self.mat = tidy::tidy(&self.mat.difference(&sw)?, 0.25 * h, 0.1 * h, 4.0 * h * h)?;
        self.ras = t.ras;
        self.goal = t.goal;
        self.worst_deg = self.worst_deg.max(t.worst);
        self.cur = pts.last().copied();
        self.anchor = Some(t.start);
        self.plan.passes += 1;
        // Where this pass landed on a wall and left it: later passes of
        // the area retrace both (`leads`).
        if let Some(at) = start
            && !self.anchors.iter().any(|a| a.start == at)
        {
            self.anchors.push(Anchor { start: at, entry: hop.then(|| pts.first().copied()).flatten() });
        }
        if let Some(at) = end
            && !self.ends.contains(&at)
        {
            self.ends.push(at);
        }
        match self.plan.chains.last_mut() {
            Some(last) if !hop => last.extend(pts.into_iter().skip(1)),
            _ => self.plan.chains.push(pts),
        }
        Ok(())
    }

    /// The named refusal of a run no step could cut (crumbs are none).
    pub(super) fn refusal(&self, why: Fail) -> Option<GenError> {
        let (d, z) = (2.0 * self.lv.r, self.lv.z);
        let (msg, at) = match why {
            Fail::Idle => return None,
            Fail::Load(deg, at) => (
                format!(
                    "rest Adaptive cannot hold the optimal load at ({:.2}, {:.2}), Z{z:.3}: the lightest step still loads the tool to {:.0}% of its diameter against {:.0}% - choose Waterline or Offset rings",
                    at.x,
                    at.y,
                    load_at_rim(deg, d, self.rim) * 100.0,
                    self.lv.load * 100.0
                ),
                at,
            ),
            Fail::Entry(at) => (
                format!(
                    "rest Adaptive has no entry with proved clearance for the rest material at ({:.2}, {:.2}), Z{z:.3}: the remaining stock does not prove the cutter's whole footprint and its approach clear near it",
                    at.x, at.y
                ),
                at,
            ),
            Fail::Crossing(at) => (
                format!(
                    "rest Adaptive's plan would cross itself at ({:.2}, {:.2}), Z{z:.3} - choose Waterline or Offset rings",
                    at.x, at.y
                ),
                at,
            ),
        };
        Some(GenError::invalid_at(msg, Point3::new(at.x, at.y, z)))
    }
}

fn bounds(pts: &[Point2]) -> (Point2, Point2) {
    pts.iter().fold(
        (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY)),
        |(lo, hi), p| (Point2::new(lo.x.min(p.x), lo.y.min(p.y)), Point2::new(hi.x.max(p.x), hi.y.max(p.y))),
    )
}

fn overlap(a: (Point2, Point2), b: (Point2, Point2)) -> bool {
    a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.y <= b.1.y && b.0.y <= a.1.y
}

fn seg_dist(q: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (q.x - a.x - t * dx).hypot(q.y - a.y - t * dy)
}
