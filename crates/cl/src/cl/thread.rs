//! The threading intent record (issue #34): what a thread's cycle is,
//! standing beside the explicit helical passes that cut it.
//!
//! Provenance: the quantities a lathe control's multi-pass threading
//! cycle asks for - start and end of the thread, major / minor
//! diameter, lead, thread height, first-pass and minimum depth, finish
//! allowance, infeed angle, spring passes, pull-out - as public lathe
//! programming references and machine-tool handbooks list them
//! (behaviour only). Our own model; no vendor material consulted. The
//! explicit passes remain the truth for verification, display and
//! simulation (the drilling `Cycle` law); the record is what a post
//! with a threading cycle writes in their place.
//!
//! The hand (issue #629): a helix is right-hand when it advances along
//! +Z while turning counter-clockwise seen from +Z - and stays so run
//! backwards (clockwise while falling). The turning frame stands the
//! free end at +Z and the chuck below, so the standard right-hand
//! thread cut toward the chuck is a clockwise-falling helix, which is
//! the tool's motion relative to a part turning M03 (clockwise seen
//! from the headstock). Screw-thread handedness and the M03 sense as
//! machine-tool handbooks and RS-274 / ISO 6983 state them.

use crate::geometry::Point3;

use super::moves::Move;
use super::plane::ArcPlane;

/// Which way a thread winds (issue #629): right-hand, the default every
/// drawing means unless it says LH, or left-hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ThreadHand {
    #[default]
    Right,
    Left,
}

impl ThreadHand {
    /// Both hands, the order a picker lists them.
    pub const ALL: [ThreadHand; 2] = [ThreadHand::Right, ThreadHand::Left];

    /// The hand a helical half-turn lays: an XY arc turning `ccw`
    /// (seen from +Z) while the tool travels `dz` along the axis.
    /// Right-hand when it turns counter-clockwise rising or clockwise
    /// falling; `None` for an arc with no axial travel (no helix).
    #[must_use]
    pub fn of_turn(ccw: bool, dz: f64) -> Option<ThreadHand> {
        if !dz.is_finite() || dz == 0.0 {
            return None;
        }
        Some(if ccw == (dz > 0.0) { ThreadHand::Right } else { ThreadHand::Left })
    }

    /// The hand one move lays: an XY arc with axial travel, else `None`.
    #[must_use]
    pub fn of_move(m: &Move) -> Option<ThreadHand> {
        let a = m.arc.filter(|a| a.plane == ArcPlane::Xy)?;
        ThreadHand::of_turn(a.ccw, m.to.z - m.from.z)
    }

    /// The arc sense (CCW seen from +Z) that lays this hand while the
    /// tool travels `toward_minus_z` (toward the chuck) or away.
    #[must_use]
    pub fn ccw_for(self, toward_minus_z: bool) -> bool {
        (self == ThreadHand::Right) != toward_minus_z
    }

    /// The file / stream word: `right` or `left`.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            ThreadHand::Right => "right",
            ThreadHand::Left => "left",
        }
    }

    /// Read a [`Self::word`] back; anything else is `None`.
    #[must_use]
    pub fn from_word(w: &str) -> Option<ThreadHand> {
        match w {
            "right" => Some(ThreadHand::Right),
            "left" => Some(ThreadHand::Left),
            _ => None,
        }
    }
}

/// One thread's machining intent: the passes `moves[first..=last]`
/// are what the machine does in G32 form; a cycle-capable post writes
/// one threading cycle from these numbers instead. Points are world
/// coordinates on the stream's lattice; lengths are mm, radial
/// quantities are RADIUS values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreadCycle {
    /// The cycle's start point: clearance radius, thread start station
    /// (the tool returns here after every pass).
    pub start: Point3,
    /// The thread's end at the root: minor radius, last station.
    pub end: Point3,
    /// Crest diameter (2 x the blank's crest radius in the window).
    pub major_diameter: f64,
    /// Root diameter (2 x the deepest pass's tip radius).
    pub minor_diameter: f64,
    /// Axial travel per spindle revolution (single start: the pitch).
    pub lead: f64,
    /// Thread height, radial (crest to root).
    pub height: f64,
    /// Depth of the first pass, radial.
    pub first_depth: f64,
    /// The smallest depth increment a pass may take, radial.
    pub min_depth: f64,
    /// The depth the finishing pass - the first pass at the root -
    /// takes below the roughing pass before it, radial (issue #417):
    /// a control's cycle roughs to `height - finish_allowance`, then
    /// finishes. Never 0 on a laid ladder: when the plan states no
    /// allowance, the last roughing rung's own step is this depth.
    pub finish_allowance: f64,
    /// Included angle of the compound infeed, degrees; 0 is straight
    /// radial infeed (every pass plunges square to the axis).
    pub infeed_angle: f64,
    /// Extra passes at full depth after the finishing pass.
    pub spring_passes: u32,
    /// The passes alternate flanks (issue #417): `infeed_angle` is the
    /// included angle of each flank's line, the side flipping every
    /// roughing pass - a single-flank cycle cannot state it.
    pub alternating: bool,
    /// Every roughing depth is cut twice before the next (issue #418):
    /// the passes are not a cycle's one-cut-per-depth ladder, so a
    /// cycle cannot state them.
    pub repeat_each_depth: bool,
    /// The roughing passes carry in-pass chip-break dips (issue #419):
    /// half-turns whose radius changes inside one synchronized chain.
    /// Only a continuous-threading G32 chain can state them - no
    /// cycle can, and a post that does not declare continuous
    /// threading refuses them.
    pub chip_break_dips: bool,
    /// Axial length of the angled pull-out at the thread's end (0: a
    /// square, radial retract).
    pub pull_out: f64,
    /// The passes' first move (the rapid onto the start point).
    pub first: usize,
    /// The passes' last move (the retract after the deepest pass),
    /// inclusive.
    pub last: usize,
    /// The hand the passes cut (issue #629) - every helical half-turn
    /// in the span lays it ([`Self::is_sane`] holds them to it), so a
    /// post never has to infer it.
    pub hand: ThreadHand,
}

impl ThreadCycle {
    /// Does the record sit on a path of these moves: an in-range span
    /// opening on its start point and finite numbers that state a
    /// thread? The door the dump and the loader check before trusting
    /// a record; out-of-range CYCLE words are the post's to refuse.
    #[must_use]
    pub fn is_sane(&self, moves: &[Move]) -> bool {
        let (Some(m0), Some(_)) = (moves.get(self.first), moves.get(self.last)) else {
            return false;
        };
        let nums = [
            self.major_diameter,
            self.minor_diameter,
            self.lead,
            self.height,
            self.first_depth,
            self.min_depth,
            self.finish_allowance,
            self.infeed_angle,
            self.pull_out,
        ];
        self.first <= self.last
            && m0.to == self.start
            && self.start.is_finite()
            && self.end.is_finite()
            && nums.iter().all(|v| v.is_finite())
            && self.lead > 0.0
            && self.height > 0.0
            && self.passes_lay_hand(moves)
    }

    /// Does every helical half-turn in the span lay the stated hand?
    /// An XY arc with no axial travel inside a thread span is no helix
    /// and fails too. A span with no arcs states nothing against it.
    #[must_use]
    pub fn passes_lay_hand(&self, moves: &[Move]) -> bool {
        let Some(span) = moves.get(self.first..=self.last) else {
            return false;
        };
        span.iter()
            .filter(|m| m.arc.is_some_and(|a| a.plane == ArcPlane::Xy))
            .all(|m| ThreadHand::of_move(m) == Some(self.hand))
    }

    /// Whether the cycle feeds toward -Z (toward the chuck, the turning
    /// frame's law): its end stands below its start.
    #[must_use]
    pub fn toward_minus_z(&self) -> bool {
        self.end.z < self.start.z
    }

    /// Whether the spindle must run REVERSE (M04) for the tool to lay
    /// these passes: the passes turn counter-clockwise seen from +Z (the
    /// free end), and the tool turns that way about the part only when
    /// the part turns clockwise seen from there - counter-clockwise seen
    /// from the headstock, which is M04. Clockwise passes are M03.
    #[must_use]
    pub fn reverse_spindle(&self) -> bool {
        self.hand.ccw_for(self.toward_minus_z())
    }
}
