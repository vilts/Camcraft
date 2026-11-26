//! The vertex weld and the union-find of the winding check (#366): corners
//! within a tolerance are one vertex, and facets link into bodies.
//! Provenance: spatial hashing (Teschner et al., VMV 2003); union-find with
//! path halving (Tarjan & van Leeuwen, JACM 1984). Our own code, moved out of
//! `pick::winding` unchanged.

use std::collections::HashMap;

use crate::geometry::Triangle;

/// Corners closer than this (mm) are one vertex: well under any machined
/// feature, well over the seam drift of a tessellated STEP face pair (a
/// shipped sample's seams differ by one 1 nm lattice step).
const WELD_TOL_MM: f64 = 1e-4;

type Cell = (i64, i64, i64);

/// Welded vertex ids by spatial hashing: a corner joins the first earlier
/// corner within [`WELD_TOL_MM`] found in its own or a neighbouring cell
/// (cell size = the tolerance), so two copies of one seam vertex a
/// rounding step apart weld whichever side of a cell wall they land on.
#[derive(Default)]
pub(super) struct Welder {
    cells: HashMap<Cell, Vec<u32>>,
    pub(super) pts: Vec<[f64; 3]>,
}

impl Welder {
    /// The id of `p`, `None` for a non-finite or absurd coordinate (never
    /// saturated onto a neighbour) or when the id space is spent.
    fn id(&mut self, p: [f64; 3]) -> Option<u32> {
        let cell = |v: f64| {
            let q = (v / WELD_TOL_MM).floor();
            (q.is_finite() && (-9.0e18..=9.0e18).contains(&q)).then_some(q as i64)
        };
        let c = (cell(p[0])?, cell(p[1])?, cell(p[2])?);
        let tol2 = WELD_TOL_MM * WELD_TOL_MM;
        for dx in -1..=1_i64 {
            for dy in -1..=1_i64 {
                for dz in -1..=1_i64 {
                    let k = (c.0.saturating_add(dx), c.1.saturating_add(dy), c.2.saturating_add(dz));
                    let Some(ids) = self.cells.get(&k) else { continue };
                    for &id in ids {
                        let Some(q) = self.pts.get(id as usize) else { continue };
                        let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
                        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= tol2 {
                            return Some(id);
                        }
                    }
                }
            }
        }
        let id = u32::try_from(self.pts.len()).ok()?;
        self.pts.push(p);
        self.cells.entry(c).or_default().push(id);
        Some(id)
    }

    pub(super) fn corners(&mut self, t: &Triangle) -> Option<[u32; 3]> {
        Some([self.id([t.a.x, t.a.y, t.a.z])?, self.id([t.b.x, t.b.y, t.b.z])?, self.id([t.c.x, t.c.y, t.c.z])?])
    }
}

/// Union-find root with path halving; indices are always in range.
pub(super) fn root(parent: &mut [u32], mut i: u32) -> u32 {
    while let Some(&p) = parent.get(i as usize) {
        if p == i {
            break;
        }
        let gp = parent.get(p as usize).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(i as usize) {
            *slot = gp;
        }
        i = gp;
    }
    i
}

pub(super) fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (root(parent, a), root(parent, b));
    if ra != rb
        && let Some(slot) = parent.get_mut(ra as usize)
    {
        *slot = rb;
    }
}
