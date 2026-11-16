//! The 3-D ladder's islands law: the part material standing above a
//! level plane (split from `machining3d.rs` by M5.0.1; the hanging
//! shadow added by the blind-floor fix). Provenance: plane sectioning
//! plus the XY shadow of a closed solid's down-facing boundary - the
//! standard projection identity of computational geometry (a column's
//! solid above a plane is either cut by the plane or starts at a
//! downward-facing face above it); no vendor material consulted.

use crate::geo2d::Region;
use crate::geometry::slice::{self, SliceError};
use crate::geometry::Triangle;
use crate::machining2d::GenError;
/// Half-open slices sit exactly on vertices for tessellated steps; this
/// is the same lattice the weld uses, so a grazing plane nudges rather
/// than lies.
pub(super) const EPS: f64 = 1e-6;

/// A hanging shadow that overlaps a level's target by less than this
/// (mm²) changes nothing a cutter can reach: the target is kept as it
/// was, byte for byte (the region ops would only re-normalize it).
const AREA_EPS: f64 = 1e-6;

pub(super) fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        // #1407: a model fault, named as one - never "machining parameters".
        e @ (SliceError::Open { .. } | SliceError::NonManifold { .. }) => GenError::Model(e.to_string()),
        other => GenError::Invalid(other.to_string()),
    }
}

/// The part material standing above `z`, projected to XY - the islands
/// law itself, [`slice::material_overhead`]: the model's cross-section
/// strictly above the plane (nudge-retried when a plane grazes a
/// tessellated shelf - reading an errored slice as no-island would be a
/// silent gouge, the retry lands above the shelf instead, failing safe),
/// united with the shadow of every down-facing face above it
/// ([`slice::shadow_above`]). The cross-section alone is only the whole
/// story for a part solid all the way down to the plane: a plane under
/// the part's underside (a blind floor's level run to the stock bottom)
/// or under an overhang sections nothing there, yet the material above
/// it shadows every column it covers. Public because it is the islands
/// law: the generators read it to shape each level (the 3-D ladder here,
/// the 2D contour's ring ladder and the app's pocket region on a part
/// through the same geometry function), and the tests pin the
/// monotonicity the ladder assumes (M2.1a).
pub fn material_above(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
    slice::material_overhead(facets, z).map_err(from_slice)
}

/// A level's target: the footprint minus the material standing above
/// `z` ([`material_above`]'s law). Read in two steps so a part solid
/// down to the plane - every level a ladder cut before the blind-floor
/// fix - keeps its target exactly: the hanging shadow is subtracted
/// only when it overlaps what the cross-section left.
pub(super) fn level_target(facets: &[Triangle], footprint: &Region, z: f64) -> Result<Region, GenError> {
    let islands = slice::material_above(facets, z).map_err(from_slice)?;
    let target = footprint.difference(&islands)?;
    if target.is_empty() {
        return Ok(target);
    }
    let hang = slice::shadow_above(facets, z).map_err(from_slice)?;
    if hang.is_empty() || target.intersect(&hang)?.area() <= AREA_EPS {
        return Ok(target);
    }
    Ok(target.difference(&hang)?)
}
