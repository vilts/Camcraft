//! The Slot Undercut at the operation layer (#1444): the draft a new operation starts from, and the bridge from the document's stored parameters and tool record to the motion crate's generator (`machining2d::undercut`). Our own glue; no vendor material consulted.

use camcraft_tools::undercut::{self, Undercut};

use super::kinds::Carried;
use super::types::{OpParams, SlotUndercutOp};
use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::{self, GenError, UndercutParams};
use crate::tools::Tool;

/// The Slot Undercut's draft: the plain Slot's authored line, a 20 mm
/// undercut and a 15 mm run-in. Numbers to edit, never a part's values;
/// the generator refuses them against the mounted cutter by name.
pub(super) fn seed_slot_undercut(c: &Carried) -> OpParams {
    OpParams::SlotUndercut(SlotUndercutOp {
        a: Point2::new(-20.0, 0.0),
        b: Point2::new(20.0, 0.0),
        bulge: 0.0,
        width: 20.0,
        run_in: 15.0,
        neck_slot: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepover: c.stepover,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        picks: crate::path_picks::PathPicks::default(),
    })
}

/// `tool` read as an undercut cutter, or the refusal by name: another
/// kind of tool, or numbers the cutter law refuses.
pub fn cutter_of(name: &str, tool: &Tool) -> Result<Undercut, GenError> {
    if !undercut::is_kind(tool.kind) {
        return Err(GenError::Invalid(format!(
            "Slot Undercut cuts with a T-slot, keyseat, dovetail or lollipop cutter - '{name}' is a {}",
            tool.kind
        )));
    }
    if let Some(why) = undercut::refusal(tool) {
        return Err(GenError::Invalid(format!("Slot Undercut tool '{name}': {why}")));
    }
    Undercut::of(tool).ok_or_else(|| GenError::Invalid(format!("Slot Undercut tool '{name}' reads as no undercut cutter")))
}

/// The generator's parameters for `op` with `cutter` at resolved heights.
fn params_of(op: &SlotUndercutOp, cutter: Undercut, clearance: f64, retract: Option<f64>, bottom: f64) -> UndercutParams {
    UndercutParams {
        cutter,
        width: op.width,
        bulge: op.bulge,
        run_in: op.run_in,
        neck_slot: op.neck_slot,
        clearance_z: clearance,
        retract_z: retract,
        bottom_z: bottom,
        stepover: op.stepover,
        feed: op.feed,
        plunge_feed: op.plunge_feed,
        climb: op.climb,
    }
}

/// Generate `op` with the tool `name` at the resolved heights.
///
/// # Errors
/// [`cutter_of`]'s refusals and every refusal of the generator's door.
pub(super) fn generate(
    op: &SlotUndercutOp,
    name: &str,
    tool: &Tool,
    clearance: f64,
    retract: Option<f64>,
    bottom: f64,
) -> Result<Toolpath, GenError> {
    let cutter = cutter_of(name, tool)?;
    machining2d::undercut(op.a, op.b, &params_of(op, cutter, clearance, retract, bottom))
}

/// The slot's footprint: the band of the authored width about the
/// centreline (square ends).
pub(super) fn footprint(op: &SlotUndercutOp) -> Result<Region, GenError> {
    let pts = machining2d::undercut_band(op.a, op.b, op.bulge, op.width * 0.5)?;
    Ok(Region::new(vec![Loop::new(pts)?])?)
}
