//! The 2-D material grid of the #3567 entry pins (`pass_start_engagement`):
//! the stock as cells, the tool disc that clears them, and the sweep of a
//! move. Provenance: a plain raster
//! material model of the public milling handbooks; the rim law is the
//! shape-mask helper's (`common/mask_grid.rs`); no vendor material consulted.

use camcraft_motion::cl::Move;
use camcraft_motion::geo2d::{Point2, Region};

/// The tool radius of the band case (mm).
pub const R: f64 = 4.0;
/// The grid cell (mm).
pub const CELL: f64 = 0.25;

/// The material grid: true where stock still stands.
pub struct Grid {
    lo: Point2,
    nx: usize,
    ny: usize,
    cells: Vec<bool>,
}

impl Grid {
    pub fn new(stock: &Region) -> Grid {
        let Some((lo, hi)) = stock.bounds() else { panic!("stock bounds") };
        let nx = ((hi.x - lo.x) / CELL).ceil() as usize + 1;
        let ny = ((hi.y - lo.y) / CELL).ceil() as usize + 1;
        let mut cells = vec![false; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let p = Point2::new(lo.x + (i as f64 + 0.5) * CELL, lo.y + (j as f64 + 0.5) * CELL);
                cells[j * nx + i] = stock.contains(p);
            }
        }
        Grid { lo, nx, ny, cells }
    }

    fn cell(&self, i: usize, j: usize) -> Point2 {
        Point2::new(self.lo.x + (i as f64 + 0.5) * CELL, self.lo.y + (j as f64 + 0.5) * CELL)
    }

    /// The cell index window of the disc of radius `r` at `c`.
    fn window(&self, c: Point2, r: f64) -> (usize, usize, usize, usize) {
        let i0 = (((c.x - r - self.lo.x) / CELL).floor().max(0.0)) as usize;
        let j0 = (((c.y - r - self.lo.y) / CELL).floor().max(0.0)) as usize;
        let i1 = ((((c.x + r - self.lo.x) / CELL).ceil()).max(0.0) as usize).min(self.nx);
        let j1 = ((((c.y + r - self.lo.y) / CELL).ceil()).max(0.0) as usize).min(self.ny);
        (i0, j0, i1, j1)
    }

    /// Clear the disc at `c`; returns the area removed (mm^2).
    pub fn cut(&mut self, c: Point2) -> f64 {
        let (i0, j0, i1, j1) = self.window(c, R);
        let mut n = 0usize;
        for j in j0..j1 {
            for i in i0..i1 {
                let p = self.cell(i, j);
                if (p.x - c.x).hypot(p.y - c.y) <= R && self.cells[j * self.nx + i] {
                    self.cells[j * self.nx + i] = false;
                    n += 1;
                }
            }
        }
        n as f64 * CELL * CELL
    }
}

/// The centre positions a move sweeps, no farther apart than half a
/// cell, each with the direction of its chord.
pub fn sweep_dirs(m: &Move) -> Vec<(Point2, (f64, f64))> {
    let mut out = Vec::new();
    for (a, b) in m.chords(0.005) {
        let n = ((b.x - a.x).hypot(b.y - a.y) / (CELL * 0.5)).ceil().max(1.0) as usize;
        for s in 0..=n {
            let t = s as f64 / n as f64;
            out.push((Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t), (b.x - a.x, b.y - a.y)));
        }
    }
    out
}

/// The centre positions a move sweeps (see [`sweep_dirs`]).
pub fn sweep(m: &Move) -> Vec<Point2> {
    sweep_dirs(m).into_iter().map(|(q, _)| q).collect()
}

pub fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}
