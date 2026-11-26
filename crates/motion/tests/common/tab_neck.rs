//! Tab neck meter for the 2D Contour shape-mask tests (#2561).
//! Provenance: plane geometry on the shape-mask grid. The material at the tabbed levels is the
//! complement of the swept cutter discs; the neck of a tab is the widest path through it from
//! the part to the far stock (twice the largest clearance, read from an exact Euclidean distance
//! transform: Felzenszwalb and Huttenlocher, "Distance Transforms of Sampled Functions", 2012).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Point2, Region};

use super::mask_grid::{CHORD_TOL, Grid};

/// One tab: a point in it, and the narrowest width of the material that joins the part to the
/// far stock through it (0 when the tab joins nothing).
#[derive(Clone, Copy, Debug)]
pub struct Neck {
    pub at: Point2,
    pub width: f64,
}

/// The tabs of an outside contour around `part`, measured on a grid of `cell` mm. A tab is a
/// patch that the passes at or above `tab_top` cut and the passes below it left. The far stock
/// is all material more than `2 * d` outside the bounds of `part`. Each tab is measured with
/// the other tabs taken out, so a wide tab does not hide a narrow one. The reading is good to
/// about one cell.
pub fn tab_necks(part: &Region, path: &Toolpath, d: f64, tab_top: f64, cell: f64) -> Result<Vec<Neck>, String> {
    let r = 0.5 * d;
    let grid = Grid::for_region(part, cell, 3.0 * d)?;
    let Some((lo, hi)) = part.bounds() else {
        return Err("the part region is empty".into());
    };
    let n = grid.len();
    let (mut above, mut below) = (vec![false; n], vec![false; n]);
    let mut depth = vec![f64::INFINITY; n];
    for mv in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        for (a, b) in mv.segments3(CHORD_TOL) {
            let seg = (Point2::new(a.x, a.y), Point2::new(b.x, b.y));
            let mask = if a.z.min(b.z) < tab_top - 1e-6 { &mut below } else { &mut above };
            grid.sweep_capsule(seg, r, (a.z, b.z), true, mask, &mut depth);
        }
    }
    let clear = clearance(&grid, &below);
    let held = grid.rasterize_region(part);
    let far: Vec<bool> = (0..n)
        .map(|i| {
            let c = grid.center(i % grid.w, i / grid.w);
            c.x < lo.x - 2.0 * d || c.x > hi.x + 2.0 * d || c.y < lo.y - 2.0 * d || c.y > hi.y + 2.0 * d
        })
        .collect();
    // The tabs: the patches of the kerf that stay, each with a core that is not grid noise.
    let patch: Vec<bool> = (0..n).map(|i| above[i] && !below[i]).collect();
    let mut label = vec![0usize; n];
    let mut seen = vec![false; n];
    let mut tabs: Vec<(usize, Point2)> = Vec::new();
    for s in 0..n {
        if !patch[s] || seen[s] {
            continue;
        }
        let cells = flood(&grid, &[s], |i| patch[i] && !std::mem::replace(&mut seen[i], true));
        let core = cells.iter().copied().max_by(|&x, &y| clear[x].total_cmp(&clear[y])).unwrap_or(s);
        if clear[core] >= 3.0 * cell {
            tabs.push((tabs.len() + 1, grid.center(core % grid.w, core / grid.w)));
            cells.iter().for_each(|&i| label[i] = tabs.len());
        }
    }
    let seeds: Vec<usize> = (0..n).filter(|&i| held[i]).collect();
    let joins = |id: usize, t: f64| -> bool {
        let open = |i: usize| !below[i] && clear[i] >= t && (label[i] == 0 || label[i] == id);
        let mut seen = vec![false; n];
        flood(&grid, &seeds, |i| open(i) && !std::mem::replace(&mut seen[i], true)).iter().any(|&i| far[i])
    };
    let cap = lo.x.max(hi.x).abs().max(lo.y.max(hi.y).abs()) + 4.0 * d;
    Ok(tabs
        .iter()
        .map(|&(id, at)| {
            if !joins(id, cell) {
                return Neck { at, width: 0.0 };
            }
            let (mut ok, mut bad) = (cell, cap);
            while bad - ok > 0.1 * cell {
                let mid = 0.5 * (ok + bad);
                if joins(id, mid) { ok = mid } else { bad = mid }
            }
            Neck { at, width: (2.0 * ok - cell).max(0.0) }
        })
        .collect())
}

/// The cells reached from `start` over 4-neighbours. `open` admits a cell one time only: it
/// marks the cell that it admits.
fn flood(grid: &Grid, start: &[usize], mut open: impl FnMut(usize) -> bool) -> Vec<usize> {
    let mut out = Vec::new();
    let mut stack: Vec<usize> = start.iter().copied().filter(|&i| open(i)).collect();
    while let Some(i) = stack.pop() {
        out.push(i);
        let (x, y) = (i % grid.w, i / grid.w);
        let mut step = |j: usize| {
            if open(j) {
                stack.push(j);
            }
        };
        if x > 0 {
            step(i - 1);
        }
        if x + 1 < grid.w {
            step(i + 1);
        }
        if y > 0 {
            step(i - grid.w);
        }
        if y + 1 < grid.h {
            step(i + grid.w);
        }
    }
    out
}

/// The distance (mm) from each cell centre to the nearest `cut` cell centre.
fn clearance(grid: &Grid, cut: &[bool]) -> Vec<f64> {
    let big = 1e18;
    let mut sq: Vec<f64> = cut.iter().map(|&c| if c { 0.0 } else { big }).collect();
    let mut line = vec![0.0; grid.w.max(grid.h)];
    for y in 0..grid.h {
        let row = y * grid.w;
        line[..grid.w].copy_from_slice(&sq[row..row + grid.w]);
        let out = lower_envelope(&line[..grid.w]);
        sq[row..row + grid.w].copy_from_slice(&out);
    }
    for x in 0..grid.w {
        for y in 0..grid.h {
            line[y] = sq[y * grid.w + x];
        }
        let out = lower_envelope(&line[..grid.h]);
        for y in 0..grid.h {
            sq[y * grid.w + x] = out[y];
        }
    }
    sq.iter().map(|v| v.sqrt() * grid.cell).collect()
}

/// One pass of the squared distance transform: `min over j of f[j] + (i - j)^2`.
fn lower_envelope(f: &[f64]) -> Vec<f64> {
    let n = f.len();
    let mut hull: Vec<usize> = Vec::with_capacity(n);
    let mut from: Vec<f64> = Vec::with_capacity(n + 1);
    let meet = |p: usize, q: usize| ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64));
    for q in 0..n {
        while let (Some(&p), Some(&z)) = (hull.last(), from.last()) {
            if hull.len() > 1 && meet(p, q) <= z {
                hull.pop();
                from.pop();
            } else {
                break;
            }
        }
        from.push(match hull.last() {
            Some(&p) => meet(p, q),
            None => f64::NEG_INFINITY,
        });
        hull.push(q);
    }
    let mut k = 0;
    (0..n)
        .map(|i| {
            while k + 1 < hull.len() && from[k + 1] < i as f64 {
                k += 1;
            }
            let p = hull[k];
            f[p] + (i as f64 - p as f64).powi(2)
        })
        .collect()
}
