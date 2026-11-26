//! Split from `stock_sim.rs` by M5.0.1; structure only.

use super::dexel::{Dexel, fy0};

use crate::geo2d::{GeoError, Point2, Region};
use crate::geometry::Triangle;
/// What the sim refuses, in the generator's vocabulary.
#[derive(thiserror::Error, Debug, PartialEq)]
pub enum StockError {
    #[error("invalid stock simulation setup: {0}")]
    Invalid(String),
    #[error(transparent)]
    Geo(#[from] GeoError),
}

/// Hard ceiling on grid cells (both axes): a model of a stadium at 1 um
/// cells would otherwise allocate until the machine stops. Refusing
/// with the numbers named is the clean answer; generators pick cells
/// from tool sizes and never approach it.
pub(super) const MAX_CELLS_PER_AXIS: usize = 4096;

/// The final part's top surface per cell: the highest facet plane
/// covering each cell center, `-inf` where no facet covers it (the
/// column is air). Facets whose XY projection is degenerate (vertical
/// walls) carry no surface height at any point - their caps do - and
/// are skipped by the signed-area test, exact by construction.
pub fn part_surface(
    facets: &[Triangle],
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
) -> Vec<f64> {
    let mut out = vec![f64::NEG_INFINITY; nx * ny];
    for t in facets {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            continue;
        }
        let det = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if det.abs() <= f64::EPSILON {
            continue; // vertical (or degenerate) projection: no top
        }
        let minx = a.x.min(b.x).min(c.x);
        let maxx = a.x.max(b.x).max(c.x);
        let miny = a.y.min(b.y).min(c.y);
        let maxy = a.y.max(b.y).max(c.y);
        let fx = |v: f64| ((v - x0) / cell).floor();
        let i0 = fx(minx).max(0.0) as usize;
        let i1 = fx(maxx).clamp(0.0, nx as f64 - 1.0) as usize;
        let j0 = fy0(miny, y0, cell).max(0.0) as usize;
        let j1 = fy0(maxy, y0, cell).clamp(0.0, ny as f64 - 1.0) as usize;
        if i0 >= nx || j0 >= ny {
            continue;
        }
        for iy in j0..=j1 {
            for ix in i0..=i1 {
                let q = Point2::new(x0 + (ix as f64 + 0.5) * cell, y0 + (iy as f64 + 0.5) * cell);
                // Barycentric against the signed det (works for either
                // winding; the det sign cancels in both ratios).
                let u = ((q.x - a.x) * (c.y - a.y) - (q.y - a.y) * (c.x - a.x)) / det;
                let v = ((b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x)) / det;
                if u < -1e-9 || v < -1e-9 || u + v > 1.0 + 1e-9 {
                    continue;
                }
                let z = a.z + u * (b.z - a.z) + v * (c.z - a.z);
                let k = ix + iy * nx;
                if z > out[k] {
                    out[k] = z;
                }
            }
        }
    }
    out
}

/// The sim's verdict, all measured against the model's own surface:
/// rest (material still standing above where the part says it must
/// not), gouge (material removed from below it), and the gross number
/// the tool actually took out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StockReport {
    pub removed_volume: f64,
    pub rest_volume: f64,
    pub rest_cells: usize,
    pub max_rest_height: f64,
    pub gouge_volume: f64,
    pub gouge_cells: usize,
    pub max_gouge_depth: f64,
}

/// One pass of the measuring law over a grid, its part surface, and
/// the region the operation cares about (columns outside the footprint
/// are the rest of the machine's world, not this operation's verdict).
#[must_use]
pub fn measure(dex: &Dexel, part: &[f64], footprint: &Region, tol: f64) -> StockReport {
    let a2 = dex.cell * dex.cell;
    let mut rep = StockReport::default();
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            let k = ix + iy * dex.nx;
            if !footprint.contains(dex.center(ix, iy)) {
                continue;
            }
            let sim = dex.top_at(k);
            let ptop = part[k];
            if ptop.is_finite() {
                if sim > ptop + tol {
                    rep.rest_volume += (sim - ptop) * a2;
                    rep.rest_cells += 1;
                    rep.max_rest_height = rep.max_rest_height.max(sim - ptop);
                }
                if sim < ptop - tol {
                    rep.gouge_volume += (ptop - sim) * a2;
                    rep.gouge_cells += 1;
                    rep.max_gouge_depth = rep.max_gouge_depth.max(ptop - sim);
                }
            }
            // Gross removal uses the initial surface. A laid cylinder
            // also limits the depth to its rounded underside (#2758).
            rep.removed_volume += dex.removed_depth(k) * a2;
        }
    }
    rep
}
