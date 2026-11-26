//! The stock footprint the 2D Offset's pass start reads (#3316), in the
//! area's own frame: a start ring whose disc stands off it runs in open
//! air, one on it runs along a wall (named on the toolpath). Read from
//! the job's stock the way the corner-relief law reads it. Provenance:
//! our own; no vendor material consulted.

use crate::cl::TiltedFrame;
use crate::geo2d::Region;
use crate::geometry::{Triangle, slice};

use super::material::FOOTPRINT_SAMPLES;
use super::{Material, OpParams, SrcRef};

/// The judged stock's XY footprint in the operation's frame; `None` when
/// no stock is known (every start ring then reads as a wall).
pub(super) fn footprint(
    stock_region: Option<&Region>,
    frame: Option<&TiltedFrame>,
    stock_facets: Option<&[Triangle]>,
    src: &SrcRef,
    params: &OpParams,
    material: &Material,
) -> Option<Region> {
    match frame {
        None => stock_region.cloned(),
        Some(f) => super::material_source::judged_stock(stock_facets, src, params, material)
            .map(|t| f.local_facets(t))
            .and_then(|t| slice::project(&t, FOOTPRINT_SAMPLES).ok())
            .filter(|r| !r.is_empty()),
    }
}
