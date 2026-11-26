//! The Multi-Axis Deburr operation's file mirror (#1894): the Deburr's
//! keys and the axis keys. An appended kind: no older file names it,
//! so it takes no format rung.
//! Provenance: our own file format (`project/mod.rs`); the names are ours.

use super::*;

use super::deburr_break_dto::{break_of, default_break_depth, default_passes, BreakModeDto, BreakProfileDto};
use super::unreachable_dto::UnreachableDto;
use crate::multiaxis::DeburrAxisMode;

/// `axis5` (simultaneous tilt) or `axis4` (the rotary plane).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeburrAxisModeDto {
    #[default]
    Axis5,
    Axis4,
}

impl From<DeburrAxisMode> for DeburrAxisModeDto {
    fn from(m: DeburrAxisMode) -> Self {
        match m {
            DeburrAxisMode::Axis5 => DeburrAxisModeDto::Axis5,
            DeburrAxisMode::Axis4 => DeburrAxisModeDto::Axis4,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MultiAxisDeburrDto {
    pub width: f64,
    pub clearance_z: HeightDto,
    pub top_z: HeightDto,
    pub bottom_z: HeightDto,
    pub feed: f64,
    pub plunge_feed: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_edges: Vec<GeoSelDto>,
    pub min_edge_angle_deg: f64,
    pub exclude_holes: bool,
    pub hole_min_diameter: f64,
    pub hole_max_diameter: f64,
    #[serde(default)]
    pub unreachable: UnreachableDto,
    #[serde(default)]
    pub break_mode: BreakModeDto,
    #[serde(default = "default_break_depth")]
    pub break_depth: f64,
    #[serde(default)]
    pub break_profile: BreakProfileDto,
    #[serde(default = "default_passes")]
    pub passes: u32,
    #[serde(default)]
    pub axis_mode: DeburrAxisModeDto,
    pub lead_deg: f64,
    pub lag_deg: f64,
    pub max_tilt_deg: f64,
    pub shank_avoid: bool,
    #[serde(default, skip_serializing_if = "PathPicksDto::is_empty")]
    pub picks: PathPicksDto,
}

impl MultiAxisDeburrDto {
    pub(super) fn to_params(&self) -> OpParams {
        OpParams::MultiAxisDeburr {
            width: self.width,
            clearance_z: self.clearance_z.into(),
            top_z: self.top_z.into(),
            bottom_z: self.bottom_z.into(),
            feed: self.feed,
            plunge_feed: self.plunge_feed,
            exclude_edges: super::area::sel_of(&self.exclude_edges),
            min_edge_angle_deg: self.min_edge_angle_deg,
            exclude_holes: self.exclude_holes,
            hole_min_diameter: self.hole_min_diameter,
            hole_max_diameter: self.hole_max_diameter,
            unreachable: self.unreachable.into(),
            sizing: break_of(self.break_mode, self.break_depth, self.break_profile, self.passes),
            axis_mode: match self.axis_mode {
                DeburrAxisModeDto::Axis5 => DeburrAxisMode::Axis5,
                DeburrAxisModeDto::Axis4 => DeburrAxisMode::Axis4,
            },
            lead_deg: self.lead_deg,
            lag_deg: self.lag_deg,
            max_tilt_deg: self.max_tilt_deg,
            shank_avoid: self.shank_avoid,
            picks: self.picks.to_picks(),
        }
    }
}
