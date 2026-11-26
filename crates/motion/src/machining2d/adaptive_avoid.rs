//! 2D Adaptive with "Do not touch" keep-outs (#3346), both methods.
//! Provenance: our own region difference over `pocket` / `adaptive_troch`
//! and the full-tool keep-out of `verify::avoid` (#2055); no vendor
//! material consulted.
//!
//! The law is the same for the two methods. The keep-out is one region:
//! it covers each tip height from the floor up to the clearance plane, a
//! span that holds every cut, every entry and every descent of the path.
//! The planner cuts its cutter-centre area back by that region and plans
//! as it does without protection, so its own checks (the crossing law,
//! the entry and link certificates, the confinement of the loops) read
//! the reduced area. Rapid links are lifted over the items. A level above
//! a low protected item loses the same area as the floor level: this costs
//! cut area, never safety.

use camcraft_sim::verify::{AVOID_PAD, AvoidSpec, elevate_rapid_links};

use crate::cl::Toolpath;
use crate::geo2d::Region;

use super::adaptive_troch::{AdaptiveTrochParams, adaptive_trochoidal_with, trochoidal_inside};
use super::corner_relief::{CornerRelief, capsule_region, plan_corner_reliefs};
use super::gen_error::GenError;
use super::pass_start::StartSide;
use super::pocket::{PocketParams, non_sliver_center, pocket_over_with_relief, pocket_started_on};

/// The keep-out of `avoid` over the full height of the path, or `None`
/// when it does not reach the cutter-centre area of `region`: then the
/// plain planner runs and its path stays the one it always was.
fn keepout_for(region: &Region, p: &PocketParams, avoid: &AvoidSpec) -> Result<Option<Region>, GenError> {
    if avoid.is_empty() {
        return Ok(None);
    }
    let lo = p.bottom_z.min(p.top_z);
    let hi = p.clearance_z.max(p.top_z);
    let ko = avoid.keepout_region(lo, hi, AVOID_PAD).map_err(GenError::Invalid)?;
    if ko.is_empty() || region.intersect(&ko.offset(0.5 * p.tool_diameter + p.stock_to_leave)?)?.is_empty() {
        return Ok(None);
    }
    Ok(Some(ko))
}

/// Lift the rapid links over the items; a link that cannot clear them
/// refuses the path. Then say what the protection did.
fn finish(mut path: Toolpath, avoid: &AvoidSpec, what: &str) -> Result<Toolpath, GenError> {
    let names = avoid.names_list();
    let mut notes = elevate_rapid_links(&mut path, avoid, None, AVOID_PAD);
    if !notes.is_empty() {
        notes.insert(0, format!("Do not touch ({names}): a rapid link cannot clear the protected items"));
        return Err(GenError::Invalid(notes.join("; ")).with_ghost(&path));
    }
    path.notes.push(format!("Do not touch ({names}): {what} keep the full tool clear at every level"));
    Ok(path)
}

/// 2D Adaptive's Spiral method (`pocket_entered_with_relief` with the
/// Adaptive strategy) whose full tool (flutes, shank and holder) stays
/// clear of each item of `avoid` by its clearance. A region that the
/// keep-out leaves with no room is refused, and the refusal names the items.
pub fn adaptive_spiral_avoid(
    region: &Region,
    p: &PocketParams,
    avoid: &AvoidSpec,
    relief: &CornerRelief,
) -> Result<Toolpath, GenError> {
    adaptive_spiral_avoid_on(region, p, avoid, relief, &StartSide::default())
}

/// [`adaptive_spiral_avoid`] with the side each level starts on (#3355):
/// the plan reads `side` over the area that the keep-out leaves, as
/// [`pocket_started_on`] reads it over the full area.
pub fn adaptive_spiral_avoid_on(
    region: &Region,
    p: &PocketParams,
    avoid: &AvoidSpec,
    relief: &CornerRelief,
    side: &StartSide,
) -> Result<Toolpath, GenError> {
    let Some(ko) = keepout_for(region, p, avoid)? else {
        return pocket_started_on(region, p, &[], relief, side);
    };
    p.validate()?;
    relief.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let names = avoid.names_list();
    let off = p.tool_diameter * 0.5 + p.stock_to_leave;
    let center = non_sliver_center(region, off)?;
    if center.is_empty() {
        return Err(GenError::Invalid("pocket is narrower than the tool".into()));
    }
    let plan = plan_corner_reliefs(region, &center, off, relief)?;
    for r in &plan.reliefs {
        if !capsule_region(r.start, r.end, r.radius)?.intersect(&ko)?.is_empty() {
            return Err(GenError::Invalid(format!(
                "corner relief at ({:.3}, {:.3}) enters protected 'Do not touch' zone ({names})",
                r.corner.x, r.corner.y
            )));
        }
    }
    let center = center.difference(&ko)?;
    if center.is_empty() {
        return Err(GenError::Invalid(format!(
            "cannot cut clear of protected 'Do not touch' {names}: no safe cutting room remains"
        )));
    }
    let path = pocket_over_with_relief(region, &center, p, Vec::new(), false, &[], &plan.reliefs, side)
        .map_err(|e| e.map_msg(|m| format!("{m} - the 'Do not touch' keep-out of {names} takes part of the region")))?;
    finish(path, avoid, "the passes")
}

/// [`adaptive_trochoidal_with`] whose full tool stays clear of each item
/// of `avoid` by its clearance. A region that the keep-out leaves with no
/// lane is refused, and the refusal names the items.
pub fn adaptive_trochoidal_avoid(
    region: &Region,
    p: &PocketParams,
    t: &AdaptiveTrochParams,
    avoid: &AvoidSpec,
) -> Result<Toolpath, GenError> {
    let Some(ko) = keepout_for(region, p, avoid)? else {
        return adaptive_trochoidal_with(region, p, t);
    };
    let names = avoid.names_list();
    let path = trochoidal_inside(region, p, t, Some(&ko))
        .map_err(|e| e.map_msg(|m| format!("{m} - the 'Do not touch' keep-out of {names} takes part of the region")))?;
    finish(path, avoid, "the trochoidal lanes")
}
