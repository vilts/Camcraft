//! The full-width meter of `offset_pass_start.rs` (#3316): walks a path
//! in order on a grid of the material and names each planar feed point
//! whose two flanks (0.9 tool radii to each side of the centre, square
//! to the motion) are both material that no earlier cut removed at its
//! depth - a cut with uncut stock on both sides over the full tool
//! width. The tool's own trail (the last 2.5 radii of travel) is not an
//! earlier cut. Provenance: our own test meter; no vendor material
//! consulted.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::geo2d::{Point2, Region};

const CELL: f64 = 0.25;

struct Grid {
    x0: f64,
    y0: f64,
    w: usize,
    h: usize,
    /// Per cell: the lowest depth cut there, and the travel at which it
    /// was first cut at that depth.
    cut: Vec<(f64, f64)>,
}

impl Grid {
    fn new(material: &Region, r: f64) -> Grid {
        let (lo, hi) = material.bounds().expect("material bounds");
        let pad = 2.0 * r + 1.0;
        let w = ((hi.x - lo.x + 2.0 * pad) / CELL).ceil() as usize + 1;
        let h = ((hi.y - lo.y + 2.0 * pad) / CELL).ceil() as usize + 1;
        Grid { x0: lo.x - pad, y0: lo.y - pad, w, h, cut: vec![(f64::INFINITY, 0.0); w * h] }
    }

    fn cell(&self, p: Point2) -> Option<usize> {
        let (fx, fy) = ((p.x - self.x0) / CELL, (p.y - self.y0) / CELL);
        (fx >= 0.0 && fy >= 0.0 && (fx as usize) < self.w && (fy as usize) < self.h).then(|| fy as usize * self.w + fx as usize)
    }

    fn sweep(&mut self, p: Point2, z: f64, r: f64, s: f64) {
        let n = (r / CELL).ceil() as i64 + 1;
        for dy in -n..=n {
            for dx in -n..=n {
                let q = Point2::new(p.x + dx as f64 * CELL, p.y + dy as f64 * CELL);
                if (q.x - p.x).hypot(q.y - p.y) <= r
                    && let Some(i) = self.cell(q)
                    && z < self.cut[i].0 - 1e-3
                {
                    self.cut[i] = (z, s);
                }
            }
        }
    }

    /// Is `q` material that no cut before travel `before` removed at `z`?
    fn uncut(&self, material: &Region, q: Point2, z: f64, before: f64) -> bool {
        material.contains(q) && self.cell(q).is_some_and(|i| self.cut[i].0 > z + 1e-3 || self.cut[i].1 >= before)
    }
}

/// Every planar feed point of `path` (in walking order) where the tool of
/// radius `r` has uncut `material` on both flanks.
pub fn full_width_points(path: &Toolpath, material: &Region, r: f64) -> Vec<Point2> {
    let mut g = Grid::new(material, r);
    let mut out = Vec::new();
    let mut travel = 0.0;
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        let planar = m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-9;
        for (a, b) in m.segments3(0.05) {
            let len = (b.x - a.x).hypot(b.y - a.y);
            let n = (len / (0.5 * CELL)).ceil().max(1.0) as usize;
            for k in 0..=n {
                let t = k as f64 / n as f64;
                let p = Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
                let z = a.z + t * (b.z - a.z);
                let s = travel + t * len;
                if planar && k > 0 && len > 1e-9 {
                    let (tx, ty) = ((b.x - a.x) / len, (b.y - a.y) / len);
                    let left = Point2::new(p.x - ty * 0.9 * r, p.y + tx * 0.9 * r);
                    let right = Point2::new(p.x + ty * 0.9 * r, p.y - tx * 0.9 * r);
                    let before = s - 2.5 * r;
                    if g.uncut(material, left, z, before) && g.uncut(material, right, z, before) {
                        out.push(p);
                    }
                }
                g.sweep(p, z, r, s);
            }
            travel += len;
        }
    }
    out
}
