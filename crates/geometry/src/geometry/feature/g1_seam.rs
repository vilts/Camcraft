//! The G1 seam rule of [`chains`] (#2209): one smooth crease is one edge.
//!
//! Provenance: our own design notes; "a B-rep edge is the maximal
//! tangent-continuous curve between two faces" is the textbook definition,
//! approximated here on a mesh. No vendor material consulted.
//!
//! The face rule of [`chains`] joins two feature edges only when they
//! bound one planar face in common. Where a crease runs between two curved
//! surfaces (a conical chamfer above a cylindrical wall), each chord of the
//! crease has a different planar strip on both sides, so no two chords
//! share a face and the crease fell apart into chord-long pieces. This rule
//! joins two edges at a vertex when all of these hold:
//!
//! - both edges separate the SAME two welded patches
//!   ([`Features::facet_patch`]), or both are the rim of one open sheet
//!   patch, so a seam inside one patch never joins
//!   and the edges of two unrelated surfaces never join;
//! - the vertex has exactly these two real feature edges (a boundary, a
//!   non-manifold or degenerate edge, or a crease between two patches each
//!   count), so a vertex where three or more feature edges meet stays a
//!   corner;
//! - the directed turn is at most [`CHAIN_TANGENT_DEG`], so a real corner
//!   of the crease splits.
//!
//! The rule only adds joins: it moves no point and drops no edge, so the
//! joined chain keeps every original chord in order.

use super::*;

/// The two distinct welded patches a two-facet crease separates (sorted),
/// `(patch, u32::MAX)` for the rim of an open sheet (one facet), or `None`
/// for any other edge.
fn patch_pair(features: &Features, e: &TopEdge) -> Option<(u32, u32)> {
    if e.kind == EdgeKind::Boundary && e.n_facets == 1 {
        let p = *features.facet_patch.get(e.facets[0] as usize)?;
        return (p != u32::MAX).then_some((p, u32::MAX));
    }
    if e.n_facets != 2 || e.kind != EdgeKind::Crease {
        return None;
    }
    let p0 = *features.facet_patch.get(e.facets[0] as usize)?;
    let p1 = *features.facet_patch.get(e.facets[1] as usize)?;
    (p0 != p1).then_some(if p0 < p1 { (p0, p1) } else { (p1, p0) })
}

/// An edge the pick shows: everything but a crease whose two facets lie in
/// one welded patch (a tessellation seam). Conservative: a degenerate edge
/// counts, so it can only block a join.
fn is_real(features: &Features, e: &TopEdge) -> bool {
    e.kind != EdgeKind::Crease || patch_pair(features, e).is_some()
}

/// Edge pairs `(vertex, a, b)` that the G1 seam rule joins, from the
/// per-vertex incidence list [`chains`] builds. Total on any input: a
/// dangling index or an unweldable position refuses the join, never panics.
pub(super) fn seam_joins(
    features: &Features,
    incident: &HashMap<u32, Vec<u32>>,
    lattice: &[Pos],
) -> Vec<(u32, u32, u32)> {
    let cos_max = CHAIN_TANGENT_DEG.to_radians().cos();
    let mut out = Vec::new();
    for (v, list) in incident {
        let mut real = list.iter().copied().filter(|&i| features.edges.get(i as usize).is_some_and(|e| is_real(features, e)));
        let (Some(ia), Some(ib), None) = (real.next(), real.next(), real.next()) else {
            continue;
        };
        let (Some(ea), Some(eb)) = (features.edges.get(ia as usize), features.edges.get(ib as usize)) else {
            continue;
        };
        match (patch_pair(features, ea), patch_pair(features, eb)) {
            (Some(pa), Some(pb)) if pa == pb => {}
            _ => continue,
        }
        let pos = |i: u32| lattice.get(i as usize).copied();
        if let (Some(q), Some(pa), Some(pb)) = (pos(*v), pos(other_end(ea, *v)), pos(other_end(eb, *v)))
            && tangent_at(q, pa, pb, cos_max)
        {
            out.push((*v, ia, ib));
        }
    }
    out
}
