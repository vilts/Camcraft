//! The threading infeed plan (issue #416, `crate::turning::ThreadInfeed`)
//! in the project file. Our own format; the additive law the approach
//! and holes keys keep: absent (and written only when not the default)
//! is the radial constant-depth ladder every earlier file cut - no
//! version step.

use serde::{Deserialize, Serialize};

use crate::turning::{DepthSchedule, InfeedMethod, ThreadDips, ThreadInfeed};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InfeedMethodDto {
    #[default]
    Radial,
    Flank,
    Alternating,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DepthScheduleDto {
    #[default]
    ConstantDepth,
    ConstantArea,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ThreadInfeedDto {
    pub method: InfeedMethodDto,
    pub flank_angle: f64,
    pub schedule: DepthScheduleDto,
    pub min_depth: f64,
    pub finish_allowance: f64,
    pub spring_passes: u32,
    /// #418: absent (and written only when set) is one cut per depth -
    /// every #416 file.
    #[serde(default, skip_serializing_if = "is_false")]
    pub repeat_each_depth: bool,
    /// #419: absent (and written only when not the default) is no
    /// chip-break dips - every #416 / #418 file.
    #[serde(default, skip_serializing_if = "ThreadDipsDto::is_default")]
    pub dips: ThreadDipsDto,
}

/// The chip-break dips (issue #419): interval, radial height and
/// length, mm; interval 0 lays none.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ThreadDipsDto {
    pub interval: f64,
    pub height: f64,
    pub length: f64,
}

impl Default for ThreadDipsDto {
    fn default() -> Self {
        ThreadDips::default().into()
    }
}

impl ThreadDipsDto {
    fn is_default(&self) -> bool {
        *self == ThreadDipsDto::default()
    }
}

impl From<ThreadDips> for ThreadDipsDto {
    fn from(d: ThreadDips) -> Self {
        ThreadDipsDto { interval: d.interval, height: d.height, length: d.length }
    }
}

impl From<ThreadDipsDto> for ThreadDips {
    fn from(d: ThreadDipsDto) -> Self {
        ThreadDips { interval: d.interval, height: d.height, length: d.length }
    }
}

impl Default for ThreadInfeedDto {
    fn default() -> Self {
        ThreadInfeed::default().into()
    }
}

impl ThreadInfeedDto {
    pub(super) fn is_default(&self) -> bool {
        *self == ThreadInfeedDto::default()
    }
}

impl From<ThreadInfeed> for ThreadInfeedDto {
    fn from(p: ThreadInfeed) -> Self {
        ThreadInfeedDto {
            method: match p.method {
                InfeedMethod::Radial => InfeedMethodDto::Radial,
                InfeedMethod::Flank => InfeedMethodDto::Flank,
                InfeedMethod::Alternating => InfeedMethodDto::Alternating,
            },
            flank_angle: p.flank_angle,
            schedule: match p.schedule {
                DepthSchedule::ConstantDepth => DepthScheduleDto::ConstantDepth,
                DepthSchedule::ConstantArea => DepthScheduleDto::ConstantArea,
            },
            min_depth: p.min_depth,
            finish_allowance: p.finish_allowance,
            spring_passes: p.spring_passes,
            repeat_each_depth: p.repeat_each_depth,
            dips: p.dips.into(),
        }
    }
}

impl From<ThreadInfeedDto> for ThreadInfeed {
    fn from(d: ThreadInfeedDto) -> Self {
        ThreadInfeed {
            method: match d.method {
                InfeedMethodDto::Radial => InfeedMethod::Radial,
                InfeedMethodDto::Flank => InfeedMethod::Flank,
                InfeedMethodDto::Alternating => InfeedMethod::Alternating,
            },
            flank_angle: d.flank_angle,
            schedule: match d.schedule {
                DepthScheduleDto::ConstantDepth => DepthSchedule::ConstantDepth,
                DepthScheduleDto::ConstantArea => DepthSchedule::ConstantArea,
            },
            min_depth: d.min_depth,
            finish_allowance: d.finish_allowance,
            spring_passes: d.spring_passes,
            repeat_each_depth: d.repeat_each_depth,
            dips: d.dips.into(),
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}
