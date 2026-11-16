//! 2D mask grid rasterizer and progressive engagement meter for shape-mask tests (#1768).

use camcraft_motion::geo2d::{Point2, Region};

pub(super) const MAX_CELLS: usize = 4_000_000;
pub(super) const CHORD_TOL: f64 = 0.05;
pub(super) const Z_EPS: f64 = 1e-3;
const RIM_SAMPLES: usize = 72;

#[derive(Clone)]
pub(super) struct Grid {
    pub x0: f64,
    pub y0: f64,
    pub cell: f64,
    pub w: usize,
    pub h: usize,
}

#[derive(Clone, Copy)]
enum Rim {
    Open,
    Stock,
}

impl Grid {
    pub fn for_region(region: &Region, cell: f64, pad: f64) -> Result<Self, String> {
        if !cell.is_finite() || cell <= 0.0 {
            return Err(format!("grid cell must be finite and positive, got {cell}"));
        }
        if !pad.is_finite() || pad < 0.0 {
            return Err(format!("grid pad must be finite and non-negative, got {pad}"));
        }
        let Some((lo, hi)) = region.bounds() else {
            return Err("expected region is empty".into());
        };
        let x0 = (lo.x - pad).floor();
        let y0 = (lo.y - pad).floor();
        let x1 = (hi.x + pad).ceil();
        let y1 = (hi.y + pad).ceil();
        let w = ((x1 - x0) / cell).ceil().max(1.0) as usize;
        let h = ((y1 - y0) / cell).ceil().max(1.0) as usize;
        let total = w.checked_mul(h).ok_or_else(|| "grid size overflow".to_string())?;
        if total > MAX_CELLS {
            return Err(format!("grid {w}x{h} ({total} cells) exceeds {MAX_CELLS}"));
        }
        Ok(Self { x0, y0, cell, w, h })
    }

    pub fn len(&self) -> usize {
        self.w * self.h
    }

    pub fn idx(&self, ix: usize, iy: usize) -> usize {
        iy * self.w + ix
    }

    pub fn center(&self, ix: usize, iy: usize) -> Point2 {
        Point2::new(
            self.x0 + (ix as f64 + 0.5) * self.cell,
            self.y0 + (iy as f64 + 0.5) * self.cell,
        )
    }

    pub fn cell_of(&self, p: Point2) -> Option<(usize, usize)> {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        let fx = (p.x - self.x0) / self.cell;
        let fy = (p.y - self.y0) / self.cell;
        if fx < 0.0 || fy < 0.0 {
            return None;
        }
        let ix = fx.floor() as usize;
        let iy = fy.floor() as usize;
        (ix < self.w && iy < self.h).then_some((ix, iy))
    }

    pub fn segment_in_bounds(&self, a: Point2, b: Point2, r: f64) -> bool {
        if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
            return false;
        }
        let x1 = self.x0 + self.w as f64 * self.cell;
        let y1 = self.y0 + self.h as f64 * self.cell;
        a.x.min(b.x) - r >= self.x0
            && a.x.max(b.x) + r <= x1
            && a.y.min(b.y) - r >= self.y0
            && a.y.max(b.y) + r <= y1
    }

    pub fn rasterize_region(&self, region: &Region) -> Vec<bool> {
        let mut mask = vec![false; self.len()];
        let mut xs = Vec::new();
        for iy in 0..self.h {
            let py = self.y0 + (iy as f64 + 0.5) * self.cell;
            xs.clear();
            for lp in region.loops() {
                let pts = lp.points();
                let n = pts.len();
                for i in 0..n {
                    let a = pts[i];
                    let b = pts[(i + 1) % n];
                    if (a.y <= py && b.y > py) || (b.y <= py && a.y > py) {
                        let t = (py - a.y) / (b.y - a.y);
                        xs.push(a.x + t * (b.x - a.x));
                    }
                }
            }
            xs.sort_by(f64::total_cmp);
            for &[x0, x1] in xs.as_chunks::<2>().0 {
                let c0 = ((x0 - self.x0) / self.cell - 0.5).ceil();
                let c1 = ((x1 - self.x0) / self.cell - 0.5).floor();
                if c1 < 0.0 || c0 >= self.w as f64 {
                    continue;
                }
                let ix0 = (c0.max(0.0) as usize).min(self.w - 1);
                let ix1 = (c1.min((self.w - 1) as f64) as usize).max(ix0);
                if c0 <= c1 {
                    let row = iy * self.w;
                    mask[row + ix0..=row + ix1].fill(true);
                }
            }
        }
        mask
    }

    pub fn dilate1(&self, src: &[bool]) -> Vec<bool> {
        let mut out = vec![false; self.len()];
        for iy in 0..self.h {
            let y0 = iy.saturating_sub(1);
            let y1 = (iy + 1).min(self.h - 1);
            for ix in 0..self.w {
                let x0 = ix.saturating_sub(1);
                let x1 = (ix + 1).min(self.w - 1);
                let mut hit = false;
                'nb: for ny in y0..=y1 {
                    for nx in x0..=x1 {
                        if src[self.idx(nx, ny)] {
                            hit = true;
                            break 'nb;
                        }
                    }
                }
                out[self.idx(ix, iy)] = hit;
            }
        }
        out
    }

    pub fn erode1(&self, src: &[bool]) -> Vec<bool> {
        let mut out = vec![false; self.len()];
        for iy in 1..self.h.saturating_sub(1) {
            for ix in 1..self.w.saturating_sub(1) {
                let mut all = true;
                'nb: for ny in (iy - 1)..=(iy + 1) {
                    for nx in (ix - 1)..=(ix + 1) {
                        if !src[self.idx(nx, ny)] {
                            all = false;
                            break 'nb;
                        }
                    }
                }
                out[self.idx(ix, iy)] = all;
            }
        }
        out
    }

    pub fn sweep_capsule(
        &self,
        seg: (Point2, Point2),
        r: f64,
        z: (f64, f64),
        in_depth: bool,
        swept_mask: &mut [bool],
        swept_z: &mut [f64],
    ) {
        let (a, b) = seg;
        if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
            return;
        }
        let max_x = self.x0 + self.w as f64 * self.cell;
        let max_y = self.y0 + self.h as f64 * self.cell;
        let bb_x0 = a.x.min(b.x) - r;
        let bb_x1 = a.x.max(b.x) + r;
        let bb_y0 = a.y.min(b.y) - r;
        let bb_y1 = a.y.max(b.y) + r;
        if bb_x1 < self.x0 || bb_x0 > max_x || bb_y1 < self.y0 || bb_y0 > max_y {
            return;
        }
        let ix0 = (((bb_x0 - self.x0) / self.cell - 0.5).floor().max(0.0) as usize).min(self.w - 1);
        let ix1 = (((bb_x1 - self.x0) / self.cell - 0.5).ceil().max(0.0) as usize).min(self.w - 1);
        let iy0 = (((bb_y0 - self.y0) / self.cell - 0.5).floor().max(0.0) as usize).min(self.h - 1);
        let iy1 = (((bb_y1 - self.y0) / self.cell - 0.5).ceil().max(0.0) as usize).min(self.h - 1);
        if ix0 > ix1 || iy0 > iy1 {
            return;
        }
        let vx = b.x - a.x;
        let vy = b.y - a.y;
        let vv = vx * vx + vy * vy;
        let r2 = r * r + 1e-12;
        for iy in iy0..=iy1 {
            let py = self.y0 + (iy as f64 + 0.5) * self.cell;
            for ix in ix0..=ix1 {
                let px = self.x0 + (ix as f64 + 0.5) * self.cell;
                let t = if vv <= 1e-18 {
                    0.0
                } else {
                    (((px - a.x) * vx + (py - a.y) * vy) / vv).clamp(0.0, 1.0)
                };
                let dx = px - (a.x + t * vx);
                let dy = py - (a.y + t * vy);
                if dx * dx + dy * dy <= r2 {
                    let i = self.idx(ix, iy);
                    // The disc covers this cell over an interval of segment parameters.
                    // Use its deepest actual position within that interval.
                    let depth = if vv <= 1e-18 || z.0 == z.1 {
                        z.0.min(z.1)
                    } else {
                        let projection = ((px - a.x) * vx + (py - a.y) * vy) / vv;
                        let perpendicular2 = ((px - a.x).powi(2) + (py - a.y).powi(2)
                            - projection * projection * vv)
                            .max(0.0);
                        let half = ((r2 - perpendicular2).max(0.0) / vv).sqrt();
                        let cut_t = if z.1 < z.0 {
                            (projection + half).min(1.0)
                        } else {
                            (projection - half).max(0.0)
                        };
                        z.0 + cut_t * (z.1 - z.0)
                    };
                    swept_z[i] = swept_z[i].min(depth);
                    if in_depth {
                        swept_mask[i] = true;
                    }
                }
            }
        }
    }

    pub fn contact_width(
        &self,
        p: Point2,
        dir: (f64, f64),
        z: f64,
        r: f64,
        expected: &[bool],
        swept_z: &[f64],
    ) -> f64 {
        let len = dir.0.hypot(dir.1);
        if len <= 1e-12 {
            return 0.0;
        }
        let rim = r + 0.75 * self.cell;
        let base = dir.1.atan2(dir.0) - std::f64::consts::FRAC_PI_2;
        let span = std::f64::consts::PI;
        let hits = (0..RIM_SAMPLES).map(|k| {
            let ang = base + (k as f64 + 0.5) * span / RIM_SAMPLES as f64;
            let sample = Point2::new(p.x + rim * ang.cos(), p.y + rim * ang.sin());
            let Some((ix, iy)) = self.cell_of(sample) else {
                return Rim::Open;
            };
            let idx = self.idx(ix, iy);
            if expected[idx] && swept_z[idx] > z + Z_EPS {
                Rim::Stock
            } else {
                Rim::Open
            }
        });
        let run = longest_arc(hits);
        let theta = (run as f64 * span / RIM_SAMPLES as f64).min(std::f64::consts::PI);
        r * (1.0 - theta.cos())
    }
}

fn longest_arc(hits: impl Iterator<Item = Rim>) -> usize {
    let mut best = 0usize;
    let mut cur = 0usize;
    for h in hits {
        match h {
            Rim::Open => cur = 0,
            Rim::Stock => {
                cur += 1;
                if cur > best {
                    best = cur;
                }
            }
        }
    }
    best
}
