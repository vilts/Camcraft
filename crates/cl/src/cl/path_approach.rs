//! The approach law of a clearance-seeded path (#690, #697), split from
//! `path.rs` (400-line law): a path seeded on the clearance plane always
//! opens with a rapid, so a post's first motion after the tool call is a
//! G00 there - never a feed out of the tool-change position.
//!
//! Provenance: #690's posted-program law; our own builder design.

use super::super::moves::{Move, MoveKind};
use super::{PathBuilder, Toolpath};
use crate::geometry::Point3;

/// How close a stated seed must be to the opening rapid's start.
const SEED_EPS: f64 = 1e-9;

impl Toolpath {
    /// Seat the opening approach over its own entry point (#2901).
    ///
    /// The generators seed their builder at the frame's zero on the
    /// clearance plane (`seed`). The machine never visits that point:
    /// after a tool change its X/Y is not known, and the post writes a
    /// Z-only G00 and then the entry X/Y. So when the first move is a
    /// rapid out of `seed` to somewhere else, its start moves to the
    /// rapid's own end X/Y on the same plane. The rapid's end is
    /// untouched, so no posted block changes - the post writes ends
    /// only - and the drawn approach, the simulation, the fixture checks
    /// and the link router see no travel from a point the machine never
    /// visits: not the frame zero, not the WCS origin (#2382's seat,
    /// reversed here).
    ///
    /// A path that does not open that way (a hole list seeded on its
    /// first hole, a lathe seeded at its own station, a zero-length
    /// rapid that says the cut starts AT the seed) is left as it is.
    /// Returns whether the start moved.
    pub fn seat_approach(&mut self, seed: Point3) -> bool {
        let Some(first) = self.moves.first_mut() else {
            return false;
        };
        let at_seed = (first.from.x - seed.x).abs() <= SEED_EPS
            && (first.from.y - seed.y).abs() <= SEED_EPS
            && (first.from.z - seed.z).abs() <= SEED_EPS;
        let leaves = (first.to.x - first.from.x).abs() > SEED_EPS || (first.to.y - first.from.y).abs() > SEED_EPS;
        let finite = first.to.x.is_finite() && first.to.y.is_finite();
        if first.kind != MoveKind::Rapid || first.arc.is_some() || !at_seed || !leaves || !finite {
            return false;
        }
        first.from.x = first.to.x;
        first.from.y = first.to.y;
        true
    }
}

impl PathBuilder {
    /// A builder seeded at `at` on the clearance plane whose path always
    /// opens with a rapid (#690, #697). Posts write only a move's end, so
    /// the opening rapid is the program's approach - the G00 after the
    /// tool call. A generator's own rapid to its start is dropped when the
    /// start IS the seed (a zero-length move), and the first motion would
    /// then be a feed out of the tool-change position; so if the first
    /// move is not a rapid, a zero-length rapid at the seed is stated
    /// ahead of it. It lands before any move is recorded, so no cycle,
    /// comp span or note index ever shifts.
    pub fn approached(at: Point3) -> Self {
        PathBuilder { approach: true, ..Self::new(at) }
    }

    /// The approach law of [`approached`](Self::approached), run before
    /// the first appended move of `kind`.
    pub(super) fn open(&mut self, kind: MoveKind) {
        if std::mem::take(&mut self.approach) && self.moves.is_empty() && kind != MoveKind::Rapid {
            self.moves.push(Move::new(MoveKind::Rapid, self.at, self.at, None).attach_axis(self.axis));
            self.at_pose = self.axis;
        }
    }
}
