//! #1759: arc turns at a Slot's row ends - each corner of the serpentine pass rides one tangent arc (the fillet of `fillet.rs`: the circle tangent to two intersecting lines, emitted as RS-274 / ISO 6983 G2/G3); clean-room, no vendor material.
//!
//! # Why a slot's corners round safely
//!
//! The pass lives in the cutter-center stadium (`slots.rs`), a convex
//! area whose boundary is the tool standing on the wall. Each fillet
//! lies inside the triangle of its corner and its two tangent points
//! (`fillet.rs`), and a triangle of three points of a convex area is in
//! that area: no arc center leaves the stadium, so no arc cuts past the
//! wall. `fillet_corners` proves the same triangle against the
//! tessellated stadium again (inscribed, so it can only refuse) and
//! keeps a corner it cannot prove sharp, by name.
//!
//! The arc pulls the tool off a row end, which stands on the round end
//! of the stadium: the rounding leaves corner rest there, never a
//! gouge. The note states the worst rest for the stock sim to measure.

use crate::cl::PathBuilder;
use crate::geo2d::{Point2, Region};

use super::arcs::{ARC_FIT_TOL, Seg};
use super::face::GenError;
use super::fillet::{Legal, fillet_corners};
use super::fillet_tally::{corner_arc_radius, fillet_floor};
use super::slots::SlotParams;

/// The option's door: with arc turns on, the radius must be one a
/// control can cut and no larger than the tool radius. Off, the radius
/// is never read.
pub(super) fn door(p: &SlotParams) -> Result<(), GenError> {
    if p.blend_turns {
        corner_arc_radius(p.blend_radius, p.tool_diameter).map_err(GenError::Invalid)?;
    }
    Ok(())
}

/// The serpentine `pass` with its row-end corners rounded, as the walk
/// past its first point - or `None` where nothing rounds (the option is
/// off, the slot has one row, or no corner could be proved), so the
/// caller posts the sharp pass unchanged. `center` is the cutter-center
/// stadium. Every refusal is a note on `b`.
pub(super) fn round(pass: &[Point2], p: &SlotParams, center: Option<&Region>, b: &mut PathBuilder) -> Option<Vec<Seg>> {
    if !p.blend_turns {
        return None;
    }
    let (Some(region), Some(&start), true) = (center, pass.first(), pass.len() > 2) else {
        b.note("turn blends: a slot of one row has no corner to round - the pass is unchanged");
        return None;
    };
    let rho = corner_arc_radius(p.blend_radius, p.tool_diameter).ok()?;
    let legal = Legal { region, inside: true, air_left: None };
    let base = vec![Seg::Line(pass.get(1..).unwrap_or(&[]).to_vec())];
    let (walk, tally) = fillet_corners(&base, start, rho, ARC_FIT_TOL, &legal);
    for n in tally.notes("slot row-end", fillet_floor(ARC_FIT_TOL)) {
        b.note(n);
    }
    (tally.rounded > 0).then_some(walk)
}

/// The one walk of the slot's pass: the rows (rounded by `round` when it
/// gave a walk, else straight past the first point), then the #2906 wall
/// lap when there is one. The rows end on the tangent point the lap starts
/// from, rounded or not: a fillet never moves a walk's end point.
pub(super) fn with_lap(rows: &[Point2], turns: Option<Vec<Seg>>, lap: Option<Vec<Seg>>) -> Option<Vec<Vec<Seg>>> {
    let mut walk = match (turns, &lap) {
        (Some(w), _) => w,
        (None, Some(_)) => vec![Seg::Line(rows.get(1..).unwrap_or(&[]).to_vec())],
        (None, None) => return None,
    };
    walk.extend(lap.unwrap_or_default());
    Some(vec![walk])
}
