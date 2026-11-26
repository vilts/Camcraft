//! The Trochoidal slot operation's file mirror (#220): the params as
//! plain keys, the return as a lowercase word with its lift height.
//! Provenance: our own file format (`project/mod.rs`); the names are ours.

use super::*;

use crate::machining2d::TrochoidReturn;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrochoidReturnDto {
    Microlift,
    Retract,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TrochoidDto {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub width: f64,
    /// Absent: all the room the channel leaves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_width: Option<f64>,
    pub step: f64,
    pub max_engagement: f64,
    #[serde(rename = "return")]
    pub ret: TrochoidReturnDto,
    /// The microlift height; read only by a microlift return.
    pub lift: f64,
    pub smoothing: f64,
    pub stock_to_leave: f64,
    pub clearance_z: HeightDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retract_z: Option<HeightDto>,
    pub top_z: HeightDto,
    pub bottom_z: HeightDto,
    pub stepdown: f64,
    pub feed: f64,
    pub return_feed: f64,
    pub plunge_feed: f64,
    pub climb: bool,
    #[serde(default, skip_serializing_if = "PathPicksDto::is_empty")]
    pub picks: PathPicksDto,
}

impl TrochoidDto {
    /// The trochoid's keys; any other kind (never passed - the caller
    /// matched the trochoid) files the trochoid's own draft numbers.
    pub(super) fn from_params(p: &OpParams) -> Self {
        let OpParams::TrochoidalSlot {
            a,
            b,
            width,
            loop_width,
            step,
            max_engagement,
            ret,
            smoothing,
            stock_to_leave,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            return_feed,
            plunge_feed,
            climb,
            picks,
        } = p
        else {
            return TrochoidDto::from_params(&crate::operation::retype_params(
                p,
                crate::operation::OpType::TrochoidalSlot.ordinal(),
            ));
        };
        let (ret, lift) = match ret {
            TrochoidReturn::Microlift { height } => (TrochoidReturnDto::Microlift, *height),
            TrochoidReturn::Retract => (TrochoidReturnDto::Retract, crate::machining2d::DEFAULT_MICROLIFT),
        };
        TrochoidDto {
            a: [a.x, a.y],
            b: [b.x, b.y],
            width: *width,
            loop_width: *loop_width,
            step: *step,
            max_engagement: *max_engagement,
            ret,
            lift,
            smoothing: *smoothing,
            stock_to_leave: *stock_to_leave,
            clearance_z: (*clearance_z).into(),
            retract_z: (*retract_z).map(Into::into),
            top_z: (*top_z).into(),
            bottom_z: (*bottom_z).into(),
            stepdown: *stepdown,
            feed: *feed,
            return_feed: *return_feed,
            plunge_feed: *plunge_feed,
            climb: *climb,
            picks: PathPicksDto::from_picks(picks),
        }
    }

    pub(super) fn to_params(&self) -> OpParams {
        OpParams::TrochoidalSlot {
            a: crate::geo2d::Point2::new(self.a[0], self.a[1]),
            b: crate::geo2d::Point2::new(self.b[0], self.b[1]),
            width: self.width,
            loop_width: self.loop_width,
            step: self.step,
            max_engagement: self.max_engagement,
            ret: match self.ret {
                TrochoidReturnDto::Microlift => TrochoidReturn::Microlift { height: self.lift },
                TrochoidReturnDto::Retract => TrochoidReturn::Retract,
            },
            smoothing: self.smoothing,
            stock_to_leave: self.stock_to_leave,
            clearance_z: self.clearance_z.into(),
            retract_z: self.retract_z.map(Into::into),
            top_z: self.top_z.into(),
            bottom_z: self.bottom_z.into(),
            stepdown: self.stepdown,
            feed: self.feed,
            return_feed: self.return_feed,
            plunge_feed: self.plunge_feed,
            climb: self.climb,
            picks: self.picks.to_picks(),
        }
    }
}
