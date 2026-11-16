//! STEP shell orientation (#1573 review): which faces of a closed shell to
//! flip so each agrees with the faces it meets - decided the same way on
//! every run, from the votes alone.
//!
//! Provenance: consistent orientation of a triangulated 2-manifold by
//! propagating relative orientation across shared edges (Botsch et al.,
//! "Polygon Mesh Processing", A K Peters 2010, ch. 1), solved as a
//! union-find with parity (Tarjan, J. ACM 22(2), 1975; the parity variant
//! is the textbook bipartiteness check), strongest evidence first (Kruskal's
//! greedy order, Proc. AMS 7(1), 1956); outward by the sign of the enclosed
//! volume (divergence theorem). Our own code; nothing from any CAM vendor.
//!
//! The rules (decision file `docs/decisions/1573-closed-shell-watertight.md`):
//! - Face pairs are joined in a fixed order: the most exact shared edges
//!   (margin of agreeing over disagreeing) first, then T-junction votes,
//!   then the face numbers. A weak vote never overrides a stronger one, and
//!   no hash order reaches the result.
//! - In each connected group the side holding more faces keeps the kernel's
//!   winding (a tie: the side of the group's lowest face).
//! - Only when that flips some face is the outward polarity decided, once,
//!   from the signed volume of the WHOLE shell: a cavity wound inward, or a
//!   lone face, keeps the kernel's winding.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use crate::geometry::{Point3, Triangle};

/// One face pair's evidence, `[agree, disagree]`: exact shared edges, and
/// T-junction sides that lie along each other.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Vote {
    pub exact: [u32; 2],
    pub near: [u32; 2],
}

/// What [`orient`] decided.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Oriented {
    /// Faces flipped, ascending.
    pub flipped: Vec<usize>,
    /// Face pairs whose net vote the flips do not satisfy.
    pub contradictions: usize,
    /// Shared edges that still run the same way in both faces after the
    /// flips: an exact winding fault left in the mesh.
    pub unresolved_edges: usize,
    /// The face pair holding the most of them.
    pub unresolved_pair: Option<(usize, usize)>,
}

/// Union-find with parity: each face's winding relative to its set's root.
struct Parity {
    up: Vec<usize>,
    odd: Vec<bool>,
    size: Vec<usize>,
}

impl Parity {
    fn new(n: usize) -> Self {
        Parity { up: (0..n).collect(), odd: vec![false; n], size: vec![1; n] }
    }

    /// The root of `f` and whether `f` is wound against it.
    fn find(&mut self, f: usize) -> (usize, bool) {
        let mut path = Vec::new();
        let (mut r, mut odd) = (f, false);
        while let Some(&u) = self.up.get(r) {
            if u == r {
                break;
            }
            path.push(r);
            odd ^= self.odd.get(r).copied().unwrap_or(false);
            r = u;
        }
        let mut acc = odd;
        for n in path {
            let own = self.odd.get(n).copied().unwrap_or(false);
            if let (Some(u), Some(o)) = (self.up.get_mut(n), self.odd.get_mut(n)) {
                (*u, *o) = (r, acc);
            }
            acc ^= own;
        }
        (r, odd)
    }

    /// Join `f` and `g` with `g` wound against `f` iff `flip`; false when
    /// they are already joined the other way (a contradiction).
    fn join(&mut self, f: usize, g: usize, flip: bool) -> bool {
        let ((rf, pf), (rg, pg)) = (self.find(f), self.find(g));
        if rf == rg {
            return (pf ^ pg) == flip;
        }
        let size = |s: &Self, r: usize| s.size.get(r).copied().unwrap_or(1);
        let (big, small) = if size(self, rf) >= size(self, rg) { (rf, rg) } else { (rg, rf) };
        let total = size(self, big).saturating_add(size(self, small));
        if let (Some(u), Some(o)) = (self.up.get_mut(small), self.odd.get_mut(small)) {
            (*u, *o) = (big, pf ^ pg ^ flip);
        }
        if let Some(s) = self.size.get_mut(big) {
            *s = total;
        }
        true
    }
}

fn margin(v: [u32; 2]) -> u32 {
    v[0].abs_diff(v[1])
}

/// The flips for `nf` faces (face `i` owns `tris[ends[i-1]..ends[i]]`) from
/// the `votes` per face pair (lower, higher). `tris` is only read here.
pub(super) fn orient(tris: &[Triangle], ends: &[usize], votes: &BTreeMap<(u32, u32), Vote>) -> Oriented {
    let nf = ends.len();
    let mut out = Oriented::default();
    let mut order: Vec<(&(u32, u32), &Vote)> =
        votes.iter().filter(|(_, v)| margin(v.exact) > 0 || margin(v.near) > 0).collect();
    order.sort_by_key(|&(&k, v)| (Reverse(margin(v.exact)), Reverse(margin(v.near)), k));
    let mut uf = Parity::new(nf);
    for (&(f, g), v) in order {
        let flip = if margin(v.exact) > 0 { v.exact[1] > v.exact[0] } else { v.near[1] > v.near[0] };
        if !uf.join(f as usize, g as usize, flip) {
            out.contradictions += 1;
        }
    }
    // Each group: the side with more faces keeps the kernel's winding.
    let found: Vec<(usize, bool)> = (0..nf).map(|f| uf.find(f)).collect();
    let mut sides: BTreeMap<usize, (usize, usize, bool)> = BTreeMap::new(); // root -> (kept, against, lowest's parity)
    for &(root, odd) in &found {
        let s = sides.entry(root).or_insert((0, 0, odd));
        if odd { s.1 += 1 } else { s.0 += 1 }
    }
    let mut flip: Vec<bool> = found
        .iter()
        .map(|&(root, odd)| {
            let (kept, against, lowest) = sides.get(&root).copied().unwrap_or((1, 0, false));
            let invert = against > kept || (against == kept && lowest);
            odd ^ invert
        })
        .collect();
    if flip.iter().any(|&f| f) && shell_volume(tris, ends, &flip) < 0.0 {
        for f in &mut flip {
            *f = !*f;
        }
    }
    let mut worst = 0;
    for (&(f, g), v) in votes {
        let differ = flip.get(f as usize) != flip.get(g as usize);
        let left = if differ { v.exact[0] } else { v.exact[1] } as usize;
        out.unresolved_edges += left;
        if left > worst {
            worst = left;
            out.unresolved_pair = Some((f as usize, g as usize));
        }
    }
    out.flipped = flip.iter().enumerate().filter(|(_, f)| **f).map(|(i, _)| i).collect();
    out
}

/// The facets of face `f`.
pub(super) fn range(ends: &[usize], f: usize) -> std::ops::Range<usize> {
    let lo = f.checked_sub(1).and_then(|p| ends.get(p).copied()).unwrap_or(0);
    lo..ends.get(f).copied().unwrap_or(lo).max(lo)
}

/// Six times the volume the whole shell encloses with `flip` applied
/// (tetrahedra to the shell's facet centroid): positive when outward.
fn shell_volume(tris: &[Triangle], ends: &[usize], flip: &[bool]) -> f64 {
    let n = tris.len().max(1) as f64;
    let o = tris.iter().fold(Point3::new(0.0, 0.0, 0.0), |s, t| Point3::new(s.x + t.a.x / n, s.y + t.a.y / n, s.z + t.a.z / n));
    (0..ends.len())
        .map(|f| {
            let v: f64 = tris.get(range(ends, f)).unwrap_or(&[]).iter().map(|t| (t.a - o).dot((t.b - o).cross(t.c - o))).sum();
            if flip.get(f).copied().unwrap_or(false) { -v } else { v }
        })
        .sum()
}

#[cfg(test)]
#[path = "step_orient_tests.rs"]
mod tests;
