//! A routed microlift traverse (M11.4 C round 4, `lift`): where the
//! straight chord between two pieces would sweep standing stock (the
//! strip the walls keep until their last lap, an island), the return
//! is routed through the cleared area instead - the free cells are the
//! cutter centers whose disc holds no stock and no wall, read off the
//! Euclidean distance transform of the planner's stock (Felzenszwalb &
//! Huttenlocher 2012, "Distance transforms of sampled functions",
//! Theory of Computing 8), the route the shortest 8-connected path
//! between them (A*, Hart, Nilsson & Raphael 1968), pulled taut to
//! straight legs each proved on the stock itself (the classical
//! string-pulling of grid paths). Ours; every bound is ours.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::geo2d::Point2;

use super::raster::Raster;

/// Cells further than this from an end (mm) are not tried as its door.
const DOOR: f64 = 6.0;

/// A leg shorter than this (mm) is no move: always legal.
pub(super) const ZERO_LEG: f64 = 1e-9;

/// Squared distance, in cells, of every cell center to the nearest
/// stock or wall cell (`f64::INFINITY` with none on the grid).
pub(super) fn clearance(ras: &Raster) -> Vec<f64> {
    let (nx, ny) = ras.dims();
    let big = 1e30;
    let mut g: Vec<f64> = (0..nx * ny).map(|i| if ras.blocked(i) { 0.0 } else { big }).collect();
    let n = nx.max(ny);
    let (mut f, mut d, mut v, mut z) = (vec![0.0; n], vec![0.0; n], vec![0usize; n], vec![0.0; n + 1]);
    for x in 0..nx {
        for y in 0..ny {
            f[y] = g[x + y * nx];
        }
        edt_1d(&f[..ny], &mut d[..ny], &mut v, &mut z);
        for y in 0..ny {
            g[x + y * nx] = d[y];
        }
    }
    for y in 0..ny {
        f[..nx].copy_from_slice(&g[y * nx..(y + 1) * nx]);
        edt_1d(&f[..nx], &mut d[..nx], &mut v, &mut z);
        g[y * nx..(y + 1) * nx].copy_from_slice(&d[..nx]);
    }
    g.iter().map(|&q| if q >= 1e29 { f64::INFINITY } else { q }).collect()
}

/// The 1-D squared distance transform of `f` (lower envelope of
/// parabolas, Felzenszwalb & Huttenlocher).
fn edt_1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let fq = f[q] + (q * q) as f64;
        loop {
            let p = v[k];
            let s = (fq - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64));
            // z[0] is minus infinity: k never drops below 0.
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f64::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, dq) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        *dq = (q as f64 - p as f64).powi(2) + f[p];
    }
}

/// The via points of a traverse from `a` to `b` whose every leg passes
/// `leg_ok`, through cells clear by `rad` (mm) on `clear` ([`clearance`])
/// and more than `gap` off the `orbit` circle; `None` where no route is
/// found. `leg_ok` holds a leg's capsule of radius `rad` clear of stock
/// and wall ([`Raster::capsule_clear`]), its start disc included, or
/// passes a leg shorter than [`ZERO_LEG`].
pub(super) fn route(
    ras: &Raster,
    clear: &[f64],
    a: Point2,
    b: Point2,
    (rad, orbit, gap): (f64, (Point2, f64), f64),
    leg_ok: &dyn Fn(Point2, Point2) -> bool,
) -> Option<Vec<Point2>> {
    let (nx, ny) = ras.dims();
    let need = (rad / ras.h + 0.75).powi(2);
    // Free: clear of stock and wall, and off the entry orbit's circle.
    let off = |q: Point2| orbit.1 <= 1e-6 || ((q.x - orbit.0.x).hypot(q.y - orbit.0.y) - orbit.1).abs() > gap + ras.h;
    let free = |i: usize| clear.get(i).is_some_and(|&c| c >= need) && off(ras.center(i));
    let (s, t) = (door(ras, a, rad, &free, leg_ok)?, door(ras, b, rad, &free, leg_ok)?);
    let cells = astar(nx, ny, s, t, &free)?;
    let mut path: Vec<Point2> = vec![a];
    path.extend(cells.iter().map(|&i| ras.center(i)));
    path.push(b);
    // Pulled taut: from each kept point, the farthest point a legal leg
    // reaches.
    let mut via = Vec::new();
    let mut i = 0usize;
    while i + 1 < path.len() {
        let mut j = i + 1;
        let mut k = path.len() - 1;
        while k > j {
            if leg_ok(path[i], path[k]) {
                j = k;
                break;
            }
            k = j + (k - j) / 2;
        }
        if !leg_ok(path[i], path[j]) {
            return None;
        }
        if j + 1 < path.len() {
            via.push(path[j]);
        }
        i = j;
    }
    Some(via)
}

/// The door of `p`: the nearest `free` cell within [`DOOR`] that a leg
/// from `p` passing `leg_ok` reaches (the lower index on a tie); `None`
/// where none does. `leg_ok` holds the law [`route`] names.
fn door(ras: &Raster, p: Point2, rad: f64, free: &dyn Fn(usize) -> bool, leg_ok: &dyn Fn(Point2, Point2) -> bool) -> Option<usize> {
    let (nx, ny) = ras.dims();
    let (cx, cy) = ras.cell_of(p)?;
    // Every leg's capsule holds its start disc: where that disc holds
    // stock or wall, no leg longer than `ZERO_LEG` from `p` is legal,
    // and the cells are not tried one by one (issue #2736: each such
    // door read every cell within `DOOR` - seconds a door).
    let open = ras.capsule_clear(p, p, rad);
    let reach = (DOOR / ras.h).ceil() as isize;
    let mut near: Vec<(f64, usize)> = Vec::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let (x, y) = (cx as isize + dx, cy as isize + dy);
            if x < 0 || y < 0 || x >= nx as isize || y >= ny as isize {
                continue;
            }
            let i = x as usize + y as usize * nx;
            let d = ((dx * dx + dy * dy) as f64).sqrt();
            let c = ras.center(i);
            if d * ras.h <= DOOR && (open || (c.x - p.x).hypot(c.y - p.y) < ZERO_LEG) && free(i) {
                near.push((d, i));
            }
        }
    }
    // Tried nearest first, so the leg law reads no cell past the door.
    near.sort_by(|u, v| u.0.total_cmp(&v.0).then(u.1.cmp(&v.1)));
    near.into_iter().find(|&(_, i)| leg_ok(p, ras.center(i))).map(|(_, i)| i)
}

/// Shortest 8-connected path of free cells from `s` to `t` (both ends
/// included), A* on the Euclidean metric; capped at one visit a cell.
fn astar(nx: usize, ny: usize, s: usize, t: usize, free: &dyn Fn(usize) -> bool) -> Option<Vec<usize>> {
    let n = nx * ny;
    let (tx, ty) = ((t % nx) as f64, (t / nx) as f64);
    let h = |i: usize| ((i % nx) as f64 - tx).hypot((i / nx) as f64 - ty);
    let key = |c: f64| Reverse((c * 1024.0) as u64);
    let mut g = vec![f64::INFINITY; n];
    let mut from = vec![usize::MAX; n];
    let mut heap = BinaryHeap::new();
    g[s] = 0.0;
    heap.push((key(h(s)), s));
    let mut visits = 0usize;
    while let Some((_, i)) = heap.pop() {
        if i == t {
            let mut out = vec![t];
            let mut c = t;
            while c != s && from[c] != usize::MAX {
                c = from[c];
                out.push(c);
            }
            out.reverse();
            return Some(out);
        }
        visits += 1;
        if visits > n {
            return None;
        }
        let (x, y) = ((i % nx) as isize, (i / nx) as isize);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (qx, qy) = (x + dx, y + dy);
            if qx < 0 || qy < 0 || qx >= nx as isize || qy >= ny as isize {
                continue;
            }
            let q = qx as usize + qy as usize * nx;
            if !free(q) {
                continue;
            }
            let ng = g[i] + if dx != 0 && dy != 0 { std::f64::consts::SQRT_2 } else { 1.0 };
            if ng < g[q] {
                g[q] = ng;
                from[q] = i;
                heap.push((key(ng + h(q)), q));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_distance_transform_is_the_brute_force_one() {
        let f: Vec<f64> = (0..40).map(|i| if i % 13 == 3 || i == 37 { 0.0 } else { 1e30 }).collect();
        let (mut d, mut v, mut z) = (vec![0.0; 40], vec![0usize; 40], vec![0.0; 41]);
        edt_1d(&f, &mut d, &mut v, &mut z);
        for (q, &dq) in d.iter().enumerate() {
            let brute = (0..40).filter(|&p| f[p] == 0.0).map(|p| (q as f64 - p as f64).powi(2)).fold(f64::INFINITY, f64::min);
            assert!((dq - brute).abs() < 1e-9, "cell {q}: {dq} vs {brute}");
        }
    }

    /// A 40 x 40 block of stock (cells `h`) with a channel cleared along
    /// y = 20, 10 mm wide.
    fn channel(h: f64) -> Raster {
        let pts = vec![Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), Point2::new(40.0, 40.0), Point2::new(0.0, 40.0)];
        let Ok(l) = crate::geo2d::Loop::new(pts) else { panic!("loop") };
        let Ok(reach) = crate::geo2d::Region::new(vec![l]) else { panic!("region") };
        let Ok(mut ras) = Raster::from_region(&reach, h, 5.0) else { panic!("raster") };
        ras.cut(Point2::new(5.0, 20.0), Point2::new(35.0, 20.0), 5.0);
        ras
    }

    /// Issue #2736: a door from a point whose own disc holds stock is
    /// refused without one leg read - each one used to read every cell
    /// within `DOOR`, seconds a door on the planner's grid.
    #[test]
    fn a_door_on_stock_reads_no_leg() {
        let ras = channel(0.1);
        let rad = 2.95;
        let reads = std::cell::Cell::new(0usize);
        let leg_ok = |a: Point2, b: Point2| {
            reads.set(reads.get() + 1);
            (b.x - a.x).hypot(b.y - a.y) < ZERO_LEG || ras.capsule_clear(a, b, rad)
        };
        // The disc at (20, 29) reaches y = 26.95: stock past the channel.
        assert_eq!(door(&ras, Point2::new(20.0, 29.0), rad, &|_| true, &leg_ok), None);
        assert_eq!(reads.get(), 0, "the leg law was read {} times", reads.get());
    }

    /// The door is the one the cell-by-cell search finds: the nearest
    /// free cell a legal leg reaches, the lower index on a tie.
    #[test]
    fn the_door_is_the_nearest_legal_cell() {
        let ras = channel(0.2);
        let rad = 2.95;
        let free = |i: usize| !i.is_multiple_of(3);
        // A stricter law than the capsule's: legs of 2 mm or more, eastward.
        let leg_ok = |a: Point2, b: Point2| ras.capsule_clear(a, b, rad) && (b.x - a.x).hypot(b.y - a.y) >= 2.0 && b.x > a.x;
        // The last point's disc reaches the channel wall: no door either way.
        let cases = [(20.0, 21.3, true), (9.1, 18.7, true), (30.0, 20.0, true), (31.0, 21.6, true), (31.0, 22.4, false)];
        for (x, y, open) in cases {
            let p = Point2::new(x, y);
            let (nx, ny) = ras.dims();
            let Some((cx, cy)) = ras.cell_of(p) else { panic!("off the grid") };
            let reach = (DOOR / ras.h).ceil() as isize;
            let mut best: Option<(f64, usize)> = None;
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (x, y) = (cx as isize + dx, cy as isize + dy);
                    if x < 0 || y < 0 || x >= nx as isize || y >= ny as isize {
                        continue;
                    }
                    let i = x as usize + y as usize * nx;
                    let d = ((dx * dx + dy * dy) as f64).sqrt();
                    if free(i) && best.is_none_or(|b| d < b.0) && d * ras.h <= DOOR && leg_ok(p, ras.center(i)) {
                        best = Some((d, i));
                    }
                }
            }
            assert_eq!(best.is_some(), open, "door at {p:?}: the case is not the one named");
            assert_eq!(door(&ras, p, rad, &free, &leg_ok), best.map(|b| b.1), "door at {p:?}");
        }
    }
}
