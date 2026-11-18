//! The Trochoidal slot operation's door (#220): op params to
//! [`crate::machining2d::TrochoidParams`] with the resolved heights and
//! the mounted tool's diameter. The generator (provenance there:
//! Otkur & Lazoglu 2007; Rauch, Duc & Hascoet 2009) does the rest.
//! Ours; no vendor material.

use crate::machining2d::{GenError, TrochoidParams};

use super::types::OpParams;

/// The generator's numbers for a trochoidal slot op.
pub(super) fn trochoid_params(
    params: &OpParams,
    tool_diameter: f64,
    clearance_z: f64,
    retract_z: Option<f64>,
    top_z: f64,
    bottom_z: f64,
    stock_top: Option<f64>,
) -> Result<TrochoidParams, GenError> {
    let OpParams::TrochoidalSlot {
        width,
        loop_width,
        step,
        max_engagement,
        ret,
        smoothing,
        stock_to_leave,
        stepdown,
        feed,
        return_feed,
        plunge_feed,
        climb,
        ..
    } = params
    else {
        return Err(GenError::Invalid("not a trochoidal slot operation".into()));
    };
    Ok(TrochoidParams {
        tool_diameter,
        width: *width,
        loop_width: *loop_width,
        step: *step,
        max_engagement: *max_engagement,
        ret: *ret,
        smoothing: *smoothing,
        stock_to_leave: *stock_to_leave,
        clearance_z,
        retract_z,
        top_z,
        bottom_z,
        stepdown: *stepdown,
        feed: *feed,
        return_feed: *return_feed,
        plunge_feed: *plunge_feed,
        climb: *climb,
        stock_top,
    })
}
