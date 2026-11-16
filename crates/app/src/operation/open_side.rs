//! #1749: the footprint a whole-model 3D Pocket (and its stamped twin,
//! Multi-Axis Rough) clears - the silhouette grown past its open edges
//! (`machining3d::open_sides`), out to the stock's outline (#1774), the
//! setup's other parts kept out.
//!
//! Provenance: the "tool centre on boundary" containment public CAM
//! documentation describes, and the 2D pocket's stock-outline reading
//! (#12); our own design, no vendor material consulted.

use super::*;

/// #1774: neither kind carries a 3+2 index today (`OpParams::tilt` reads
/// `None` for both), so a frame never reaches these arms. Should one
/// ever arrive, the open-side law has not been read in that frame: the
/// op refuses by name rather than keep the footprint that leaves a ridge
/// along every open edge while reading ok.
pub(super) const TILTED_OPEN_SIDE: &str = "a whole-model 3D Pocket on a tilted (3+2) plane cannot yet clear past the model's open edges, \
     and would leave the stock standing along them - pick the area to clear, or cut it untilted";

/// `region` as the ladder reads it: grown past the silhouette, out to
/// `stock` (the setup's stock outline), when it is the model's own
/// source-built silhouette (`source_built`, a whole-model source);
/// otherwise the picked or controlled boundary, exactly as given.
/// `neighbours` are the setup's other parts (`keep_law::neighbour_parts`):
/// the growth never reaches into them. A tilted whole model refuses.
pub(super) fn open_footprint(
    region: Region,
    source_built: bool,
    tilted: bool,
    src: &SrcRef,
    neighbours: &[Triangle],
    stock: Option<&Region>,
    r: f64,
) -> Result<Region, GenError> {
    if !source_built || !matches!(src, SrcRef::Entity(_)) {
        return Ok(region);
    }
    if tilted {
        return Err(GenError::Invalid(TILTED_OPEN_SIDE.into()));
    }
    let keep_out = if neighbours.is_empty() {
        Region::empty()
    } else {
        slice::project(neighbours, FOOTPRINT_SAMPLES).map_err(from_slice)?
    };
    machining3d::open_sides(&region, r, &keep_out, stock)
}

#[cfg(test)]
mod law {
    use super::*;

    fn square(h: f64) -> Region {
        let p = |x: f64, y: f64| Point2::new(x, y);
        Region::new(vec![Loop::new(vec![p(-h, -h), p(h, -h), p(h, h), p(-h, h)]).expect("loop")]).expect("region")
    }

    #[test]
    fn a_tilted_whole_model_refuses_by_name() {
        let src = SrcRef::Entity("part".into());
        let err = open_footprint(square(10.0), true, true, &src, &[], None, 3.0).expect_err("a tilted whole model refuses");
        assert!(err.to_string().contains("tilted (3+2) plane"), "{err}");
        let picked = open_footprint(square(10.0), false, true, &src, &[], None, 3.0).expect("a picked region stays as given");
        assert!((picked.area().abs() - 400.0).abs() < 1e-6);
    }
}
