//! Provenance: original floor and mouth classification by segment-triangle intersections, for #2601.

use super::cylinder::{coord, hypot, ray_hit, vkey};
use super::recognize::{FIT_TOL, Found, set};
use crate::geometry::{Point3, Triangle};
use std::collections::{HashMap, HashSet};

/// A segment from inside the bore to just beyond each wall end can
/// meet a floor joined to that end's rim, but never a distant face or
/// an unrelated surface crossing the bore.
/// The transverse offset avoids floor fan seams. Wall-only legacy meshes
/// have no floor evidence, so they keep the existing through verdict.
pub(super) fn ends(
    tris: &[Triangle],
    (along, u, v): (usize, usize, usize),
    f: Found,
) -> (bool, bool) {
    let mut inside = Point3::new(0.0, 0.0, 0.0);
    set(&mut inside, along, (f.smin + f.smax) * 0.5);
    set(&mut inside, u, f.cu + f.rad * 0.4);
    set(&mut inside, v, f.cv + f.rad * 0.3);
    let capped = |end: f64, sign: f64| {
        let on_rim = |p: Point3| {
            (coord(p, along) - end).abs() <= 1e-6
                && (hypot(coord(p, u) - f.cu, coord(p, v) - f.cv) - f.rad).abs() <= FIT_TOL
        };
        let mut beyond = inside;
        set(&mut beyond, along, end + sign * 1e-6);
        let mut dir = Point3::new(0.0, 0.0, 0.0);
        set(&mut dir, along, sign);
        // Only floor patches inside this end's disc participate. Exterior
        // annuli and surfaces crossing the bore cannot connect a false cap.
        let floor_vertex = |p: Point3| {
            (coord(p, along) - end).abs() <= 1e-6
                && hypot(coord(p, u) - f.cu, coord(p, v) - f.cv) <= f.rad + FIT_TOL
        };
        let edges = |t: &Triangle| {
            [(t.a, t.b), (t.b, t.c), (t.c, t.a)].map(|(a, b)| {
                let (a, b) = (vkey(&a), vkey(&b));
                if a <= b { (a, b) } else { (b, a) }
            })
        };
        let mut by_edge: HashMap<_, Vec<usize>> = HashMap::new();
        let mut pending = Vec::new();
        for (i, t) in tris.iter().enumerate() {
            let vertices = [t.a, t.b, t.c];
            if !vertices.into_iter().all(floor_vertex) {
                continue;
            }
            for edge in edges(t) {
                by_edge.entry(edge).or_default().push(i);
            }
            if vertices.into_iter().filter(|&p| on_rim(p)).count() >= 2 {
                pending.push(i);
            }
        }
        // The hit facet can be wholly inside a subdivided floor. It must
        // belong to a patch connected by full edges to the bore rim.
        let mut visited = HashSet::new();
        while let Some(i) = pending.pop() {
            if !visited.insert(i) {
                continue;
            }
            let t = &tris[i];
            if ray_hit(inside, dir, t) && !ray_hit(beyond, dir, t) {
                return true;
            }
            for edge in edges(t) {
                if let Some(neighbors) = by_edge.get(&edge) {
                    pending.extend(neighbors.iter().copied().filter(|j| !visited.contains(j)));
                }
            }
        }
        false
    };
    let low = capped(f.smin, -1.0);
    let high = capped(f.smax, 1.0);
    (high && !low, !low && !high && f.through)
}

/// Parity ray for the radial through-law: from a point halfway
/// between the hole's floor and the mesh's far extreme, offset
/// half a radius off the axis (so it rides the tunnel, never the
/// wall), shoot along +axis and count mesh crossings. A solid
/// floor ahead makes the point's side material (an odd count to
/// infinity); an open exit - the tunnel continuous through faces
/// whose patches the hole itself removed - reads even. Tangencies
/// double-count here, which is why the offset rides the 3-4-5
/// bearing, not an axis: the floor's fan seams and the wall's rim
/// meridians stand at `k * 360/N` degrees, and atan2(3, 4) is no
/// rational fraction of the turn, so the ray can never land on two
/// facets of one surface at once. A misread reads blind, and a
/// blind through-hole's floor sits at the wall's own deepest
/// vertex - tangent, complete, just short of the exit allowance.
#[allow(clippy::too_many_arguments)] // flat geometry reads for one parity vote, like the doors' frames
pub(super) fn ray_open(
    tris: &[Triangle],
    along: usize,
    u: usize,
    v: usize,
    cu: f64,
    cv: f64,
    rad: f64,
    smin: f64,
    mesh_min: f64,
) -> bool {
    let mut o = Point3::new(0.0, 0.0, 0.0);
    set(&mut o, along, (smin + mesh_min) * 0.5);
    set(&mut o, u, cu + rad * 0.4);
    set(&mut o, v, cv + rad * 0.3);
    let mut d = Point3::new(0.0, 0.0, 0.0);
    set(&mut d, along, 1.0);
    let mut hits = 0usize;
    for t in tris {
        if ray_hit(o, d, t) {
            hits += 1;
        }
    }
    hits.is_multiple_of(2)
}
