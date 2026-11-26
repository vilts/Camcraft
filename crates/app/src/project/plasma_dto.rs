//! The plasma profile's file mirror (#670): the op's parameters as
//! plain keys (blank options absent), and the cached path's plasma
//! record (kerf, torch spans, nesting) beside the moves - the additive
//! law of the comps / threads / probes keys: absent is none.
//! Provenance: our own file format (`project/mod.rs`); the names are ours.

use super::*;

use crate::cl::{ContourRole, Move, PlasmaCut, TorchSpan};
use crate::operation::{PlasmaComp, PlasmaOp};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlasmaCompDto {
    Computer,
    Control,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlasmaDto {
    pub clearance_z: HeightDto,
    pub top_z: HeightDto,
    pub bottom_z: HeightDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kerf: Option<f64>,
    pub comp: PlasmaCompDto,
    #[serde(default)]
    pub reversed: bool,
    pub lead_in: f64,
    pub lead_out: f64,
    pub small_hole_dia: f64,
    pub small_hole_feed: f64,
    pub overburn: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pierce_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pierce_delay: Option<f64>,
    #[serde(default, skip_serializing_if = "PathPicksDto::is_empty")]
    pub picks: PathPicksDto,
}

impl PlasmaDto {
    pub(super) fn from_op(p: &PlasmaOp) -> Self {
        PlasmaDto {
            clearance_z: p.clearance_z.into(),
            top_z: p.top_z.into(),
            bottom_z: p.bottom_z.into(),
            kerf: p.kerf,
            comp: match p.comp {
                PlasmaComp::Computer => PlasmaCompDto::Computer,
                PlasmaComp::Control => PlasmaCompDto::Control,
            },
            reversed: p.reversed,
            lead_in: p.lead_in,
            lead_out: p.lead_out,
            small_hole_dia: p.small_hole_dia,
            small_hole_feed: p.small_hole_feed,
            overburn: p.overburn,
            feed: p.feed,
            pierce_height: p.pierce_height,
            cut_height: p.cut_height,
            pierce_delay: p.pierce_delay,
            picks: PathPicksDto::from_picks(&p.picks),
        }
    }

    pub(super) fn to_params(&self) -> OpParams {
        OpParams::PlasmaProfile(PlasmaOp {
            clearance_z: self.clearance_z.into(),
            top_z: self.top_z.into(),
            bottom_z: self.bottom_z.into(),
            kerf: self.kerf,
            comp: match self.comp {
                PlasmaCompDto::Computer => PlasmaComp::Computer,
                PlasmaCompDto::Control => PlasmaComp::Control,
            },
            reversed: self.reversed,
            lead_in: self.lead_in,
            lead_out: self.lead_out,
            small_hole_dia: self.small_hole_dia,
            small_hole_feed: self.small_hole_feed,
            overburn: self.overburn,
            feed: self.feed,
            pierce_height: self.pierce_height,
            cut_height: self.cut_height,
            pierce_delay: self.pierce_delay,
            picks: self.picks.to_picks(),
        })
    }
}

/// One cached torch span.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TorchSpanDto {
    pub contour: u32,
    /// `outer` / `inner` / `open`.
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u32>,
    pub on: u64,
    pub off: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small: Option<f64>,
}

/// The cached plasma record of a path.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlasmaCutDto {
    pub kerf: f64,
    #[serde(default)]
    pub control_comp: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pierce_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pierce_delay: Option<f64>,
    #[serde(default)]
    pub auto_feed: bool,
    #[serde(default)]
    pub reversed: bool,
    pub torches: Vec<TorchSpanDto>,
}

impl PlasmaCutDto {
    pub(super) fn from_cut(p: &PlasmaCut) -> Self {
        PlasmaCutDto {
            kerf: p.kerf,
            control_comp: p.control_comp,
            pierce_height: p.pierce_height,
            cut_height: p.cut_height,
            pierce_delay: p.pierce_delay,
            auto_feed: p.auto_feed,
            reversed: p.reversed,
            torches: p
                .torches
                .iter()
                .map(|t| TorchSpanDto {
                    contour: t.contour,
                    role: t.role.word().to_ascii_lowercase(),
                    parent: t.parent,
                    on: t.on as u64,
                    off: t.off as u64,
                    small: t.small,
                })
                .collect(),
        }
    }

    /// The record of a loaded path - or none when any span does not sit
    /// on the moves (the op then reads stale and re-generates; a torch
    /// record is never half-trusted).
    pub(super) fn load(&self, moves: &[Move]) -> Option<PlasmaCut> {
        let torches = self
            .torches
            .iter()
            .map(|t| {
                Some(TorchSpan {
                    contour: t.contour,
                    role: ContourRole::from_word(&t.role.to_ascii_uppercase())?,
                    parent: t.parent,
                    on: usize::try_from(t.on).ok()?,
                    off: usize::try_from(t.off).ok()?,
                    small: t.small,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let cut = PlasmaCut {
            kerf: self.kerf,
            control_comp: self.control_comp,
            pierce_height: self.pierce_height,
            cut_height: self.cut_height,
            pierce_delay: self.pierce_delay,
            auto_feed: self.auto_feed,
            reversed: self.reversed,
            torches,
        };
        cut.is_sane(moves).then_some(cut)
    }
}
