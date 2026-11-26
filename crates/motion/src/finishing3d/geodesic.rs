//! Geodesic finishing (M2.5 C): rings of equal GEODESIC distance
//! from the model's open boundary - contours that run WITH the
//! surface instead of level with the table, placed by arc-length
//! along the mesh. The catalog's story for the family is continuity:
//! where the surface lets one ball rest the whole way, a single loop
//! climbs out of the floor, over the wall and around the rim with no
//! Z break; where it does not - a sharp corner the lifted tangent
//! cannot clear - the loop opens into runs, and the opening is
//! counted by note, never hidden in a smooth-looking picture.
//!
//! Provenance: geodesic-distance isoline toolpaths are published
//! finishing practice (contours of the boundary distance field,
//! marched on the triangulated surface); the distance here is the
//! graph geodesic - multi-source Dijkstra on the welded edge graph,
//! Mitchell's continuous variant minus its star unfolding: the error
//! is the mesh's own discretization, the same currency every
//! tessellation law in this module already spends, and nothing
//! rests on it (placement is the FIELD's answer, safety is the
//! envelope's). Written from that geometry and our own design notes.
//! No vendor material consulted.
//!
//! ## The laws here
//!
//! - **The field comes from a real boundary.** Edges that exactly
//!   one facet runs are OPEN; their vertices seed distance zero. A
//!   model with no open edge is refused by name - a closed shell has
//!   nowhere for the rings to start, and inventing a seam would be
//!   dressing a guess as a datum. Vertices the boundary cannot
//!   reach (a closed island riding along with the open mesh) are
//!   refused their contours and counted by note.
//! - **Distance along the mesh, never across it.** Dijkstra on
//!   welded edges; the iso value is interpolated linearly on each
//!   edge from the canonical (lower, higher) vertex pair, so both
//!   triangles meeting at an edge compute the SAME point bit for
//!   bit and the crease chain welds their segments exactly.
//! - **A contour point is a resting center, or it is nothing.** The
//!   normal is the lerp of the edge's vertex normals; the center is
//!   the surface point lifted by `r` along it. It must keep the
//!   programmed window and stand clear of every facet to `r - tol`
//!   (grazing contact included; the foot is on real geometry by
//!   construction). The lifted tangent is honest only where the
//!   surface is smooth enough for it: past a sharp crease it bites
//!   and the point is refused - the break is the mesh telling the
//!   truth about a corner the ball cannot roll.
//! - **Every entry is certified, every leg is certified.** A run
//!   starts by a vertical plunge down the tangent column - and the
//!   column must be clear from the clearance plane (the verdict
//!   lane's law, same currency as the rings: `r - tol`). A feed
//!   leg must keep the swept ball clear by `leg_clears`; where it
//!   does not, the loop opens and each piece gets its own certified
//!   entry. Moves speak TIPS (center lowered by `r`).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::crease::{add3, chain, cmp_pt, scale3, sub3, vkey, vkey_scale, Seg};
use super::params::{scallop_stepover, validate, Finish3DParams};
use super::raster::placement_guard;
use super::rings::leg_clears;
use super::grid::SupSet;
use super::support::{build_sups, column_clear, dist2_pt_tri};

/// The boundary-seeded geodesic distance field over the welded
/// vertices: zero on open-edge vertices, Dijkstra everywhere else.
/// `None` says the mesh has no open edge at all; the second value
/// counts vertices the boundary cannot reach (a closed island riding
/// along with the open one) - the caller says so by note.
fn distance_field(verts: &[Point3], tris: &[[usize; 3]]) -> Option<(Vec<f64>, usize)> {
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); verts.len()];
    let mut open: Vec<bool> = vec![false; verts.len()];
    let mut edge_faces: HashMap<(usize, usize), usize> = HashMap::new();
    for t in tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *edge_faces.entry(if a < b { (a, b) } else { (b, a) }).or_default() += 1;
        }
    }
    for (&(a, b), &n) in &edge_faces {
        if n == 1 {
            open[a] = true;
            open[b] = true;
        }
    }
    if !open.iter().any(|&o| o) {
        return None;
    }
    for t in tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let e = sub3(verts[b], verts[a]);
            let len = e.dot(e).sqrt();
            adj[a].push((b, len));
            adj[b].push((a, len));
        }
    }
    // The heap orders by the bits of the (non-negative, finite)
    // distance: for that domain the bit order IS the total order, so
    // no partial_cmp unwrapping lives here.
    let mut d = vec![f64::INFINITY; verts.len()];
    let mut heap: BinaryHeap<(Reverse<u64>, usize)> = BinaryHeap::new();
    for i in 0..verts.len() {
        if open[i] {
            d[i] = 0.0;
            heap.push((Reverse(0u64), i)); // the bits of +0.0f64
        }
    }
    while let Some((Reverse(bits), u)) = heap.pop() {
        if bits != d[u].to_bits() {
            continue; // a stale, longer entry for u
        }
        for &(v, w) in &adj[u] {
            let nd = d[u] + w;
            if nd < d[v] {
                d[v] = nd;
                heap.push((Reverse(nd.to_bits()), v));
            }
        }
    }
    let stranded = d.iter().filter(|x| !x.is_finite()).count();
    Some((d, stranded))
}

/// Angle-of-fan-weighted vertex normals: the sum of the incident
/// facets' (already area-weighted) normals, unitized. A zero-length
/// sum - a pinched or all-degenerate fan - answers `None`, and its
/// contour points are refused: that is the honest reading of "no
/// normal here".
fn vertex_normals(facets: &[Triangle], tris: &[[usize; 3]], nverts: usize) -> Vec<Option<Point3>> {
    let mut n = vec![Point3::ZERO; nverts];
    for (fi, f) in facets.iter().enumerate() {
        let m = f.normal();
        if !m.is_finite() || m.dot(m) <= 0.0 {
            continue;
        }
        for vi in tris[fi] {
            n[vi] = add3(n[vi], m);
        }
    }
    n.into_iter()
        .map(|m| {
            let l = m.dot(m).sqrt();
            if l > 1e-12 && m.is_finite() {
                Some(scale3(m, 1.0 / l))
            } else {
                None
            }
        })
        .collect()
}

/// Is this lifted point a resting ball center? Tip inside the
/// programmed window, center at or under the clearance plane, and no
/// facet nearer than `r - tol` (the tessellation currency: a
/// tangency reads `r`, grazing bite up to `tol` is the same debt a
/// certified ring start carries). The foot is on real geometry by
/// construction - the point came off a real facet edge.
fn resting(c: Point3, p: &Finish3DParams, r: f64, sups: &SupSet) -> bool {
    if !c.is_finite() {
        return false;
    }
    let tip = c.z - r;
    if tip < p.bottom_z - p.tolerance || tip > p.top_z + p.tolerance || c.z > p.clearance_z + r {
        return false;
    }
    let lim = r - p.tolerance;
    if !lim.is_finite() || lim <= 0.0 {
        return false; // a non-positive currency answers nothing
    }
    for s in sups {
        if c.x < s.xmin || c.x > s.xmax || c.y < s.ymin || c.y > s.ymax {
            continue;
        }
        if dist2_pt_tri(c, &s.tri) < lim * lim {
            return false;
        }
    }
    true
}

/// Geodesic finishing proper: the boundary distance field, its iso
/// contours marched triangle by triangle, and the resting centers
/// along them ridden as rings. See the module header for the laws.
pub fn geodesic(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let s = vkey_scale(facets);

    // Weld vertices; index faces by triangle.
    let mut verts: Vec<Point3> = Vec::new();
    let mut ids: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut tris: Vec<[usize; 3]> = Vec::with_capacity(facets.len());
    for f in facets {
        tris.push([
            vid(f.a, s, &mut verts, &mut ids),
            vid(f.b, s, &mut verts, &mut ids),
            vid(f.c, s, &mut verts, &mut ids),
        ]);
    }
    fn vid(
        pt: Point3,
        s: f64,
        verts: &mut Vec<Point3>,
        ids: &mut HashMap<(i64, i64, i64), usize>,
    ) -> usize {
        let k = vkey(pt, s);
        if let Some(&i) = ids.get(&k) {
            return i;
        }
        let i = verts.len();
        verts.push(pt);
        ids.insert(k, i);
        i
    }

    let (d, stranded) = match distance_field(&verts, &tris) {
        None => {
            return Err(GenError::Invalid(
                "geodesic needs an open boundary to march from - this model is a closed shell"
                    .into(),
            ))
        }
        Some(x) => x,
    };
    let step = p.stepover.unwrap_or_else(|| scallop_stepover(r, p.tolerance));
    let dmax = d
        .iter()
        .filter(|x| x.is_finite())
        .copied()
        .fold(0.0f64, f64::max);
    const MAX_RINGS: f64 = 8192.0;
    let nlevels = (dmax / step).floor().max(0.0);
    placement_guard(step, nlevels, "geodesic", MAX_RINGS)?;
    let nlevels = nlevels as usize;
    let normals = vertex_normals(facets, &tris, verts.len());

    // March every triangle once, bucketing its contour segments by
    // level index. Coincident segments (the degenerate case of a
    // contour running exactly along a shared iso-edge, both faces
    // claiming it) are deduped by their welded key pair.
    type WeldedPair = ((i64, i64, i64), (i64, i64, i64));
    let mut buckets: Vec<Vec<Seg>> = (0..nlevels).map(|_| Vec::new()).collect();
    let mut seen: Vec<HashSet<WeldedPair>> = (0..nlevels).map(|_| HashSet::new()).collect();
    let mut refused_pts = 0usize;
    for t in &tris {
        let mut hits: Vec<(i64, Point3)> = Vec::new();
        for &(a, b) in &[(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (da, db) = (d[a], d[b]);
            if !da.is_finite() || !db.is_finite() || da == db {
                continue; // unreachable island, or an edge lying in one level
            }
            let (lo, hi) = if da < db { (a, b) } else { (b, a) };
            let (dl, dh) = (d[lo], d[hi]);
            let k0 = (dl / step).floor() as i64 + 1;
            let k1 = (dh / step).floor() as i64;
            for k in k0.max(1)..=k1 {
                if k as usize > nlevels {
                    continue;
                }
                let (Some(nl), Some(nh)) = (normals[lo], normals[hi]) else {
                    continue; // a vertex with no normal carries no contour
                };
                let w = (k as f64 * step - dl) / (dh - dl);
                let q = add3(verts[lo], scale3(sub3(verts[hi], verts[lo]), w));
                let nraw = add3(scale3(nl, 1.0 - w), scale3(nh, w));
                let l2 = nraw.dot(nraw);
                if !nraw.is_finite() || l2 <= 1e-12 {
                    continue;
                }
                let c = add3(q, scale3(scale3(nraw, 1.0 / l2.sqrt()), r));
                if !resting(c, p, r, &sups) {
                    refused_pts += 1;
                    continue; // a bite, a window break: this point is nothing
                }
                hits.push((k, c));
            }
        }
        hits.sort_by_key(|h| h.0);
        let mut i = 0;
        while i + 1 < hits.len() {
            if hits[i].0 == hits[i + 1].0 {
                let (k, (c0, c1)) = (hits[i].0, (hits[i].1, hits[i + 1].1));
                let (mut k0, mut k1) = (vkey(c0, s), vkey(c1, s));
                if k1 < k0 {
                    std::mem::swap(&mut k0, &mut k1);
                }
                if k0 != k1 && seen[k as usize - 1].insert((k0, k1)) {
                    buckets[k as usize - 1].push(Seg { c0, c1, k0, k1 });
                }
                i += 2;
            } else {
                i += 1; // a lone crossing: a contour touching this
                        // triangle's vertex - its far face carries the seg
            }
        }
    }

    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut emitted = 0usize;
    let mut refused_runs = 0usize;
    let mut breaks = 0usize;
    let mut capped = false;
    'levels: for segs in &buckets {
        let mut runs = chain(segs);
        runs.retain(|pts| pts.len() >= 2);
        runs.sort_by(|a, b| cmp_pt(a[0], b[0]));
        for run in &runs {
            // Split where a straight leg would bite: no cut move
            // spans geometry the swept ball cannot cross.
            let mut pieces: Vec<Vec<Point3>> = Vec::new();
            let mut cur: Vec<Point3> = vec![run[0]];
            for &pt in &run[1..] {
                let last = cur.last().copied().unwrap_or(pt);
                if leg_clears(facets, last, pt, r, p.tolerance) {
                    cur.push(pt);
                } else {
                    breaks += 1;
                    if cur.len() >= 2 {
                        pieces.push(std::mem::replace(&mut cur, vec![pt]));
                    } else {
                        cur = vec![pt];
                    }
                }
            }
            if cur.len() >= 2 {
                pieces.push(cur);
            }
            for piece in pieces {
                let c0 = piece[0];
                // The verdict lane's law: the tangent STOP is not
                // the descent. Certify the column from the
                // clearance plane down, at the tessellated currency.
                if !column_clear(c0.x, c0.y, c0.z, p.clearance_z + r, r - p.tolerance, &sups) {
                    refused_runs += 1;
                    continue;
                }
                b.push(MoveKind::Rapid, Point3::new(c0.x, c0.y, p.clearance_z), None);
                b.push(
                    MoveKind::Plunge,
                    Point3::new(c0.x, c0.y, c0.z - r),
                    Some(p.plunge_feed),
                );
                for &c in &piece[1..] {
                    b.push(MoveKind::Feed, Point3::new(c.x, c.y, c.z - r), Some(p.feed));
                }
                emitted += 1;
                if emitted >= MAX_RINGS as usize * 64 {
                    capped = true; // a hostile mesh cannot outrun the cap
                    break 'levels;
                }
            }
        }
    }
    if stranded > 0 {
        b.note(format!(
            "geodesic: {stranded} mesh vertices the open boundary cannot reach - that closed island stands as rest, reported not cut"
        ));
    }
    if refused_pts > 0 {
        b.note(format!(
            "geodesic: {refused_pts} contour points refused where the lifted tangent bites or leaves the programmed window - the loops opened there, counted not hidden"
        ));
    }
    if breaks > 0 {
        b.note(format!(
            "geodesic: {breaks} legs crossed a bite to the swept ball - loops opened into runs, each entered by its own certified plunge"
        ));
    }
    if refused_runs > 0 {
        b.note(format!(
            "geodesic: {refused_runs} run entries refused - their plunge column from clearance crosses the solid; what hides there stands as rest, reported not cut"
        ));
    }
    if capped {
        b.note(format!(
            "geodesic: the run count reached the emission cap {}; the rest of the field was not marched - widen the stepover",
            MAX_RINGS as usize * 64
        ));
    }
    if emitted == 0 {
        return Err(GenError::Empty(format!(
            "geodesic found no runnable contour inside the programmed window (stepover {:.4} over a field reaching {:.4})",
            step, dmax
        )));
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid(
            "geodesic path left the finite horizon".into(),
        ));
    }
    Ok(path)
}
