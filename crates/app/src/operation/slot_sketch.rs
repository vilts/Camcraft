//! #1868: the Slot and the Trochoidal Slot read off picked DXF sketch
//! chains. One closed line-arc slot outline is the room (its ends and
//! width seed the slot); one open straight chain is the centerline (its
//! ends seed the slot, the typed width stands). Every other pick is
//! refused by name - a slot is never guessed from an outline that is
//! not one.
//!
//! Provenance: our own law over `geometry::sketch_chains`, placed in
//! the world by `sketch::world_pose`; the plane geometry is
//! `machining2d::slot_room`'s. No vendor material consulted.

use flecs_ecs::prelude::World;

use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::{GenError, SlotBound};

use super::contour_sketch_pick::{transform_chain_pts, visit_src_chains};
use super::{GeoSel, OpParams, SrcRef};

/// Chord sag of a sampled arc, mm: well inside the slot room's own fit
/// tolerance, so a sampled round end takes nothing off the width, and
/// under the CL resolution (1 um), so the seeded ends are to size.
const SAMPLE_TOL: f64 = 1e-4;
/// The least share of a closed outline that its fitted slot covers. A
/// line-arc slot loop covers all of it; a rectangle, an L or a keyhole
/// leaves corners and is no slot outline.
const OUTLINE_COVER: f64 = 0.995;

/// Does a pick list name a DXF profile?
fn picks_sketch(world: &World, items: &[GeoSel]) -> bool {
    items.iter().any(|g| crate::sketch::find(world, &g.entity).is_some())
}

/// True for a Slot or Trochoidal Slot whose picks are DXF sketch chains.
#[must_use]
pub(crate) fn is_sketch_slot(world: &World, src: &SrcRef, params: &OpParams) -> bool {
    matches!(params, OpParams::Slot { .. } | OpParams::TrochoidalSlot { .. })
        && matches!(src, SrcRef::Selection(items) if picks_sketch(world, items))
}

/// The slot room of sketch picks, in world XY; `None` when the picks
/// name no DXF profile (the model picks' law reads them).
pub(super) fn bound_of(world: &World, items: &[GeoSel]) -> Option<Result<SlotBound, GenError>> {
    picks_sketch(world, items).then(|| resolve(world, items))
}

fn bare(e: &GenError) -> String {
    let s = e.to_string();
    s.strip_prefix("invalid machining parameters: ").map(str::to_owned).unwrap_or(s)
}

fn resolve(world: &World, items: &[GeoSel]) -> Result<SlotBound, GenError> {
    let mut chains: Vec<(String, bool, Vec<Point2>)> = Vec::new();
    visit_src_chains(world, &SrcRef::Selection(items.to_vec()), |entity, pose, chain, label| {
        let Some(pose) = pose else {
            return Err(GenError::Invalid(format!("DXF profile '{entity}' has no stock to lie on - set up stock first")));
        };
        chains.push((label, chain.is_closed(), transform_chain_pts(pose, &chain.sample(SAMPLE_TOL))?));
        Ok(())
    })?;
    let [(label, closed, pts)] = chains.as_slice() else {
        return Err(GenError::Invalid(format!(
            "a slot on a DXF profile cuts one chain, and this selection has {} chains - pick one closed slot outline or one open centerline for each operation",
            chains.len()
        )));
    };
    if !*closed {
        return SlotBound::from_centerline(pts).map_err(|e| GenError::Invalid(format!("{label}: {}", bare(&e))));
    }
    let not_slot = |why: String| {
        GenError::Invalid(format!(
            "{label} is not a slot outline: {why} - pick a closed line-arc slot loop (two parallel sides and two half-circle ends) or an open straight centerline"
        ))
    };
    let ring = Loop::new(pts.clone()).map_err(|e| not_slot(e.to_string()))?;
    let region = Region::new(vec![ring]).map_err(|e| not_slot(e.to_string()))?;
    let area = region.area().abs();
    let bound = SlotBound::from_face(region).map_err(|e| not_slot(bare(&e)))?;
    let fit = bound.fit().map_err(|e| not_slot(bare(&e)))?;
    let len = (fit.b.x - fit.a.x).hypot(fit.b.y - fit.a.y);
    let stadium = len * fit.width + std::f64::consts::FRAC_PI_4 * fit.width * fit.width;
    // A NaN on either side refuses.
    if stadium.is_nan() || area.is_nan() || stadium < OUTLINE_COVER * area {
        return Err(not_slot(format!(
            "the largest straight slot inside it, {:.3} mm long and {:.3} mm wide, covers only {:.1}% of it",
            len + fit.width,
            fit.width,
            100.0 * stadium / area
        )));
    }
    Ok(bound)
}
