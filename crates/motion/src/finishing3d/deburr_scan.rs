//! The exposed-edge scan the 3D Deburr and the multi-axis Deburr share
//! (#1894): weld the facets, pair every facet edge, and keep the rims
//! and the convex creases in the weld's own sorted order.
//!
//! Provenance: pencil's concavity gate read the other way (see
//! `deburr`'s "The loci are pencil's mirror"); standard mesh edge
//! pairing. Our own design notes; no vendor material consulted.

use std::collections::HashMap;

use crate::geometry::{Point3, Triangle};

use super::crease::{scale3, sub3, third, vkey};

/// One exposed edge, `e0` to `e1`, as the scan meets it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Exposed {
    /// A facet edge of an open mesh: one face, nothing beside it.
    /// `inside` is the face's third vertex, `facet` its index.
    Rim { inside: Point3, facet: usize },
    /// A convex crease. `qa` stands inside face B, `qb` inside face A;
    /// `bend` is the dot of the two unit face normals.
    Crease { qa: Point3, qb: Point3, bend: f64 },
}

/// The scan's result: the exposed edges in sorted weld order, and the
/// counts of the shared edges it had to skip.
pub(crate) struct EdgeScan {
    pub edges: Vec<(Point3, Point3, Exposed)>,
    /// Shared edges with the same direction in both faces.
    pub flips: usize,
    /// Edges that touch more than two facets.
    pub manifold: usize,
}

/// The edge tally: an unordered vertex pair to the witnesses standing
/// on it, each a (facet, from, to) - the directed half so a run can
/// walk the edge in its own face's order.
type EdgeBook = HashMap<(usize, usize), Vec<(usize, usize, usize)>>;

/// Scan `facets` (welded at scale `s`) for rims and convex creases.
/// Flat meetings, concave creases and degenerate facets are no edge.
pub(crate) fn scan(facets: &[Triangle], s: f64) -> EdgeScan {
    // Weld vertices; bucket directed edges by the unordered pair so
    // a rim and both faces of one crease meet in one entry (pencil's
    // index, the same walk).
    let mut verts: Vec<Point3> = Vec::new();
    let mut ids: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut vix: Vec<[usize; 3]> = Vec::with_capacity(facets.len());
    for f in facets {
        vix.push([
            vid(f.a, s, &mut verts, &mut ids),
            vid(f.b, s, &mut verts, &mut ids),
            vid(f.c, s, &mut verts, &mut ids),
        ]);
    }
    let mut book: EdgeBook = HashMap::new();
    for (fi, tri) in vix.iter().enumerate() {
        for (from, to) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            if from == to {
                continue; // a degenerate triangle's own repeat is no edge
            }
            let key = if from < to { (from, to) } else { (to, from) };
            book.entry(key).or_default().push((fi, from, to));
        }
    }
    let mut keys: Vec<(usize, usize)> = book.keys().copied().collect();
    keys.sort_unstable();

    let mut out = EdgeScan { edges: Vec::new(), flips: 0, manifold: 0 };
    for &(lo, hi) in &keys {
        let list = &book[&(lo, hi)];
        let (e0, e1) = (verts[lo], verts[hi]);
        if list.len() == 1 {
            let facet = list[0].0;
            out.edges.push((e0, e1, Exposed::Rim { inside: third(facets[facet], e0, e1), facet }));
            continue;
        }
        if list.len() > 2 {
            out.manifold += 1;
            continue; // a weld gone wrong is no edge to break
        }
        let (fi, a0, a1) = list[0];
        let (fj, b0, b1) = list[1];
        if (a0, a1) == (b0, b1) {
            out.flips += 1; // same direction in both faces: one winding is flipped
            continue;
        }
        if fi == fj {
            continue; // the same face twice is a degenerate triangle, not a crease
        }
        let (na, nb) = (facets[fi].normal(), facets[fj].normal());
        if !(na.dot(na) > 0.0 && nb.dot(nb) > 0.0) {
            continue; // a degenerate facet owns no corner
        }
        let (na, nb) = (
            scale3(na, 1.0 / na.dot(na).sqrt()),
            scale3(nb, 1.0 / nb.dot(nb).sqrt()),
        );
        let bend = na.dot(nb).clamp(-1.0, 1.0);
        if bend > 1.0 - 1e-12 {
            continue; // flat meeting: tangent faces show no edge to break
        }
        // The mirrored gate: pencil keeps corners whose opposite
        // vertices stand above the other face; what bends the OTHER
        // way (or stands under on one side - a knife edge) is convex
        // enough to burr.
        let qa = third(facets[fj], e0, e1); // inside face B
        let qb = third(facets[fi], e0, e1); // inside face A
        if sub3(qa, e0).dot(na) > 0.0 && sub3(qb, e0).dot(nb) > 0.0 {
            continue; // concave: the pencil pair's corner, not a burr
        }
        out.edges.push((e0, e1, Exposed::Crease { qa, qb, bend }));
    }
    out
}

fn vid(pt: Point3, s: f64, verts: &mut Vec<Point3>, ids: &mut HashMap<(i64, i64, i64), usize>) -> usize {
    let k = vkey(pt, s);
    if let Some(&i) = ids.get(&k) {
        return i;
    }
    let i = verts.len();
    verts.push(pt);
    ids.insert(k, i);
    i
}
