//! The walls of the legal center area as ONE fixed vertex list (issue
//! 2031). Every rest pass that rides a wall rides it on these vertices,
//! so two passes over the same stretch of wall share their chords
//! exactly and can never cross by a rounding (the verifier's crossing
//! law reads exact signs; a shared end point or a collinear retrace is
//! no crossing, a chord a nanometre off another's line is one).
//! Provenance: our own bookkeeping over the polygon clipper's output
//! (its intersection vertices are snapped to a 1 nm lattice and its
//! offsets rebuild a curved wall on other chords, which is what makes a
//! re-derived wall vertex stand off the wall's line); the
//! bucket grid is the textbook uniform-grid proximity query (de Berg et
//! al., "Computational Geometry", 3rd ed., Springer 2008).

use crate::geo2d::{Point2, Region};

use std::collections::HashMap;

use super::super::front::dist;

/// A room vertex this close to a wall chord (mm) stands on the wall:
/// past the 5 um flats the polygon offsets leave on a wall they rebuild
/// (an opened room's walls are the same walls, cut into other chords).
const ON_WALL: f64 = 0.05;

/// A wall vertex: (loop, vertex index).
pub(super) type WallAt = (usize, usize);

/// A point of a pass: on the wall or off it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Vtx {
    pub p: Point2,
    pub at: Option<WallAt>,
}

/// The wall loops, densified once, with their arc lengths and a bucket
/// grid of their chords.
pub(super) struct Canon {
    loops: Vec<Vec<Point2>>,
    /// Per loop: arc length at each vertex, the total last.
    s: Vec<Vec<f64>>,
    x0: f64,
    y0: f64,
    c: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<(u32, u32)>>,
    /// Each wall vertex by its exact coordinates.
    index: HashMap<(u64, u64), (usize, usize)>,
}

/// A loop with points put on its edges so no chord is longer than
/// `seg`; the loop's own vertices stay exactly as they are.
fn cut_edges(pts: &[Point2], seg: f64) -> Vec<Point2> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        out.push(a);
        let k = ((dist(a, b) / seg).ceil() as usize).clamp(1, 4096);
        for j in 1..k {
            let t = j as f64 / k as f64;
            out.push(Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    out
}

impl Canon {
    /// The walls of `center`, no chord longer than `seg`.
    pub(super) fn new(center: &Region, seg: f64) -> Self {
        let loops: Vec<Vec<Point2>> =
            center.loops().iter().map(|l| cut_edges(l.points(), seg.max(1e-6))).filter(|l| l.len() >= 3).collect();
        let mut index = HashMap::new();
        for (l, lp) in loops.iter().enumerate() {
            for (k, p) in lp.iter().enumerate() {
                index.insert((p.x.to_bits(), p.y.to_bits()), (l, k));
            }
        }
        let s = loops
            .iter()
            .map(|l| {
                let mut s = Vec::with_capacity(l.len() + 1);
                s.push(0.0);
                for i in 0..l.len() {
                    s.push(s[i] + dist(l[i], l[(i + 1) % l.len()]));
                }
                s
            })
            .collect();
        let (lo, hi) = center.bounds().unwrap_or((Point2::new(0.0, 0.0), Point2::new(1.0, 1.0)));
        let span = (hi.x - lo.x).max(hi.y - lo.y).max(1e-6);
        let c = (2.0 * seg).max(span / 512.0).max(1e-6);
        let nx = ((hi.x - lo.x) / c).floor() as usize + 3;
        let ny = ((hi.y - lo.y) / c).floor() as usize + 3;
        let mut canon = Canon { loops, s, x0: lo.x - c, y0: lo.y - c, c, nx, ny, cells: vec![Vec::new(); nx * ny], index };
        for l in 0..canon.loops.len() {
            let n = canon.loops[l].len();
            for k in 0..n {
                let (a, b) = (canon.loops[l][k], canon.loops[l][(k + 1) % n]);
                let (i0, j0) = canon.cell(a.x.min(b.x) - ON_WALL, a.y.min(b.y) - ON_WALL);
                let (i1, j1) = canon.cell(a.x.max(b.x) + ON_WALL, a.y.max(b.y) + ON_WALL);
                for j in j0..=j1 {
                    for i in i0..=i1 {
                        canon.cells[i + j * nx].push((l as u32, k as u32));
                    }
                }
            }
        }
        canon
    }

    fn cell(&self, x: f64, y: f64) -> (usize, usize) {
        let i = ((x - self.x0) / self.c).floor().clamp(0.0, (self.nx - 1) as f64) as usize;
        let j = ((y - self.y0) / self.c).floor().clamp(0.0, (self.ny - 1) as f64) as usize;
        (i, j)
    }

    /// Is `a -> b` a chord of a wall itself: two wall vertices next to
    /// each other on one loop? (Legal by construction; a point put on a
    /// slanted wall stands an ulp off its line, where a segment test
    /// against that same wall is noise.)
    pub(super) fn wall_chord(&self, a: Point2, b: Point2) -> bool {
        let at = |p: Point2| self.index.get(&(p.x.to_bits(), p.y.to_bits())).copied();
        match (at(a), at(b)) {
            (Some((la, ka)), Some((lb, kb))) if la == lb => {
                let n = self.loops[la].len();
                (ka + 1) % n == kb || (kb + 1) % n == ka
            }
            _ => false,
        }
    }

    /// The wall vertex `k` of loop `l`.
    pub(super) fn point(&self, (l, k): (usize, usize)) -> Point2 {
        self.loops.get(l).and_then(|lp| lp.get(k)).copied().unwrap_or_default()
    }

    /// The vertex `step` (+1 / -1) along loop `l` from `k`.
    pub(super) fn next(&self, (l, k): (usize, usize), step: isize) -> (usize, usize) {
        let n = self.loops.get(l).map_or(1, Vec::len).max(1);
        (l, (k as isize + step).rem_euclid(n as isize) as usize)
    }

    /// Where `p` stands on the walls: (loop, vertex index + fraction
    /// along the chord to the next vertex), or `None` off them.
    fn locate(&self, p: Point2) -> Option<(usize, f64)> {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return None;
        }
        let (i, j) = self.cell(p.x, p.y);
        let mut best: Option<(f64, usize, f64)> = None;
        for &(l, k) in &self.cells[i + j * self.nx] {
            let (l, k) = (l as usize, k as usize);
            let n = self.loops[l].len();
            let (a, b) = (self.loops[l][k], self.loops[l][(k + 1) % n]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let dd = dx * dx + dy * dy;
            let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
            let d = (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy);
            if d <= ON_WALL && best.is_none_or(|b| d < b.0) {
                best = Some((d, l, k as f64 + t));
            }
        }
        best.map(|(_, l, x)| (l, x))
    }

    /// Arc length along loop `l` forward from position `a` to `b`.
    fn fwd(&self, l: usize, a: f64, b: f64) -> f64 {
        let s = &self.s[l];
        let n = s.len() - 1;
        let at = |x: f64| {
            let i = (x.floor().max(0.0) as usize).min(n - 1);
            s[i] + (x - i as f64).clamp(0.0, 1.0) * (s[i + 1] - s[i])
        };
        (at(b) - at(a)).rem_euclid(s[n].max(1e-12))
    }

    /// A closed room loop with every wall stretch replaced by the wall's
    /// own vertices. A vertex where the room's boundary leaves or meets
    /// the wall (a clipper intersection, a nanometre off the chord) is
    /// dropped: the pass leaves the wall at a wall vertex. Every chord of
    /// a pass is proved inside the center area afterwards (`peel`).
    pub(super) fn snap(&self, pts: &[Point2]) -> Vec<Vtx> {
        let n = pts.len();
        let pos: Vec<Option<(usize, f64)>> = pts.iter().map(|&p| self.locate(p)).collect();
        // Is the room edge i -> i+1 a stretch of wall?
        let links: Vec<bool> = (0..n)
            .map(|i| match (pos[i], pos[(i + 1) % n]) {
                (Some((la, a)), Some((lb, b))) if la == lb => {
                    self.fwd(la, a, b) <= dist(pts[i], pts[(i + 1) % n]) * 1.01 + 1e-5
                }
                _ => false,
            })
            .collect();
        let wall = |l: usize, k: usize| Vtx { p: self.loops[l][k], at: Some((l, k)) };
        let Some(gap) = links.iter().position(|&l| !l) else {
            // The room's loop IS a wall loop.
            return pos.first().copied().flatten().map_or(Vec::new(), |(l, _)| (0..self.loops[l].len()).map(|k| wall(l, k)).collect());
        };
        let i0 = (gap + 1) % n;
        let mut out = Vec::with_capacity(n);
        let mut i = 0;
        while i < n {
            let a = (i0 + i) % n;
            let Some((l, xa)) = pos[a] else {
                out.push(Vtx { p: pts[a], at: None });
                i += 1;
                continue;
            };
            let mut j = i;
            while j + 1 < n && links[(i0 + j) % n] {
                j += 1;
            }
            let xb = pos[(i0 + j) % n].map_or(xa, |q| q.1);
            let m = self.loops[l].len();
            // The wall vertices from the first at or after `xa` to the
            // last at or before `xb`, forward (the stretch may pass the
            // loop's seam).
            let first = (xa - 1e-9).ceil().max(0.0) as usize;
            let mut last = (xb + 1e-9).floor().max(0.0) as usize;
            if j > i && xb < xa {
                last += m;
            }
            // (A lone wall vertex stays only when it is one of the
            // wall's own: then `first == last`.)
            for k in first..=last.min(first + m) {
                out.push(wall(l, k % m));
            }
            i = j + 1;
        }
        out
    }
}
