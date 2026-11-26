//! The Slot Undercut operation's file mirror (#1444): the parameters as
//! plain keys. An appended kind: no older file names it, so it takes no
//! format rung.
//! Provenance: our own file format (`project/mod.rs`); the names are ours.

use super::*;

use crate::operation::SlotUndercutOp;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SlotUndercutDto {
    pub a: [f64; 2],
    pub b: [f64; 2],
    /// 0.0: a straight slot.
    #[serde(default)]
    pub bulge: f64,
    pub width: f64,
    pub run_in: f64,
    /// 0.0: nothing stands above the neck's path.
    #[serde(default)]
    pub neck_slot: f64,
    pub clearance_z: HeightDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retract_z: Option<HeightDto>,
    pub top_z: HeightDto,
    pub bottom_z: HeightDto,
    pub stepover: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub climb: bool,
    #[serde(default, skip_serializing_if = "PathPicksDto::is_empty")]
    pub picks: PathPicksDto,
}

impl SlotUndercutDto {
    pub(super) fn from_op(p: &SlotUndercutOp) -> Self {
        SlotUndercutDto {
            a: [p.a.x, p.a.y],
            b: [p.b.x, p.b.y],
            bulge: p.bulge,
            width: p.width,
            run_in: p.run_in,
            neck_slot: p.neck_slot,
            clearance_z: p.clearance_z.into(),
            retract_z: p.retract_z.map(Into::into),
            top_z: p.top_z.into(),
            bottom_z: p.bottom_z.into(),
            stepover: p.stepover,
            feed: p.feed,
            plunge_feed: p.plunge_feed,
            climb: p.climb,
            picks: PathPicksDto::from_picks(&p.picks),
        }
    }

    pub(super) fn to_params(&self) -> OpParams {
        OpParams::SlotUndercut(SlotUndercutOp {
            a: crate::geo2d::Point2::new(self.a[0], self.a[1]),
            b: crate::geo2d::Point2::new(self.b[0], self.b[1]),
            bulge: self.bulge,
            width: self.width,
            run_in: self.run_in,
            neck_slot: self.neck_slot,
            clearance_z: self.clearance_z.into(),
            retract_z: self.retract_z.map(Into::into),
            top_z: self.top_z.into(),
            bottom_z: self.bottom_z.into(),
            stepover: self.stepover,
            feed: self.feed,
            plunge_feed: self.plunge_feed,
            climb: self.climb,
            picks: self.picks.to_picks(),
        })
    }
}
