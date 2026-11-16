//! Application error conversion for the geometry crate's blind-slot calculation.

use super::*;

pub(super) fn region(points: &[Point2], stock: &Region, extend: f64) -> Result<Region, GenError> {
    boundary_chains::open_slot::region(points, stock, extend).map_err(boundary_chain_error)
}
