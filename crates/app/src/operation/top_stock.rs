//! #104: a named height warning over the operation's region, with credit for prior cuts.
//! Provenance: original shop-rule diagnostic; polygon intersection and a plane's maximum
//! at a clipped polygon vertex are standard computational geometry. No vendor material.

use super::drill_prior::PriorCell;
use super::{GenError, Loop, Point2, Region, Triangle, Z_TOL};
use crate::cl::Toolpath;
use crate::stock_sim::Columns;
#[cfg(test)]
use crate::stock_sim::Dexel;

/// Stored in existing path notes, not a new persisted verifier variant. This is
/// advisory: the move-level collision laws still decide whether posting is safe.
pub const TOP_STOCK_WARNING: &str = "Top below stock:";

pub(super) fn note(
    path: &mut Toolpath,
    top: Option<f64>,
    region: &Region,
    stock: &[Triangle],
    prior: Option<&mut PriorCell>,
) {
    let Some(top) = top.filter(|z| z.is_finite()) else {
        return;
    };
    let raw = match raw_top(region, stock) {
        Ok(Some(z)) if z > top + Z_TOL => z,
        Ok(_) => return,
        Err(e) => {
            tracing::warn!("Top/stock warning could not read the region: {e}");
            return;
        }
    };
    // Simulate only when the raw comparison would warn; share the same lazy
    // prior used by reach/rapid laws. Unknown removal never counts as air.
    let remaining = prior
        .and_then(PriorCell::get)
        .and_then(|dex| prior_top(region, &Columns::of_grid(dex)))
        .map_or(raw, |z| raw.min(z));
    if remaining > top + Z_TOL {
        path.notes.push(format!(
            "{TOP_STOCK_WARNING} the operation's top Z{top:.3} is below the stock top Z{remaining:.3} over its region - \
             approach rapids may enter uncut stock; raise Top or face first"
        ));
    }
}

/// The declared stock's highest surface over this region, not the global box
/// top: a tall feature elsewhere must not warn for a low local patch. Vertical
/// facets have no projected area; the caps meeting them supply the same height.
fn raw_top(region: &Region, stock: &[Triangle]) -> Result<Option<f64>, GenError> {
    let mut top: Option<f64> = None;
    for t in stock {
        let (u, v) = (t.b - t.a, t.c - t.a);
        let det = u.x * v.y - u.y * v.x;
        if det.abs() <= 1e-12 {
            continue;
        }
        let footprint = Region::new(vec![Loop::new(vec![
            Point2::new(t.a.x, t.a.y),
            Point2::new(t.b.x, t.b.y),
            Point2::new(t.c.x, t.c.y),
        ])?])?;
        let clipped = region.intersect(&footprint)?;
        for q in clipped.loops().iter().flat_map(|l| l.points()) {
            let (x, y) = (q.x - t.a.x, q.y - t.a.y);
            let z = t.a.z + (x * v.y - y * v.x) / det * u.z + (u.x * y - u.y * x) / det * v.z;
            if !z.is_finite() {
                return Err(GenError::Invalid("non-finite stock surface".into()));
            }
            top = Some(top.map_or(z, |a| a.max(z)));
        }
    }
    Ok(top)
}

/// Read every prior cell that can meet the region, not just its centre or the
/// approach point. Padding by a cell diagonal includes boundary cells (and may
/// conservatively keep a warning at a pocket wall). No sampled cell = unknown.
fn prior_top(region: &Region, dex: &Columns<'_>) -> Option<f64> {
    // #586 (M7.3): read through the column seam, a rest grid or a field
    // stock alike - the same lattice formulas as before.
    let (cell, o) = (dex.cell(), dex.origin());
    let padded = region.offset(cell * std::f64::consts::SQRT_2).ok()?;
    let (lo, hi) = padded.bounds()?;
    let ix0 = ((lo.x - o.x) / cell).floor().max(0.0) as usize;
    let iy0 = ((lo.y - o.y) / cell).floor().max(0.0) as usize;
    let ix1 = ((hi.x - o.x) / cell)
        .ceil()
        .clamp(0.0, dex.nx() as f64) as usize;
    let iy1 = ((hi.y - o.y) / cell)
        .ceil()
        .clamp(0.0, dex.ny() as f64) as usize;
    let mut top: Option<f64> = None;
    for iy in iy0..iy1 {
        for ix in ix0..ix1 {
            if padded.contains(dex.center(ix, iy)) {
                let z = dex.top_at(ix, iy);
                if z.is_nan() {
                    return None;
                }
                top = Some(top.map_or(z, |a| a.max(z)));
            }
        }
    }
    top.filter(|z| z.is_finite())
}

#[cfg(test)]
mod tests {
    /// The tests' grids read through the seam, as production does.
    fn prior_top_grid(region: &crate::geo2d::Region, dex: &super::Dexel) -> Option<f64> {
        super::prior_top(region, &super::Columns::of_grid(dex))
    }
    use super::*;
    use crate::geometry::{Point3, cube};

    fn patch(x0: f64, x1: f64) -> Region {
        Region::new(vec![
            Loop::new(vec![
                Point2::new(x0, 1.0),
                Point2::new(x1, 1.0),
                Point2::new(x1, 3.0),
                Point2::new(x0, 3.0),
            ])
            .expect("loop"),
        ])
        .expect("region")
    }

    #[test]
    fn stock_top_is_local_and_disjoint_stock_is_silent() {
        let mut stock = cube(Point3::ZERO, Point3::new(5.0, 5.0, 4.0))
            .facets
            .to_vec();
        stock.extend(
            cube(Point3::new(10.0, 0.0, 0.0), Point3::new(15.0, 5.0, 20.0))
                .facets
                .iter()
                .copied(),
        );
        assert_eq!(raw_top(&patch(1.0, 3.0), &stock).expect("top"), Some(4.0));
        assert_eq!(
            raw_top(&patch(11.0, 13.0), &stock).expect("top"),
            Some(20.0)
        );
        assert_eq!(raw_top(&patch(6.0, 8.0), &stock).expect("top"), None);
    }

    #[test]
    fn a_sloped_stock_cap_is_read_at_the_region_not_its_far_vertex() {
        let t = Triangle::new(
            Point3::ZERO,
            Point3::new(10.0, 0.0, 10.0),
            Point3::new(0.0, 10.0, 0.0),
        );
        let top = raw_top(&patch(1.0, 3.0), &[t])
            .expect("top")
            .expect("overlap");
        assert!((top - 3.0).abs() < 1e-6, "{top}");
    }

    #[test]
    fn prior_credit_is_regional_and_partial_removal_keeps_the_highest_cell() {
        let mut dex = Dexel::block(0.0, 0.0, 20.0, 10.0, 10.0, 0.25).expect("grid");
        // A local pocket, not a globally faced block. Only this patch is clear.
        for iy in 0..dex.ny {
            for ix in 0..dex.nx {
                if dex.center(ix, iy).x < 8.0 {
                    dex.lower(ix + iy * dex.nx, 5.0);
                }
            }
        }
        assert_eq!(prior_top_grid(&patch(2.0, 4.0), &dex), Some(5.0));
        assert_eq!(prior_top_grid(&patch(6.0, 10.0), &dex), Some(10.0));
        assert_eq!(prior_top_grid(&patch(30.0, 32.0), &dex), None);
    }

    #[test]
    fn equal_or_higher_top_and_unknown_prior_are_handled_without_new_post_rules() {
        let stock = cube(Point3::ZERO, Point3::new(5.0, 5.0, 10.0)).facets;
        for top in [10.0, 12.0] {
            let mut path = Toolpath::new(Vec::new());
            note(
                &mut path,
                Some(top),
                &patch(1.0, 3.0),
                &stock,
                Some(&mut PriorCell::new(None)),
            );
            assert!(path.notes.is_empty());
        }
        let mut path = Toolpath::new(Vec::new());
        note(
            &mut path,
            Some(5.0),
            &patch(1.0, 3.0),
            &stock,
            Some(&mut PriorCell::new(None)),
        );
        assert_eq!(path.notes.len(), 1);
        assert!(path.notes[0].contains("Z10.000"));
    }
    #[test]
    fn a_prior_with_no_finite_surface_is_unknown_not_clearance() {
        let mut dex = Dexel::block(0.0, 0.0, 5.0, 5.0, 10.0, 0.25).expect("grid");
        for k in 0..dex.nx * dex.ny {
            dex.lower(k, f64::NEG_INFINITY);
        }
        assert_eq!(prior_top_grid(&patch(1.0, 3.0), &dex), None);
    }
}
