//! The ring and row plan of one planar area (split from `pocket.rs` for
//! the 400-line law, #3316): the shape law both the 2-D pocket and the
//! 3-D pocket's per-level plan share. Provenance: concentric offset rings
//! and zigzag rows of published pocketing practice; no vendor material
//! consulted.

use crate::geo2d::{Point2, Region};
use crate::entry::dive_room;

use super::face::GenError;
use super::pocket_strategy::PocketStrategy;
use super::pocket_zigzag::zigzag_passes;
use super::waterscan::waterline_passes;

/// The cutter-center passes for one planar area: the shape law both the
/// 2-D pocket and the 3-D pocket's per-level plan share (the strategies
/// band/erode `center` identically; only the numbers come from the
/// operation's params either way). The start side of #3316 is
/// `level_rings::ring_plan`'s, over this plan.
pub(crate) fn plan_passes(
    center: &Region,
    stepover: f64,
    climb: bool,
    strategy: PocketStrategy,
) -> Result<Vec<Vec<Point2>>, GenError> {
    match strategy {
        PocketStrategy::Zigzag | PocketStrategy::Auto => zigzag_passes(center, stepover, climb),
        // Offset rings are ring tiling at the caller's step; the HSM
        // plan is `pocket_hsm.rs`'s (the planar pocket's own, and the
        // 3-D ladder's per level through `machining3d::hsm`), which
        // hands a refused area back here as rings.
        PocketStrategy::Waterline | PocketStrategy::Adaptive | PocketStrategy::OffsetRings => {
            waterline_passes(center, stepover, climb)
        }
    }
}

/// Erode `region` by `erode` and drop sub-tolerance Clipper arc-chord
/// slivers (`<= 0.01 mm^2` area and `<= 0.008 mm` half-width) that hold
/// no cuttable pass and would spin on empty bores (#2232).
pub(super) fn non_sliver_center(region: &Region, erode: f64) -> Result<Region, GenError> {
    let center = region.offset(-erode)?;
    if center.is_empty() {
        return Ok(center);
    }
    let outers = center.loops().iter().filter(|l| l.is_ccw()).count();
    if outers <= 1 && center.area().abs() > 1e-2 {
        return Ok(center);
    }
    let clean = Region::from_slices(center.loops().to_vec())?;
    let mut kept = Vec::new();
    let mut dropped = false;
    for c in super::pocket_hsm::components(&clean)? {
        if c.area().abs() > 1e-2 || dive_room(&c, 0.008).is_some() {
            kept.extend(c.loops().iter().cloned());
        } else {
            dropped = true;
        }
    }
    // Nothing dropped: keep the eroded area exactly as offset gave it. The
    // rebuild through `from_slices` re-starts each loop at another vertex,
    // which moved a multi-piece pocket's pass starts (#2426).
    if !dropped {
        return Ok(center);
    }
    Ok(Region::from_slices(kept)?)
}

/// True when a tool of `tool_diameter`, leaving `stock_to_leave` on each
/// wall, has a centre position in `region`: the pocket generators' own
/// "pocket is narrower than the tool" predicate, for a door that warns
/// before Generate (#3631). An empty region has no material to fit, so
/// it fits.
pub fn pocket_tool_fits(region: &Region, tool_diameter: f64, stock_to_leave: f64) -> Result<bool, GenError> {
    if region.is_empty() {
        return Ok(true);
    }
    Ok(!non_sliver_center(region, tool_diameter * 0.5 + stock_to_leave)?.is_empty())
}

#[cfg(test)]
mod non_sliver_tests {
    use super::non_sliver_center;
    use crate::geo2d::{Loop, Point2, Region};

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
        let pts = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        let Ok(l) = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()) else {
            panic!("fixture loop")
        };
        l
    }

    /// #2426: a two-piece area with no sliver comes back exactly as the
    /// offset gave it - same loops, same start vertices - so the pass
    /// starts of a multi-piece pocket do not move.
    #[test]
    fn a_multi_piece_center_without_slivers_is_the_plain_offset() {
        let Ok(reg) = Region::new(vec![rect(0.0, 0.0, 30.0, 30.0), rect(50.0, 0.0, 61.0, 11.0)]) else {
            panic!("fixture region")
        };
        let Ok(plain) = reg.offset(-5.0) else { panic!("offset") };
        let Ok(kept) = non_sliver_center(&reg, 5.0) else { panic!("non_sliver_center") };
        assert_eq!(plain.loops().iter().filter(|l| l.is_ccw()).count(), 2, "fixture must have two pieces");
        assert_eq!(kept, plain);
    }
}
