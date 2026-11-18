//! The in-process stock read from a region that only BOUNDS the
//! material (issue 2031): the remaining stock earlier operations left
//! is known as a superset, so a cell stands as material wherever that
//! superset may reach it. Provenance: conservative rasterization - a
//! cell is covered when the shape, grown by the cell's half diagonal,
//! holds its center (the Minkowski-sum reading of overestimated
//! conservative rasterization, Hasselgren, Akenine-Moller and Ohlsson,
//! "Conservative Rasterization", GPU Gems 2, 2005). The growth and the
//! queries are ours.

use crate::geo2d::{Point2, Region};

use super::Raster;

/// Slack added to the half diagonal (mm): two polygon-offset arc
/// tolerances, so the offset's own flats cannot lose a cell.
const GROW_SLACK: f64 = 0.01;

impl Raster {
    /// How far a bound on the material is grown before it is sampled at
    /// cell centers, so every cell it touches stands as material.
    pub(in crate::machining2d::adaptive) fn conservative_growth(&self) -> f64 {
        self.h * std::f64::consts::FRAC_1_SQRT_2 + GROW_SLACK
    }

    /// Keep material only in the cells whose centers lie inside
    /// `material` (already grown by [`Self::conservative_growth`]); the
    /// walls stay what the grid was built with.
    pub(in crate::machining2d::adaptive) fn keep(&mut self, material: &Region) {
        let mut inside = vec![false; self.cells.len()];
        let mut xs: Vec<f64> = Vec::new();
        for iy in 0..self.ny {
            let y = self.y0 + (iy as f64 + 0.5) * self.h;
            xs.clear();
            for l in material.loops() {
                let pts = l.points();
                let n = pts.len();
                for i in 0..n {
                    let (a, b) = (pts[i], pts[(i + 1) % n]);
                    let (ylo, yhi) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
                    if y >= ylo && y < yhi {
                        xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                    }
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.as_chunks::<2>().0 {
                let i0 = ((pair[0] - self.x0) / self.h - 0.5).ceil().max(0.0) as usize;
                let i1 = ((pair[1] - self.x0) / self.h - 0.5).floor();
                if i1 < 0.0 {
                    continue;
                }
                let i1 = (i1 as usize).min(self.nx - 1);
                for ix in i0..=i1 {
                    inside[ix + iy * self.nx] = true;
                }
            }
        }
        for (c, keep) in self.cells.iter_mut().zip(inside) {
            *c &= keep;
        }
    }

    /// [`Raster::cut`], counting the material cells it clears (issue
    /// 2109: what a feed removes, walls aside, tells cut from air).
    pub(in crate::machining2d::adaptive) fn cut_count(&mut self, a: Point2, b: Point2, r: f64) -> usize {
        if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite() && r.is_finite()) {
            return 0;
        }
        let h = self.h;
        let iy0 = ((a.y.min(b.y) - r - self.y0) / h - 0.5).ceil().max(0.0) as usize;
        let iy1 = ((a.y.max(b.y) + r - self.y0) / h - 0.5).floor();
        if iy1 < 0.0 {
            return 0;
        }
        let iy1 = (iy1 as usize).min(self.ny.saturating_sub(1));
        let mut n = 0;
        for iy in iy0..=iy1 {
            let y = self.y0 + (iy as f64 + 0.5) * h;
            let Some((xl, xr)) = super::capsule_row(a, b, r, y) else { continue };
            let i0 = ((xl - self.x0) / h - 0.5).ceil().max(0.0) as usize;
            let i1 = ((xr - self.x0) / h - 0.5).floor();
            if i1 < 0.0 {
                continue;
            }
            let (row, i1) = (iy * self.nx, (i1 as usize).min(self.nx.saturating_sub(1)));
            for ix in i0..=i1 {
                if let Some(c) = self.cells.get_mut(row + ix) {
                    n += usize::from(*c);
                    *c = false;
                }
            }
        }
        n
    }

    /// Does the capsule of radius `rad` about `a -> b` hold no material
    /// cell (walls aside)? Off the grid nothing is proved: `false`.
    pub(in crate::machining2d::adaptive) fn air(&self, a: Point2, b: Point2, rad: f64) -> bool {
        self.capsule_free(a, b, rad, false)
    }

    /// Does material stand between `inner` and `outer` of `p`, read on
    /// four rings all round? (What a cutter standing at `p` would cut,
    /// where its center is known to stand `inner` or more off the
    /// material's outline.)
    pub(in crate::machining2d::adaptive) fn touches(&self, p: Point2, inner: f64, outer: f64) -> bool {
        const RING: usize = 180;
        let step = std::f64::consts::TAU / RING as f64;
        (0..4).any(|k| {
            let rad = outer - (outer - inner) * f64::from(k) / 3.0;
            (0..RING).any(|i| {
                let a = i as f64 * step;
                self.mat(Point2::new(p.x + rad * a.cos(), p.y + rad * a.sin()))
            })
        })
    }
}
