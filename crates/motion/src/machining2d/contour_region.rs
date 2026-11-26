//! Provenance: single-region entry and depth ladder extracted from contour.rs for #2502.

use crate::cl::{PathBuilder, Toolpath};
use crate::geo2d::{Loop, Region};
use crate::geometry::{Point3, Triangle};

use super::contour::{ContourParams, emit_band, finish_contour, prepare_band};
use super::contour_comp;
use super::contour_depth::{Ladder, ladder, ladder_picked, picked_law};
use super::contour_start;
use super::contour_step::Said;
use super::face::GenError;
use super::fillet_tally::FilletTally;

/// Offset-contour `region` (a material cross-section): every level walks
/// the cutter-center loops of the region grown by `r + leave`, so the
/// tool edge grazes the boundary with `leave = 0` and stops a finishing
/// allowance short of it otherwise. The plan phrases this as
/// `- (r - leave)`; that is the same curve written from the void side -
/// here the region is material, so both outer walls and island/hole
/// walls come out of one positive offset. [`super::contour::Side::Inside`] negates that
/// one term - the region erodes and each loop traverses the other way, so
/// the cutter rides the boundary from the void side with the material on
/// the same side of travel (Auto is this Outside law inside the
/// generator). Climb keeps the material on the right (M3, #1555): outside
/// rings run clockwise, inside rings counter-clockwise. Loop order is
/// deterministic (largest absolute area first, canonically oriented, and
/// traversed per the climb/side rule).
///
/// Between subpaths (tab hops) and between loops of one level the tool
/// retracts to the hop plane - `retract_z`, clamped to `clearance_z` so
/// a legacy retract parked above clearance cannot make the hop climb
/// higher than the clearance move it replaced, with the clamp echoed in
/// the panel - rapids across in air, and plunges again. A level change
/// still lifts to full `clearance_z`: the new level engages material the
/// previous one never touched, and crosses into it by the `entry`
/// family (M8.2 A) - every arm certified against the one center this
/// ladder shares, on the bore side where that center is bounded air.
/// Rapids inside the footprint below the
/// safe plane are the verifier's business (step 7), flagged, never
/// hidden.
pub fn contour(region: &Region, p: &ContourParams) -> Result<Toolpath, GenError> {
    contour_impl(region, p, None)
}

/// [`contour`] of a region sectioned off a finished part: each ring is
/// walked only at the levels where the part's wall it trims still
/// exists (`contour_depth`) - a blind feature's ring ends at its floor
/// plus the stock to leave, and the dropped levels are named in the
/// notes. Through features and outer walls keep every level, so a part
/// whose rings all run the full depth posts exactly [`contour`]'s path.
pub fn contour_on_part(
    region: &Region,
    part: &[Triangle],
    p: &ContourParams,
) -> Result<Toolpath, GenError> {
    contour_impl(region, p, Some((part, &[], false)))
}

/// [`contour_on_part`] of a part that is the union of touching bodies
/// (M13.11): `bodies` are the per-solid index ranges of `part`, and the
/// depth ladder reads each plane as the 2D union of the bodies' own
/// sections - two models butted face to face cut every level the one
/// block they stand for cuts. Fewer than two ranges is
/// [`contour_on_part`], byte for byte.
pub fn contour_on_bodies(
    region: &Region,
    part: &[Triangle],
    bodies: &[std::ops::Range<usize>],
    p: &ContourParams,
) -> Result<Toolpath, GenError> {
    contour_impl(region, p, Some((part, bodies, false)))
}

/// [`contour_on_bodies`] of rings the user PICKED (#1927): a ring that
/// meets its wall at no level below the part's top - the chosen side
/// stands in the part all the way down - is refused by name, never a
/// silent top-plane pass in air (`contour_depth::picked_law`).
pub fn contour_on_picked(
    region: &Region,
    part: &[Triangle],
    bodies: &[std::ops::Range<usize>],
    p: &ContourParams,
) -> Result<Toolpath, GenError> {
    contour_impl(region, p, Some((part, bodies, true)))
}

/// The part a contour reads: its facets, its body ranges, and whether the
/// rings were picked (the #1927 law applies).
type PartRef<'a> = (&'a [Triangle], &'a [std::ops::Range<usize>], bool);

fn contour_impl(region: &Region, p: &ContourParams, part: Option<PartRef<'_>>) -> Result<Toolpath, GenError> {
    p.validate()?;
    // Control-side compensation (M13.11): its parameter door first.
    contour_comp::door(p)?;
    let band = prepare_band(region, p)?;
    let refs: Vec<&Loop> = band.loops.iter().collect();
    // #1448: the user's start point re-sequences its ring instead.
    let pick = contour_start::pick_ring(&refs, p)?;
    let exit = contour_start::pick_exit(&refs, p)?; // #1473
    let flat = p.ring_levels()?;
    let r = p.tool_diameter * 0.5;
    let d = r + p.stock_to_leave;
    // Lead shrinks, refusals and reseated starts, each said once.
    let mut lead_notes: Vec<String> = Vec::new();
    // Which ring is cut at which level: all of them at every level for
    // a flat region; against a part, each ring down to its own floor.
    let lad = match part {
        None => Ladder::uniform(&flat, band.loops.len()),
        Some((f, bodies, true)) => {
            ladder_picked(&band.plan, f, bodies, &flat, p.top_z, r, p.stock_to_leave, &mut lead_notes)
        }
        Some((f, bodies, false)) => ladder(&band.plan, f, bodies, &flat, p.top_z, r, p.stock_to_leave, &mut lead_notes),
    };
    // A picked ring on the wrong side is refused by its own name first
    // (#1927), also when it was the only ring (#2004: with no air level
    // at Top Z, such a ring walks no level at all).
    if let Some((f, _, true)) = part {
        picked_law(&band.plan, f, &flat, &lad, d, p.side)?;
    }
    if lad.zs.is_empty() {
        // Every ring stands in the part at its first level: nothing is
        // legal to cut, and an empty path would read as a clean one.
        return Err(GenError::Invalid(format!(
            "every contour ring cuts into the part at its first level - nothing to machine \
             without a gouge ({})",
            lead_notes.join("; ")
        )));
    }
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut said = Said::default();
    let mut tally = FilletTally::default();
    emit_band(&mut b, &band, p, &lad, pick, exit, &mut said, &mut lead_notes, &mut tally)?;
    finish_contour(b, lead_notes, &tally, p)
}
