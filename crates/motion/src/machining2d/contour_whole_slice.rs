//! Overhead-outline slicing for the whole-model 2D contour (#2436): the
//! region of all part material above a level, and the check for an outline
//! that changes between the last cut level and the next flat level (a step
//! shorter than the stepdown). Provenance: 2.5D Z-level cross-section +
//! downward shadow union and polygon symmetric difference from textbook
//! computational geometry; no vendor material consulted.

use std::ops::Range;

use crate::geo2d::Region;
use crate::geometry::Triangle;
use crate::geometry::slice::{SliceError, material_overhead_of};

use super::contour_params::ContourParams;
use super::face::GenError;

const SAME_LEVEL: f64 = 1e-6;
const SLICE_NUDGE: f64 = 1e-3;
/// Symmetric-difference area (mm^2) under which two outlines are the same.
const SAME_OUTLINE_AREA: f64 = 1e-2;

fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        e @ (SliceError::Open { .. } | SliceError::NonManifold { .. }) => GenError::Model(e.to_string()),
        other => GenError::Invalid(other.to_string()),
    }
}

/// All part material at or above `probe_z` (section strictly above plus shadow
/// of overhanging down-facing walls), nudged inside `cut_z` when `probe_z` sits
/// at the top of the model.
pub(super) fn whole_region_at(
    part: &[Triangle],
    bodies: &[Range<usize>],
    probe_z: f64,
    cut_z: f64,
) -> Result<Region, GenError> {
    let r = material_overhead_of(part, bodies, probe_z).map_err(from_slice)?;
    if !r.is_empty() {
        return Ok(r);
    }
    material_overhead_of(part, bodies, cut_z - SLICE_NUDGE).map_err(from_slice)
}

fn outline_changes(a: &Region, b: &Region) -> Result<bool, GenError> {
    let d = a.difference(b)?.area().abs() + b.difference(a)?.area().abs();
    Ok(d > SAME_OUTLINE_AREA)
}

/// The next region to cut after a band, and whether it is an intermediate
/// band. When the band emitted passes down to `ceiling` and the outline just
/// under `ceiling` differs from the outline at the next flat level `z_next`,
/// a step lies between them with no flat level in it: the outline just under
/// `ceiling` is returned so that its floor pass is cut first (`true`).
/// Otherwise the outline at `z_next` (`false`).
pub(super) fn next_region(
    part: &[Triangle],
    bodies: &[Range<usize>],
    p: &ContourParams,
    ceiling: f64,
    z_next: f64,
    emitted: bool,
) -> Result<(Region, bool), GenError> {
    let lower = whole_region_at(part, bodies, z_next - p.stock_to_leave, z_next)?;
    if !emitted || ceiling <= z_next + SAME_LEVEL {
        return Ok((lower, false));
    }
    let upper = whole_region_at(part, bodies, ceiling - p.stock_to_leave - SLICE_NUDGE, ceiling)?;
    if upper.is_empty() || !outline_changes(&upper, &lower)? {
        return Ok((lower, false));
    }
    Ok((upper, true))
}
