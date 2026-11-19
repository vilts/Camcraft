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
/// Issue #1950: `air` (the Adaptive plain ladder) reaches `stock_band(r,
/// true)` past the stock, so the stock ring is entered from the air.
#[allow(clippy::too_many_arguments)]
pub(super) fn open_footprint(
    region: Region,
    source_built: bool,
    tilted: bool,
    src: &SrcRef,
    neighbours: &[Triangle],
    stock: Option<&Region>,
    r: f64,
    air: bool,
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
    machining3d::open_sides(&region, r, &keep_out, stock, machining3d::stock_band(r, air))
}

/// Issue #1950: the Adaptive air band (the footprint outside the stock
/// outline, up to `stock_band(r, true)` past it) is cut, so steel that
/// stands in it is struck. The generator does not route round it: a
/// fixture box that meets the band in XY and stands above the op's
/// bottom refuses the op by name, before the post. The machine table is
/// not judged here (it stands under the whole stock; the post gate's
/// fixture law judges it on every move). The post gate stays the second
/// line for steel moved after generation.
pub(super) fn air_band_refusal(
    footprint: &Region,
    stock: &Region,
    fixtures: &[(&str, &crate::fixture::FixtureBox)],
    bottom_z: f64,
) -> Result<(), GenError> {
    if fixtures.is_empty() {
        return Ok(());
    }
    let band = footprint.difference(stock)?;
    if band.is_empty() {
        return Ok(());
    }
    for (name, b) in fixtures {
        if *name == crate::fixture::MACHINE_TABLE || b.min.z.max(b.max.z) <= bottom_z {
            continue;
        }
        let t = AIR_BAND_TOL;
        let (x0, x1) = (b.min.x.min(b.max.x) - t, b.max.x.max(b.min.x) + t);
        let (y0, y1) = (b.min.y.min(b.max.y) - t, b.max.y.max(b.min.y) + t);
        if ![x0, x1, y0, y1].iter().all(|v| v.is_finite()) {
            return Err(GenError::Invalid(format!("the fixture '{name}' has a box that cannot be read, so the Adaptive air band cannot be judged against it")));
        }
        let rect = Region::new(vec![Loop::new(vec![
            Point2::new(x0, y0),
            Point2::new(x1, y0),
            Point2::new(x1, y1),
            Point2::new(x0, y1),
        ])?])?;
        if !band.intersect(&rect)?.is_empty() {
            return Err(GenError::Invalid(format!("{AIR_BAND_FIXTURE} '{name}' - move the fixture out of that band, or select the Offset rings strategy")));
        }
    }
    Ok(())
}

/// The named refusal of [`air_band_refusal`].
pub const AIR_BAND_FIXTURE: &str = "the Adaptive strategy cuts the stock ring from the air up to 2 tool radii past the stock side, and that band meets the fixture";

/// The XY growth of a fixture box for [`air_band_refusal`] (mm): steel
/// contact is contact.
const AIR_BAND_TOL: f64 = 0.01;

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
        let err = open_footprint(square(10.0), true, true, &src, &[], None, 3.0, false).expect_err("a tilted whole model refuses");
        assert!(err.to_string().contains("tilted (3+2) plane"), "{err}");
        let picked = open_footprint(square(10.0), false, true, &src, &[], None, 3.0, false).expect("a picked region stays as given");
        assert!((picked.area().abs() - 400.0).abs() < 1e-6);
    }
}
