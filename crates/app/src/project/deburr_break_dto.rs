//! The project-file words of the Deburr break sizing (#1890), and the
//! v76 step that turns each older 3D Deburr `width` into a chord.
//!
//! Provenance: our own file mirror of `finishing3d::DeburrBreak` (the
//! project file is this module's concern); the sqrt(2) factor is the
//! chord-to-setback ratio of a 90-degree edge. No vendor material.

use serde::{Deserialize, Serialize};

use super::entity::EntityDto;
use super::params::ParamsDto;
use crate::finishing3d::{BreakMode, BreakProfile, DeburrBreak};

/// `constant_width` (the default, and each file before v76) or
/// `constant_depth`. Another word is refused at load.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BreakModeDto {
    #[default]
    ConstantWidth,
    ConstantDepth,
}

/// `chamfer` (the default, and each file before v76) or `round`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BreakProfileDto {
    #[default]
    Chamfer,
    Round,
}

/// The absent `break_depth` key: the default break's depth.
#[must_use]
pub fn default_break_depth() -> f64 {
    DeburrBreak::default().depth
}

/// The absent `passes` key: one pass, as each file before v76 cut.
#[must_use]
pub fn default_passes() -> u32 {
    DeburrBreak::default().passes
}

/// The file words of `b`: (mode, depth, profile, passes).
#[must_use]
pub fn break_dto(b: &DeburrBreak) -> (BreakModeDto, f64, BreakProfileDto, u32) {
    let mode = match b.mode {
        BreakMode::ConstantWidth => BreakModeDto::ConstantWidth,
        BreakMode::ConstantDepth => BreakModeDto::ConstantDepth,
    };
    let profile = match b.profile {
        BreakProfile::Chamfer => BreakProfileDto::Chamfer,
        BreakProfile::Round => BreakProfileDto::Round,
    };
    (mode, b.depth, profile, b.passes)
}

/// The break the file words state.
#[must_use]
pub fn break_of(mode: BreakModeDto, depth: f64, profile: BreakProfileDto, passes: u32) -> DeburrBreak {
    DeburrBreak {
        mode: match mode {
            BreakModeDto::ConstantWidth => BreakMode::ConstantWidth,
            BreakModeDto::ConstantDepth => BreakMode::ConstantDepth,
        },
        depth,
        profile: match profile {
            BreakProfileDto::Chamfer => BreakProfile::Chamfer,
            BreakProfileDto::Round => BreakProfile::Round,
        },
        passes,
    }
}

/// v75 -> v76 (#1890): a 3D Deburr `width` was the cone's setback at the
/// edge's level; it is now the chamfer chord. On the 90-degree edge each
/// earlier width was measured on, the chord is `sqrt(2)` times the
/// setback, so the step multiplies it and the cut depth stays the same.
pub(super) fn settle_chord_width(entities: &mut [EntityDto]) {
    for row in entities {
        if let Some(op) = &mut row.operation
            && let ParamsDto::Deburr { width, .. } = &mut op.params
        {
            *width *= std::f64::consts::SQRT_2;
        }
    }
}
