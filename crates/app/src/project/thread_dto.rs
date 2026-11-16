//! The cached threading-cycle record (issue #34, `crate::cl::ThreadCycle`)
//! in the project file. Our own format; the additive law of the comps
//! and notes keys: absent is none - every non-threading path, and every
//! cache before the record existed.

use serde::{Deserialize, Serialize};

use crate::cl::{lattice, Move, ThreadCycle, ThreadHand};
use crate::geometry::Point3;
use crate::machining2d::{PassSchedule, ThreadPasses};

/// The thread's hand in the file (#629): `right` / `left`. Absent reads
/// right - what every drawing means unless it says LH, and the hand
/// every cached record before the key existed laid (its passes rose
/// turning counter-clockwise: a right-hand helix).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreadHandDto {
    #[default]
    Right,
    Left,
}

impl From<ThreadHand> for ThreadHandDto {
    fn from(h: ThreadHand) -> Self {
        match h {
            ThreadHand::Right => ThreadHandDto::Right,
            ThreadHand::Left => ThreadHandDto::Left,
        }
    }
}

impl From<ThreadHandDto> for ThreadHand {
    fn from(h: ThreadHandDto) -> Self {
        match h {
            ThreadHandDto::Right => ThreadHand::Right,
            ThreadHandDto::Left => ThreadHand::Left,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ThreadCycleDto {
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub major_diameter: f64,
    pub minor_diameter: f64,
    pub lead: f64,
    pub height: f64,
    pub first_depth: f64,
    pub min_depth: f64,
    pub finish_allowance: f64,
    pub infeed_angle: f64,
    pub spring_passes: u32,
    pub pull_out: f64,
    /// #417: absent (and written only when set) is a single-flank or
    /// radial record - every earlier cache.
    #[serde(default, skip_serializing_if = "is_false")]
    pub alternating: bool,
    /// #418: absent (and written only when set) is one cut per depth.
    #[serde(default, skip_serializing_if = "is_false")]
    pub repeat_each_depth: bool,
    pub first: u64,
    pub last: u64,
    /// The hand the passes cut (#629); absent reads right.
    #[serde(default)]
    pub hand: ThreadHandDto,
}

impl ThreadCycleDto {
    pub(super) fn from_cycle(t: &ThreadCycle) -> Self {
        // The points take the lattice snap `MoveDto` gives the moves
        // they sit on, so the record still opens on its move on load.
        let lat = |p: Point3| [lattice(p.x), lattice(p.y), lattice(p.z)];
        ThreadCycleDto {
            start: lat(t.start),
            end: lat(t.end),
            major_diameter: t.major_diameter,
            minor_diameter: t.minor_diameter,
            lead: t.lead,
            height: t.height,
            first_depth: t.first_depth,
            min_depth: t.min_depth,
            finish_allowance: t.finish_allowance,
            infeed_angle: t.infeed_angle,
            spring_passes: t.spring_passes,
            pull_out: t.pull_out,
            alternating: t.alternating,
            repeat_each_depth: t.repeat_each_depth,
            first: t.first as u64,
            last: t.last as u64,
            hand: t.hand.into(),
        }
    }

    /// The record over the loaded moves - `None` when it does not sit
    /// on them (the file's bytes are the law, the sanity door is the
    /// reader's: the passes then post as themselves).
    pub(super) fn to_cycle(&self, moves: &[Move]) -> Option<ThreadCycle> {
        let t = ThreadCycle {
            start: Point3::new(self.start[0], self.start[1], self.start[2]),
            end: Point3::new(self.end[0], self.end[1], self.end[2]),
            major_diameter: self.major_diameter,
            minor_diameter: self.minor_diameter,
            lead: self.lead,
            height: self.height,
            first_depth: self.first_depth,
            min_depth: self.min_depth,
            finish_allowance: self.finish_allowance,
            infeed_angle: self.infeed_angle,
            spring_passes: self.spring_passes,
            pull_out: self.pull_out,
            alternating: self.alternating,
            repeat_each_depth: self.repeat_each_depth,
            first: usize::try_from(self.first).unwrap_or(usize::MAX),
            last: usize::try_from(self.last).unwrap_or(usize::MAX),
            hand: self.hand.into(),
        };
        t.is_sane(moves).then_some(t)
    }
}

/// A thread-mill operation's pass control in the file (#988), flattened
/// into the Thread params. Every key is additive and written only when
/// it differs from the #894 ride, so an older file reads the ride it
/// was saved with and a default save stays byte-identical.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct ThreadPassesDto {
    /// `equal` (absent) or `constant_area`.
    #[serde(default, skip_serializing_if = "PassScheduleDto::is_equal")]
    pub pass_schedule: PassScheduleDto,
    /// Radial depth of a finishing pass of its own, mm (absent: none).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub finish_depth: f64,
    /// Repeats of the final radius (absent: none).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub spring_passes: u32,
    /// Radial depth left short of the standard thread depth, mm.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub depth_allowance: f64,
    /// The spindle runs M4 (absent: M3).
    #[serde(default, skip_serializing_if = "is_false")]
    pub spindle_reverse: bool,
    /// The mill is left-hand cut (absent: right-hand cut).
    #[serde(default, skip_serializing_if = "is_false")]
    pub cutter_left_hand: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PassScheduleDto {
    #[default]
    Equal,
    ConstantArea,
}

impl PassScheduleDto {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn is_equal(&self) -> bool {
        *self == PassScheduleDto::Equal
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

impl From<ThreadPasses> for ThreadPassesDto {
    fn from(p: ThreadPasses) -> Self {
        Self {
            pass_schedule: match p.schedule {
                PassSchedule::Equal => PassScheduleDto::Equal,
                PassSchedule::ConstantArea => PassScheduleDto::ConstantArea,
            },
            finish_depth: p.finish,
            spring_passes: p.spring,
            depth_allowance: p.allowance,
            spindle_reverse: p.spindle_reverse,
            cutter_left_hand: p.cutter_left,
        }
    }
}

impl From<ThreadPassesDto> for ThreadPasses {
    fn from(d: ThreadPassesDto) -> Self {
        Self {
            schedule: match d.pass_schedule {
                PassScheduleDto::Equal => PassSchedule::Equal,
                PassScheduleDto::ConstantArea => PassSchedule::ConstantArea,
            },
            finish: d.finish_depth,
            spring: d.spring_passes,
            allowance: d.depth_allowance,
            spindle_reverse: d.spindle_reverse,
            cutter_left: d.cutter_left_hand,
        }
    }
}
