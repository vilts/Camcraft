//! Project generalization (M2.5 D3): drape ANY closed curve the model
//! itself carries, not just the XY silhouette.
//!
//! Provenance: projecting a part's boundary/trim curves along Z onto
//! the machining surface is public CAM practice ("projected
//! finishing", described in machine-tool handbooks at the level of
//! behavior). The construction here - weld-scaled open-edge chaining
//! and the column-grounded march of the resulting loops - is our own,
//! reusing this crate's crease-pair weld/chain and the shadow
//! families' march laws verbatim: two truths, one code. Nothing was
//! read from any vendor's code or output.
//!
//! ## The law
//!
//! M2.2e's Project marched one curve: the center-shadow's boundary -
//! the silhouette, the shallow facets translated by `r·m_xy`. A part
//! whose surface is an open shell (a trim, a sheet, a face with a
//! hole in it) carries its own closed boundary curves in 3D: the
//! edges where facets end. Those are the model's TRUE rim, and the
//! silhouette is only its shadow, displaced from the rim by the
//! tilt term. D3 drapes both: the silhouette first (unchanged law,
//! unchanged bytes), then every closed boundary loop of the mesh -
//! its XY kept, its Z grounded by the column law (`plunge_support`),
//! marched by the same `march_loop` the rim rides. A hole's rim in a
//! tilted plate is cut exactly where the plate's edge is, not where
//! its shadow falls.
//!
//! A CLOSED MANIFOLD has no open edge, so it gains no loop: for
//! every solid model in the goldens, project answers exactly as it
//! did before this file existed, byte for byte. Boundary segments
//! that chain into open runs (a crack, a T-junction) are not closed
//! curves: they are refused by count - reported in a note, never
//! silently cut as if closed.

use std::collections::HashMap;

use crate::cl::PathBuilder;
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

use super::crease::{chain, vkey, vkey_scale, Seg};
use super::march::emit_runs;
use super::params::{Finish3DParams, refinement_floor, scallop_stepdown};
use super::shadow::march_loop;
use super::grid::SupSet;
use super::support::plunge_support;

/// The model's boundary, welded and chained: closed 3D loops (each
/// with its duplicate closing vertex removed) and the count of open
/// runs - cracks and T-junctions - that are curves but not closed
/// ones. Deterministic: facets are walked in build order, the chain
/// is crease.rs' own (index-sorted candidates, smoothest leg wins,
/// closing beats ties), and the loops are then ordered by |XY area|
/// descending with the entry-point tiebreak - the shadow_pass order
/// law: a fact about the model, not about HashMap memory.
/// A facet edge as its two welded endpoint keys, low key first -
/// the identity an edge has once floats are quantized away.
type WeldedEdge = ((i64, i64, i64), (i64, i64, i64));

pub(super) fn boundary_loops(facets: &[Triangle]) -> (Vec<Vec<Point2>>, usize) {
    let scale = vkey_scale(facets);
    let key = |q: Point3| vkey(q, scale);
    // Weld: distinct vertex keys, and the facet-edge face count.
    let mut edge_faces: HashMap<WeldedEdge, u32> = HashMap::new();
    for f in facets {
        let k = [key(f.a), key(f.b), key(f.c)];
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            let pair = if k[a] <= k[b] { (k[a], k[b]) } else { (k[b], k[a]) };
            *edge_faces.entry(pair).or_default() += 1;
        }
    }
    // The boundary segments: facet edges whose key-pair lives on one
    // face only. Both orientations are collected as they stand, so a
    // traversed loop keeps a walk direction the march can climb-gate.
    let mut segs: Vec<Seg> = Vec::new();
    for f in facets {
        let (ka, kb, kc) = (key(f.a), key(f.b), key(f.c));
        for (p, q, k0, k1) in [
            (f.a, f.b, ka, kb),
            (f.b, f.c, kb, kc),
            (f.c, f.a, kc, ka),
        ] {
            let pair = if k0 <= k1 { (k0, k1) } else { (k1, k0) };
            if edge_faces.get(&pair).copied() == Some(1) {
                segs.push(Seg { c0: p, c1: q, k0, k1 });
            }
        }
    }
    let mut loops: Vec<Vec<Point2>> = Vec::new();
    let mut cracks = 0usize;
    for pts in chain(&segs) {
        let n = pts.len();
        if n >= 4 {
            let d = pts[n - 1] - pts[0];
            let dd = d.dot(d);
            if dd.is_finite() && dd <= (4.0 * scale) * (4.0 * scale) {
                // Closed: drop the duplicated seam vertex - the
                // march's modular indexing closes the polygon itself.
                loops.push(pts[..n - 1].iter().map(|q| Point2::new(q.x, q.y)).collect());
                continue;
            }
        }
        cracks += 1;
    }
    loops.sort_by(|a, b| {
        xy_area(b)
            .abs()
            .total_cmp(&xy_area(a).abs())
            .then_with(|| a[0].x.total_cmp(&b[0].x).then_with(|| a[0].y.total_cmp(&b[0].y)))
    });
    (loops, cracks)
}

/// Shoelace twice (a stable ordering key; sign = walk orientation).
fn xy_area(l: &[Point2]) -> f64 {
    let mut s = 0.0f64;
    for i in 0..l.len() {
        let (a, b) = (l[i], l[(i + 1) % l.len()]);
        s += a.x * b.y - b.x * a.y;
    }
    s
}

/// The drape pass of the generalized Project: after the silhouette
/// rim (shadow_pass' own work, unchanged), every closed boundary
/// loop of the mesh is marched - XY kept, Z grounded by the same
/// column law every march family pays. Answers whether it emitted
/// any run (the caller folds that into the ring close's `emitted`).
/// `continuing` says whether the silhouette already parked a tool in
/// the program - the first drape entry hops from there when it did,
/// and arrives by its own rapid when the rim never came. The
/// builder's notes carry the honest accounting: cracked segments
/// refused by count, trimmed ground samples reported the shadow
/// families' way.
pub(super) fn drape_pass(
    b: &mut PathBuilder,
    p: &Finish3DParams,
    r: f64,
    sups: &SupSet,
    facets: &[Triangle],
    continuing: bool,
) -> bool {
    let (loops, cracks) = boundary_loops(facets);
    if cracks > 0 {
        b.note(format!(
            "project: {cracks} boundary run(s) stand open (a crack, a T-junction) - not closed curves, not draped"
        ));
    }
    if loops.is_empty() {
        return false; // the honest majority: a closed shell adds nothing
    }
    let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
    let trimmed = std::cell::Cell::new(0usize);
    let sup = |q: Point2| {
        let (c, cut) = plunge_support(q.x, q.y, r, p.bottom_z, p.clearance_z + r, sups);
        if cut {
            trimmed.set(trimmed.get() + 1);
        }
        c
    };
    let mut runs: Vec<Vec<(Point2, f64)>> = Vec::new();
    for l in &loops {
        let poly: Vec<Point2> = if p.climb {
            l.clone()
        } else {
            l.iter().rev().copied().collect()
        };
        runs.extend(march_loop(
            &poly,
            &sup,
            delta0,
            p.tolerance,
            p.tolerance * 0.25,
            refinement_floor(delta0, p.min_step),
        ));
    }
    if trimmed.get() > 0 {
        b.note(format!(
            "project: {} drape samples refused - their plunge column from clearance crosses the solid; what hides there stands as rest, reported not cut",
            trimmed.get()
        ));
    }
    if runs.is_empty() {
        return false; // every loop projected to a point or a line: vertical walls
    }
    emit_runs(b, p, r, sups, &runs, continuing);
    true
}
