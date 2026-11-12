//! The part solid as the exact probe reads it: an XY bucket grid over
//! the facets (a uniform-grid broad phase, textbook spatial hashing)
//! and the two questions the solid law asks of it - does an assembly
//! at this tip cross a facet ([`super::probe::tri_pen`]), and, when it
//! crosses none, is it wholly inside (vertical-ray parity). Our own
//! design; no vendor source.

use super::probe::{Atom, Frame};
use super::hole_wall::wall_excuse;
use super::swept::{Body, pen};
use crate::geometry::{Point3, Triangle};

/// Facets whose XY box spans more cells than this ride the always-asked
/// list instead of being copied into every cell.
const BIG_CELLS: usize = 1024;

pub(super) struct SolidIndex<'a> {
    facets: &'a [Triangle],
    lo: [f64; 3],
    hi: [f64; 3],
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
    big: Vec<u32>,
    /// Per-facet box [x0, y0, z0, x1, y1, z1], the narrow-phase reject.
    bx: Vec<[f64; 6]>,
    marks: Vec<u32>,
    stamp: u32,
    cand: Vec<u32>,
}

impl<'a> SolidIndex<'a> {
    /// `None` when the mesh cannot be read honestly (a non-finite
    /// vertex): the caller fails closed. An empty mesh is an empty
    /// solid - nothing to enter.
    pub(super) fn build(facets: &'a [Triangle]) -> Option<Self> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let mut bx = Vec::with_capacity(facets.len());
        for t in facets {
            let mut b = [f64::INFINITY, f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
            for p in [t.a, t.b, t.c] {
                if !p.is_finite() {
                    return None;
                }
                for (k, c) in [p.x, p.y, p.z].into_iter().enumerate() {
                    lo[k] = lo[k].min(c);
                    hi[k] = hi[k].max(c);
                    b[k] = b[k].min(c);
                    b[k + 3] = b[k + 3].max(c);
                }
            }
            bx.push(b);
        }
        let n = facets.len();
        let side = ((n as f64).sqrt() * 0.5).clamp(1.0, 256.0) as usize;
        let ext = if n == 0 { 1.0 } else { (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-6) };
        let cell = ext / side as f64;
        let nx = if n == 0 { 1 } else { (((hi[0] - lo[0]) / cell).floor() as usize + 1).min(side + 1) };
        let ny = if n == 0 { 1 } else { (((hi[1] - lo[1]) / cell).floor() as usize + 1).min(side + 1) };
        let mut ix = Self {
            facets,
            lo,
            hi,
            cell,
            nx,
            ny,
            cells: vec![Vec::new(); nx * ny],
            big: Vec::new(),
            bx,
            marks: vec![0; n],
            stamp: 0,
            cand: Vec::new(),
        };
        for i in 0..n {
            let b = ix.bx[i];
            let (i0, i1, j0, j1) = ix.span(b[0], b[3], b[1], b[4]);
            if (i1 - i0 + 1) * (j1 - j0 + 1) > BIG_CELLS {
                ix.big.push(i as u32);
                continue;
            }
            for j in j0..=j1 {
                for k in i0..=i1 {
                    ix.cells[j * nx + k].push(i as u32);
                }
            }
        }
        Some(ix)
    }

    fn span(&self, x0: f64, x1: f64, y0: f64, y1: f64) -> (usize, usize, usize, usize) {
        let c = |v: f64, o: f64, n: usize| -> usize {
            let f = ((v - o) / self.cell).floor();
            if f.is_nan() || f < 0.0 { 0 } else { (f as usize).min(n - 1) }
        };
        (c(x0, self.lo[0], self.nx), c(x1, self.lo[0], self.nx), c(y0, self.lo[1], self.ny), c(y1, self.lo[1], self.ny))
    }

    /// Facets whose boxes may meet the world box (a superset).
    fn gather(&mut self, b0: [f64; 3], b1: [f64; 3]) {
        self.cand.clear();
        if b1[0] < self.lo[0] || b0[0] > self.hi[0] || b1[1] < self.lo[1] || b0[1] > self.hi[1] || b1[2] < self.lo[2] || b0[2] > self.hi[2] {
            return;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.marks.iter_mut().for_each(|m| *m = 0);
            self.stamp = 1;
        }
        let (i0, i1, j0, j1) = self.span(b0[0], b1[0], b0[1], b1[1]);
        let nx = self.nx;
        for j in j0..=j1 {
            for k in i0..=i1 {
                for &f in &self.cells[j * nx + k] {
                    let fu = f as usize;
                    if self.marks[fu] != self.stamp && overlap(&self.bx[fu], &b0, &b1) {
                        self.marks[fu] = self.stamp;
                        self.cand.push(f);
                    }
                }
            }
        }
        for &f in &self.big {
            if overlap(&self.bx[f as usize], &b0, &b1) {
                self.cand.push(f);
            }
        }
    }

    /// Does the assembly swept from `fr`'s tip along the local move `d`
    /// share volume with the solid beyond `allow`? `grow` widens every
    /// atom (the arc chords' sagitta); `walls` are the containment
    /// kinds' hole axes in `fr`'s XY, whose round walls' tessellation is
    /// excused ([`wall_excuse`], #1205) - empty for every other law.
    pub(super) fn hits(&mut self, fr: &Frame, d: [f64; 3], atoms: &[Atom], allow: f64, grow: f64, walls: &[[f64; 2]]) -> bool {
        let len = atoms.iter().map(|a| a.z1).fold(0.0, f64::max);
        let (p0, p1) = (fr.up(0.0), fr.up(len));
        let e = fr.end(d);
        let (q0, q1) = (Point3::new(p0.x + e.x, p0.y + e.y, p0.z + e.z), Point3::new(p1.x + e.x, p1.y + e.y, p1.z + e.z));
        let lo3 = |k: usize| [p0, p1, q0, q1].iter().map(|p| [p.x, p.y, p.z][k]).fold(f64::INFINITY, f64::min);
        let hi3 = |k: usize| [p0, p1, q0, q1].iter().map(|p| [p.x, p.y, p.z][k]).fold(f64::NEG_INFINITY, f64::max);
        let (axis_lo, axis_hi) = ([lo3(0), lo3(1), lo3(2)], [hi3(0), hi3(1), hi3(2)]);
        let mut pts = Vec::with_capacity(32);
        let upright = fr.is_upright();
        // One broad phase per atom when upright (its own radius and Z
        // window); one for the whole stack when the axis leans.
        let groups: Vec<(usize, usize)> = if upright { (0..atoms.len()).map(|i| (i, i + 1)).collect() } else { vec![(0, atoms.len())] };
        for (g0, g1) in groups {
            let set = &atoms[g0..g1];
            let r = set.iter().map(Atom::rho_max).fold(0.0, f64::max) + grow;
            let (b0, b1) = if upright {
                let zl = set.iter().map(|a| a.z0.max(allow)).fold(f64::INFINITY, f64::min);
                let zh = set.iter().map(|a| a.z1).fold(f64::NEG_INFINITY, f64::max);
                let (za, zb) = (p0.z.min(q0.z), p0.z.max(q0.z));
                ([axis_lo[0] - r, axis_lo[1] - r, za + zl], [axis_hi[0] + r, axis_hi[1] + r, zb + zh])
            } else {
                ([axis_lo[0] - r, axis_lo[1] - r, axis_lo[2] - r], [axis_hi[0] + r, axis_hi[1] + r, axis_hi[2] + r])
            };
            self.gather(b0, b1);
            for &f in &self.cand {
                let t = fr.tri(&self.facets[f as usize]);
                let body = Body::new(&t, d);
                for a in set {
                    let pen = pen(&body, a, allow, allow, grow, &mut pts);
                    if pen > allow {
                        let ex = wall_excuse(&t, d, a, allow, grow, walls);
                        if pen > ex + allow {
                            return true;
                        }
                    }
                }
            }
        }
        // Crossing nothing, the swept assembly is wholly inside or wholly
        // outside; it can be inside only if its axis is inside the box.
        let within = (0..3).all(|k| axis_lo[k] >= self.lo[k] && axis_hi[k] <= self.hi[k]);
        within && self.inside(fr.up(atoms.first().map_or(0.0, |a| 0.5 * (a.z0 + a.z1))))
    }

    /// Ray parity up +Z from `p`. A ray that grazes an edge or vertex
    /// is retried a hair aside; three degenerate tries fail closed.
    fn inside(&mut self, p: Point3) -> bool {
        for (dx, dy) in [(0.0, 0.0), (1.37e-7, 0.71e-7), (-0.93e-7, 1.73e-7)] {
            let q = Point3::new(p.x + dx, p.y + dy, p.z);
            self.gather([q.x, q.y, q.z], [q.x, q.y, self.hi[2] + 1.0]);
            let mut count = 0usize;
            let mut clean = true;
            for &f in &self.cand {
                let t = &self.facets[f as usize];
                let o = |a: Point3, b: Point3| (b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x);
                let (d1, d2, d3) = (o(t.a, t.b), o(t.b, t.c), o(t.c, t.a));
                let area = (t.b.x - t.a.x) * (t.c.y - t.a.y) - (t.b.y - t.a.y) * (t.c.x - t.a.x);
                if area == 0.0 {
                    continue; // vertical facet: its neighbours carry the crossing
                }
                let s = area.signum();
                let (e1, e2, e3) = (d1 * s, d2 * s, d3 * s);
                if e1 < 0.0 || e2 < 0.0 || e3 < 0.0 {
                    continue;
                }
                if e1 == 0.0 || e2 == 0.0 || e3 == 0.0 {
                    clean = false;
                    break;
                }
                // Barycentric height of the facet over q.
                let (w1, w2, w3) = (e2 / area.abs(), e3 / area.abs(), e1 / area.abs());
                let z = w1 * t.a.z + w2 * t.b.z + w3 * t.c.z;
                if z > q.z {
                    count += 1;
                } else if z == q.z {
                    clean = false;
                    break;
                }
            }
            if clean {
                return count % 2 == 1;
            }
        }
        true
    }
}

/// Does facet box `f` meet the world box `b0..b1`?
fn overlap(f: &[f64; 6], b0: &[f64; 3], b1: &[f64; 3]) -> bool {
    (0..3).all(|k| f[k + 3] >= b0[k] && f[k] <= b1[k])
}
