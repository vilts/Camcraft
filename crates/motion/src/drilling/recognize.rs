//! Hole recognition from the facet mesh (M5.0.1; axis-candidate
//! generalization M5.2 F2). A drilled hole is a closed round wall,
//! and F2's law is that the wall's AXIS is a finding, not a
//! premise: the Z-only filter that opened every wall circle was
//! stage one of the truth, and the cross-drilled hole - the live
//! tooling's subject on a lathe - is the same statement read of a
//! radial axis. So recognition runs the same machine three times,
//! once per candidate axis (`cylinder::AXES`): weld the facets that
//! could be that axis's walls, circle-fit each component's rim, and
//! report what answers. The weld is edge-AND-angle: two facets join
//! only across a shared edge whose normals sit within 45 degrees,
//! so a wall never bridges the dihedral where surfaces meet (a
//! cross hole crossing the bar's face, a wall standing on a floor)
//! and one hole's fit is never contaminated by its neighbor.
//!
//! Provenance: recognition of cylindrical walls by normal
//! filtering, circle fitting and rim-span testing is general
//! machine-vision lore (the Kasa fit lives in `cylinder`); the
//! per-candidate weld and the shared-edge angle gate are ours,
//! specced clean-room from the law that a drilled hole is a closed
//! round wall wherever its axis points. No vendor feature-
//! recognition strategy consulted.

use super::cylinder::{coord, fit_circle_uv, hypot, same_surface, vkey, AXES};
use super::mouth::ray_open;
use super::hole::{CrossHole, Hole};

use std::collections::HashMap;

use crate::geometry::{Point3, Triangle};

/// Facets with a normal-component at or above this along the
/// candidate axis are caps of that axis, or too steep to be one of
/// its walls. A 45-degree chamfer sits at 0.707 - outside by
/// design; a shallower chamfer ring is caught and merged below.
/// The same filter the pre-F2 Z-only recognition stood on, read of
/// the candidate's coordinate: a cap never welds, so a plate's two
/// face sheets can never bridge its holes into one blob.
pub(super) const WALL_NZ: f64 = 0.7;

/// Farthest a fitted vertex may sit from the shared circle before the
/// component is "not a cylinder". 20 um: over the chord noise of our
/// own tessellations, under any real taper.
pub(super) const FIT_TOL: f64 = 0.02;

/// A hole is a hole, not a pinprick or a silo.
pub(super) const R_MIN: f64 = 0.05;

pub(super) const R_MAX: f64 = 500.0;

/// Transverse tolerance for coaxial wall components. Only touching or
/// overlapping axial spans can fold into one stepped hole; separated
/// blind holes keep their own spans and mouths.
pub(super) const MERGE_DXY: f64 = 0.05;

/// A closed rim spans this much of the compass; a flat wall's vertices
/// straddle two opposite rays (span ~180), a slot's arc end 180. Both
/// fail; a full circle passes.
pub(super) const SPAN_MIN: f64 = std::f64::consts::TAU * 0.75;

/// A wall must stand on at least this many distinct vertices before
/// a circle can be honestly claimed through them.
const MIN_FIT_VIDS: usize = 6;

/// One found cylinder, in whichever candidate pass found it: the
/// transverse center, the radius, the span along the axis, and
/// whether the low end is open (a through).
#[derive(Clone, Copy)]
pub(super) struct Found {
    pub(super) cu: f64,
    pub(super) cv: f64,
    pub(super) rad: f64,
    pub(super) smin: f64,
    pub(super) smax: f64,
    pub(super) through: bool,
}

/// Find the drilled holes in a triangle mesh: closed, round, vertical
/// walls whose facets face their own axis (so raised bosses fall out),
/// merged across Z steps, snapped onto the lattice so re-detection of
/// an unmoved model is bit-stable. Degenerate or empty input yields an
/// empty list - never a panic, never a hang (Prime Directive).
///
/// Runs on raw facets, not the Feature layer: recognition must not
/// inherit (or drift from) feature grouping.
#[must_use]
pub fn recognize(tris: &[Triangle]) -> Vec<Hole> {
    analyze(tris).0
}

/// Directed findings: radial X/Y holes from either side, plus Z holes
/// whose mouth is below their blind floor. Each record preserves its
/// own mouth, inward plunge direction and depth. The live-tooling
/// generator still refuses Z plunges; recognition does not promise
/// that the mounted machine can generate every detected direction.
#[must_use]
pub fn recognize_cross(tris: &[Triangle]) -> Vec<CrossHole> {
    analyze(tris).1
}

/// Three candidate readings of one mesh: (Z holes, radial holes).
/// The Z output is the law `recognize` shipped since M5.0.1 - the
/// same wall filter, the same fit, the same vote - with the weld
/// moved from bare vertex-sharing to shared-edge-plus-angle, which
/// only ever SPLITS what a fit was failing anyway: a hole welded to
/// a through-slot's wall fitted no circle and dropped before too.
/// A slot still drops (a racetrack fits no circle in any plane); a
/// boss still votes away; a chamfer ring still folds into its hole.
fn analyze(tris: &[Triangle]) -> (Vec<Hole>, Vec<CrossHole>) {
    // Mesh extents: what makes a Z hole "through".
    let mut mins = [f64::INFINITY; 3];
    for t in tris {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) || !t.normal().is_finite() {
            continue;
        }
        for v in [a, b, c] {
            for (i, m) in mins.iter_mut().enumerate() {
                *m = m.min(coord(v, i));
            }
        }
    }
    if !mins[2].is_finite() {
        return (Vec::new(), Vec::new());
    }

    let mut found: [Vec<Found>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (ci, &(along, u, v)) in AXES.iter().enumerate() {
        found[ci] = analyze_axis(tris, along, u, v, mins[along], false);
    }

    super::merge::collect(tris, found, mins[2])
}

pub(super) fn set(p: &mut Point3, i: usize, val: f64) {
    match i {
        0 => p.x = val,
        1 => p.y = val,
        _ => p.z = val,
    }
}

/// One candidate axis's readings: weld its wall facets (finite,
/// normal not a cap of this axis) into components across shared
/// edges the angle test forgives, and classify each component.
///
/// `convex` reads the other side of the same walls (#985): the
/// components whose facets face AWAY from their axis - raised bosses.
pub(super) fn analyze_axis(
    tris: &[Triangle],
    along: usize,
    u: usize,
    v: usize,
    mesh_min: f64,
    convex: bool,
) -> Vec<Found> {
    // Vertex table on exact bits: `world_facets` transforms shared
    // vertices identically, so seams match exactly. But the union-
    // find runs on FACETS, not vertices - a cross-hole wall and the
    // bar's face share rim vertices at the intersection curve, and
    // vertex-keyed welding there is exactly the F2 blob that
    // silently drops both holes. Vertices are the adjacency KEY
    // only; the weld itself is edge-plus-angle.
    let mut ids: HashMap<(u64, u64, u64), usize> = HashMap::new();
    let mut verts: Vec<Point3> = Vec::new();
    let mut facets: Vec<([usize; 3], Point3)> = Vec::new(); // vids, normal
    let mut parent: Vec<usize> = Vec::new(); // over facet indices
    for t in tris {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            continue;
        }
        let n = t.normal();
        if !n.is_finite() || coord(n, along).abs() >= WALL_NZ {
            continue; // a cap of this axis is not its wall
        }
        let mut v3 = [0usize; 3];
        for (slot, p) in v3.iter_mut().zip([a, b, c]) {
            *slot = match ids.get(&vkey(&p)) {
                Some(&i) => i,
                None => {
                    let i = verts.len();
                    ids.insert(vkey(&p), i);
                    verts.push(p);
                    i
                }
            };
        }
        parent.push(facets.len());
        facets.push(([v3[0], v3[1], v3[2]], n));
    }
    // Weld facets across shared edges, angle-gated: adjacent
    // facets of one tessellated cylinder answer cos(dtheta) >=
    // 0.7 and pass; walls meeting at a dihedral never do. Every
    // co-edge pair is tested, so a non-manifold triple cannot
    // strand a member behind a refused pair.
    type VKey = (u64, u64, u64);
    let mut edge_of: HashMap<(VKey, VKey), Vec<usize>> = HashMap::new();
    for (fi, (fv, _)) in facets.iter().enumerate() {
        for e in [(fv[0], fv[1]), (fv[1], fv[2]), (fv[2], fv[0])] {
            let (k0, k1) = (vkey(&verts[e.0]), vkey(&verts[e.1]));
            edge_of
                .entry(if k0 <= k1 { (k0, k1) } else { (k1, k0) })
                .or_default()
                .push(fi);
        }
    }
    for fis in edge_of.values() {
        for a in 0..fis.len() {
            for b in a + 1..fis.len() {
                let (fa, fb) = (fis[a], fis[b]);
                if fa != fb && same_surface(facets[fa].1, facets[fb].1) {
                    union(&mut parent, fa, fb);
                }
            }
        }
    }
    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for fi in 0..facets.len() {
        by_root.entry(root(&mut parent, fi)).or_default().push(fi);
    }

    let mut out = Vec::new();
    for fis in by_root.values() {
        if let Some(f) = classify(fis, &facets, &verts, tris, along, u, v, mesh_min, convex) {
            out.push(f);
        }
    }
    out
    // (A component the fit refuses drops silently, exactly as the
    // Z-only pipeline dropped slots and bosses; a lathe's whole
    // set of cross holes is user-visible in the Detect holes list,
    // and the turned solid's OD band is such a drop by law.)
}

/// The component's verdict: wall vertices, span, circle, quality,
/// rim, and the hole-vs-boss vote - every law the Z-only original
/// stated, read of the candidate's own coordinates. `None` is the
/// honest drop: planar sets collapse to a line in the transverse
/// plane (singular normal equations, no circle), tapers and bands
/// blow the fit tolerance, half-rims fail the span.
#[allow(clippy::too_many_arguments)] // the component's whole frame (weld, axis, candidates) - threaded once, like revolve's scan
fn classify(
    fis: &[usize],
    facets: &[([usize; 3], Point3)],
    verts: &[Point3],
    tris: &[Triangle],
    along: usize,
    u: usize,
    v: usize,
    mesh_min: f64,
    convex: bool,
) -> Option<Found> {
    let mut vids: Vec<usize> = Vec::new();
    let mut cens: Vec<(Point3, Point3)> = Vec::new();
    for &fi in fis {
        let (fv, n) = facets[fi];
        for &vi in &fv {
            if !vids.contains(&vi) {
                vids.push(vi);
            }
        }
        let cen = Point3::new(
            (verts[fv[0]].x + verts[fv[1]].x + verts[fv[2]].x) / 3.0,
            (verts[fv[0]].y + verts[fv[1]].y + verts[fv[2]].y) / 3.0,
            (verts[fv[0]].z + verts[fv[1]].z + verts[fv[2]].z) / 3.0,
        );
        cens.push((cen, n));
    }
    if vids.len() < MIN_FIT_VIDS {
        return None; // a facet pair or a sliver is not a hole wall
    }
    let (mut smin, mut smax) = (f64::INFINITY, f64::NEG_INFINITY);
    for &vi in &vids {
        let s = coord(verts[vi], along);
        smin = smin.min(s);
        smax = smax.max(s);
    }
    if smax - smin <= 1e-6 {
        return None; // a ring of facets laid flat, not a wall
    }
    let (cu, cv, rad) = fit_circle_uv(&vids, verts, u, v)?;
    if !(rad.is_finite() && R_MIN < rad && rad < R_MAX) {
        return None;
    }
    let worst = vids
        .iter()
        .map(|&vi| (hypot(coord(verts[vi], u) - cu, coord(verts[vi], v) - cv) - rad).abs())
        .fold(0.0f64, f64::max);
    if worst > FIT_TOL {
        return None; // a slot's racetrack fits no circle
    }
    let mut angs: Vec<f64> = vids
        .iter()
        .map(|&vi| (coord(verts[vi], v) - cv).atan2(coord(verts[vi], u) - cu))
        .collect();
    angs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut max_gap = angs[0] + std::f64::consts::TAU - angs[angs.len() - 1];
    for w in angs.windows(2) {
        max_gap = max_gap.max(w[1] - w[0]);
    }
    if std::f64::consts::TAU - max_gap <= SPAN_MIN {
        return None;
    }
    // Hole vs boss: on a hole's wall an outward-wound mesh's normals
    // point AT the axis. A raised cylinder's wall points away. Vote
    // by facet; a coin flip refuses.
    let (mut toward, mut away) = (0usize, 0usize);
    for (cen, n) in &cens {
        let d = coord(*n, u) * (cu - coord(*cen, u)) + coord(*n, v) * (cv - coord(*cen, v));
        if d > 0.0 {
            toward += 1;
        } else if d < 0.0 {
            away += 1;
        }
    }
    // A boss pass (#985) keeps exactly the walls this vote drops.
    let (ours, theirs) = if convex { (away, toward) } else { (toward, away) };
    if ours <= theirs {
        return None;
    }
    // Through, by the axis' own physics:
    // - Z (the mill law, unchanged): the floor reaches the mesh's
    //   bottom plane exactly.
    // - radial: the same edge law answers when the hole's circle
    //   straddles the bar's own axis line - the common live-tooling
    //   case, where the wall's deepest vertex IS the mesh's far
    //   extreme exactly (the rim circle passes y = 0 on a round
    //   bar). When it doesn't, a parity ray read between the floor
    //   and the mesh's far extreme settles it: air past the floor
    //   is an exit, material is a floor.
    let through = if along == 2 {
        (smin - mesh_min).abs() <= 1e-6
    } else {
        (smin - mesh_min).abs() <= 1e-6
            || ray_open(tris, along, u, v, cu, cv, rad, smin, mesh_min)
    };
    Some(Found {
        cu,
        cv,
        rad,
        smin,
        smax,
        through,
    })
}

/// Union-find, path-halving, on the vertex table.
fn union(parent: &mut [usize], i: usize, j: usize) {
    let (ra, rb) = (root(parent, i), root(parent, j));
    if ra != rb {
        parent[ra.max(rb)] = ra.min(rb);
    }
}

fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}
