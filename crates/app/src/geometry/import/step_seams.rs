//! STEP shell seams (#1573 review): how a closed shell's faces meet once
//! meshed, and the one fix the importer makes to them.
//!
//! Provenance: edge manifoldness and consistent orientation of a
//! triangulated 2-manifold (each interior edge used once in each
//! direction; orientation propagated face to face across shared edges) are
//! textbook mesh topology (Botsch et al., "Polygon Mesh Processing", A K
//! Peters 2010, ch. 1); vertex welding by spatial hashing (Teschner et al.,
//! VMV 2003); point-to-segment distance for T-junctions is plain vector
//! geometry. Our own code; nothing from any CAM vendor.
//!
//! Three seam faults are told apart, because only one of them is a hole:
//! - **Orientation**: an edge two faces share runs the same way in both. A
//!   face is wound against its neighbours; the geometry is closed. Fixed:
//!   `step_orient` decides the flips (deterministically, strongest evidence
//!   first) and they are applied here.
//! - **T-junction**: neighbouring faces divide their shared boundary into
//!   different segments, so its edges are used once each, yet every point of
//!   one side lies on the other side within the chord tolerance. Closed.
//! - **Gap**: a boundary edge with a point farther than the gap tolerance
//!   from every other face's boundary. The only fault that is refused.

use std::collections::{BTreeMap, HashMap};

use super::step_orient::{self, Vote};
use crate::geometry::{Point3, Triangle};

/// What [`close`] found and did.
#[derive(Debug, Default)]
pub(super) struct Seams {
    /// Boundary edges with a point off every other face's boundary.
    pub gaps: usize,
    /// The first such edge's ends.
    pub first_gap: Option<(Point3, Point3)>,
    /// The face that owns the most gap edges.
    pub gap_face: Option<usize>,
    /// How far the gap edges' midpoints lie from any other face's
    /// boundary, at most (over every gap edge, or an even stride through
    /// them past [`WIDTH_WORK`]).
    pub widest: f64,
    /// Edges two faces share running the same way, before the fix.
    pub reversed_edges: usize,
    /// Faces flipped to agree with their neighbours, ascending.
    pub flipped: Vec<usize>,
    /// Face pairs whose net vote the flips could not satisfy.
    pub contradictions: usize,
    /// Shared edges still running the same way in both faces after the
    /// flips, and the face pair holding the most of them.
    pub unresolved_edges: usize,
    pub unresolved_pair: Option<(usize, usize)>,
    /// Edges used once whose points all lie on another face's boundary.
    pub t_junction_edges: usize,
}

/// Distance evaluations spent on [`Seams::widest`]: each gap edge measured
/// is a scan of every boundary edge, so the work is bounded.
const WIDTH_WORK: usize = 100_000_000;

#[derive(Clone, Copy)]
struct Use {
    n: u32,
    face: [u32; 2],
    fwd: [bool; 2],
}

struct Seg {
    a: Point3,
    b: Point3,
    face: u32,
}

/// Weld `p` into `ids` within `weld` (the 27 grid cells around it).
fn weld_id(p: Point3, weld: f64, ids: &mut Vec<Point3>, grid: &mut HashMap<(i64, i64, i64), Vec<u32>>) -> u32 {
    let c = |v: f64| (v / weld).floor().clamp(-9.0e15, 9.0e15) as i64;
    let (cx, cy, cz) = (c(p.x), c(p.y), c(p.z));
    for dx in -1..=1i64 {
        for dy in -1..=1i64 {
            for dz in -1..=1i64 {
                let key = (cx.saturating_add(dx), cy.saturating_add(dy), cz.saturating_add(dz));
                for &i in grid.get(&key).map(Vec::as_slice).unwrap_or(&[]) {
                    if let Some(q) = ids.get(i as usize)
                        && (q.x - p.x).abs() <= weld
                        && (q.y - p.y).abs() <= weld
                        && (q.z - p.z).abs() <= weld
                    {
                        return i;
                    }
                }
            }
        }
    }
    let id = u32::try_from(ids.len()).unwrap_or(u32::MAX);
    ids.push(p);
    grid.entry((cx, cy, cz)).or_default().push(id);
    id
}

fn seg_dist(p: Point3, a: Point3, b: Point3) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let q = Point3::new(a.x + ab.x * t, a.y + ab.y * t, a.z + ab.z * t);
    let d = p - q;
    d.dot(d).sqrt()
}

/// Examine and fix the seams of one shell's mesh. `tris` are the shell's
/// facets, face by face: face `i` owns `ends[i-1]..ends[i]`. Vertices weld
/// within `weld`; a boundary point within `gap` of another face's boundary
/// is closed. Faces are flipped in place (see the module note).
pub(super) fn close(tris: &mut [Triangle], ends: &[usize], weld: f64, gap: f64) -> Seams {
    let owner = |i: usize| u32::try_from(ends.partition_point(|&e| e <= i)).unwrap_or(u32::MAX);
    let mut ids: Vec<Point3> = Vec::new();
    let mut grid = HashMap::new();
    let mut edges: HashMap<(u32, u32), Use> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        let k = [t.a, t.b, t.c].map(|p| weld_id(p, weld, &mut ids, &mut grid));
        if k[0] == k[1] || k[1] == k[2] || k[2] == k[0] {
            continue; // collapsed onto an edge: bounds nothing
        }
        let face = owner(i);
        for j in 0..3 {
            let (a, b) = (k[j], k[(j + 1) % 3]);
            let u = edges.entry((a.min(b), a.max(b))).or_insert(Use { n: 0, face: [0; 2], fwd: [false; 2] });
            if let (Some(f), Some(d)) = (u.face.get_mut(u.n as usize), u.fwd.get_mut(u.n as usize)) {
                (*f, *d) = (face, a < b);
            }
            u.n = u.n.saturating_add(1);
        }
    }
    drop(grid);
    // No hash order may reach the votes, the flips or the message (#1576
    // review): the votes are counts in an ordered map, and the boundary
    // segments are sorted below before anything reads their order.
    let mut seams = Seams::default();
    // Votes per face pair (lower, higher).
    let mut votes: BTreeMap<(u32, u32), Vote> = BTreeMap::new();
    let mut vote = |f: u32, g: u32, agree: bool, exact: bool| {
        let v = votes.entry((f.min(g), f.max(g))).or_default();
        let side = if exact { &mut v.exact } else { &mut v.near };
        if let Some(n) = side.get_mut(usize::from(!agree)) {
            *n = n.saturating_add(1);
        }
    };
    let mut segs: Vec<Seg> = Vec::new();
    for (&(lo, hi), u) in &edges {
        let at = |i: u32| ids.get(i as usize).copied();
        match u.n {
            1 => {
                let (a, b) = if u.fwd[0] { (lo, hi) } else { (hi, lo) };
                if let (Some(a), Some(b)) = (at(a), at(b)) {
                    segs.push(Seg { a, b, face: u.face[0] });
                }
            }
            2 if u.face[0] != u.face[1] => {
                let agree = u.fwd[0] != u.fwd[1];
                if !agree {
                    seams.reversed_edges += 1;
                }
                vote(u.face[0], u.face[1], agree, true);
            }
            _ => {} // inside one face, or non-manifold: neither is a hole
        }
    }
    drop(edges);
    // By face, then by position: the first gap and the sampled widths are
    // the same whatever order the facets came in.
    let key = |s: &Seg| [s.a.x, s.a.y, s.a.z, s.b.x, s.b.y, s.b.z];
    segs.sort_by(|x, y| x.face.cmp(&y.face).then_with(|| {
        key(x).iter().zip(key(y)).map(|(a, b)| a.total_cmp(&b)).find(|o| o.is_ne()).unwrap_or(std::cmp::Ordering::Equal)
    }));
    cover(&segs, gap, &mut seams, &mut |f, g, agree| vote(f, g, agree, false));
    let o = step_orient::orient(tris, ends, &votes);
    for &f in &o.flipped {
        for t in tris.get_mut(step_orient::range(ends, f)).unwrap_or(&mut []) {
            *t = Triangle::new(t.a, t.c, t.b);
        }
    }
    (seams.flipped, seams.contradictions) = (o.flipped, o.contradictions);
    (seams.unresolved_edges, seams.unresolved_pair) = (o.unresolved_edges, o.unresolved_pair);
    seams
}

/// Check every boundary edge against the other faces' boundary edges.
fn cover(segs: &[Seg], gap: f64, seams: &mut Seams, vote: &mut impl FnMut(u32, u32, bool)) {
    if segs.is_empty() {
        return;
    }
    let len = |s: &Seg| (s.b - s.a).dot(s.b - s.a).sqrt();
    let mean = segs.iter().map(len).sum::<f64>() / segs.len() as f64;
    let cell = (2.0 * mean).max(gap).max(1e-12);
    let c = |v: f64| (v / cell).floor().clamp(-9.0e15, 9.0e15) as i64;
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    let mut long: Vec<usize> = Vec::new();
    for (i, s) in segs.iter().enumerate() {
        let lo = [s.a.x.min(s.b.x) - gap, s.a.y.min(s.b.y) - gap, s.a.z.min(s.b.z) - gap].map(c);
        let hi = [s.a.x.max(s.b.x) + gap, s.a.y.max(s.b.y) + gap, s.a.z.max(s.b.z) + gap].map(c);
        let span = |k: usize| hi[k].saturating_sub(lo[k]).saturating_add(1);
        if span(0).saturating_mul(span(1)).saturating_mul(span(2)) > 512 {
            long.push(i);
            continue;
        }
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    grid.entry((x, y, z)).or_default().push(i);
                }
            }
        }
    }
    let mut gaps: Vec<&Seg> = Vec::new();
    for s in segs {
        let mid = Point3::new((s.a.x + s.b.x) * 0.5, (s.a.y + s.b.y) * 0.5, (s.a.z + s.b.z) * 0.5);
        let mut closed = true;
        for (k, p) in [s.a, mid, s.b].into_iter().enumerate() {
            let near = grid.get(&(c(p.x), c(p.y), c(p.z))).map(Vec::as_slice).unwrap_or(&[]);
            let best = near
                .iter()
                .chain(&long)
                .filter_map(|&j| segs.get(j))
                .filter(|o| o.face != s.face)
                .map(|o| (seg_dist(p, o.a, o.b), o))
                .min_by(|x, y| x.0.total_cmp(&y.0).then(x.1.face.cmp(&y.1.face)));
            match best {
                Some((d, o)) if d <= gap => {
                    if k == 1 {
                        // The side across the seam: its direction votes.
                        let (u, v) = (s.b - s.a, o.b - o.a);
                        let cos = u.dot(v) / (u.dot(u) * v.dot(v)).sqrt();
                        if cos.abs() > 0.9 {
                            vote(s.face, o.face, cos < 0.0);
                        }
                    }
                }
                _ => closed = false,
            }
        }
        if closed {
            seams.t_junction_edges += 1;
        } else {
            seams.gaps += 1;
            seams.first_gap.get_or_insert((s.a, s.b));
            gaps.push(s);
        }
    }
    let mut by_face: HashMap<u32, usize> = HashMap::new();
    for s in &gaps {
        *by_face.entry(s.face).or_default() += 1;
    }
    seams.gap_face = by_face.into_iter().max_by_key(|&(f, n)| (n, std::cmp::Reverse(f))).map(|(f, _)| f as usize);
    // Every gap edge when the scan is small (a few thousand boundary edges
    // in a real shell), else an even stride through them: the same edges on
    // every run.
    let work = gaps.len().saturating_mul(segs.len());
    let stride = work.div_ceil(WIDTH_WORK).max(1);
    for s in gaps.iter().step_by(stride) {
        let mid = Point3::new((s.a.x + s.b.x) * 0.5, (s.a.y + s.b.y) * 0.5, (s.a.z + s.b.z) * 0.5);
        let near = segs.iter().filter(|o| o.face != s.face).map(|o| seg_dist(mid, o.a, o.b)).fold(f64::INFINITY, f64::min);
        if near.is_finite() {
            seams.widest = seams.widest.max(near);
        }
    }
}

#[cfg(test)]
#[path = "step_seams_tests.rs"]
mod tests;
