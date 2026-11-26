//! A runaway face divided again over its boundary nodes only (#2844).
//! solidcore-tess plans a long thin strip (a thread flank) isotropically:
//! hundreds of thousands of triangles where the strip's own edge nodes
//! already describe it. Here such a face gets the Delaunay fill of its
//! boundary loop, with no node inside, and keeps it only when
//! [`mesh_deviation::measure_within`] proves it inside the tolerance. The
//! shared edge nodes stay as they are, so the neighbouring faces still
//! meet it point for point.
//! Provenance: our own seam design over the public API of solidcore-tess
//! (`FacePlan`, `TessPlan`; Apache-2.0) and the polygon fill of
//! `mesh_redivide_fill`. No HSMWorks material consulted.

use std::collections::HashSet;

use solidcore::kernel::CancelToken;
use solidcore_record::brep::Brep;
use solidcore_tess::{EvalSurface, FacePlan, TessPlan};

use crate::mesh_deviation;
use crate::mesh_redivide_fill::{area2, fill};
use crate::mesh_weld::SNAP_MOVE_MM;

/// Only a face the planner gave at least this many triangles is divided
/// again: a face under it is cheap as planned and stays the planner's.
pub const REDIVIDE_MIN_TRIANGLES: usize = 10_000;

/// Only a face with this many planned triangles for each boundary node is
/// divided again (the boundary alone makes about one triangle a node).
pub const REDIVIDE_RATIO: usize = 16;

/// Every runaway face of `plan` (see the constants) divided over its
/// boundary nodes where that division measures inside `tol_mm`; each one
/// named in the returned notes. A face it cannot divide, or whose division
/// measures over the tolerance, keeps the planner's triangles.
pub(crate) fn redivide(plan: &mut TessPlan, record: &Brep, tol_mm: f64, stop: &CancelToken) -> Vec<String> {
    let mut notes = Vec::new();
    for i in 0..plan.faces.len() {
        if stop.is_cancelled() {
            break;
        }
        let Some(f) = plan.faces.get(i) else { continue };
        let planned = f.triangles.len();
        if planned < REDIVIDE_MIN_TRIANGLES || planned / REDIVIDE_RATIO < f.fixed.len() {
            continue;
        }
        let Some((index, s)) = f.surface.and_then(|k| Some((k, plan.surfaces.get(k as usize)?))) else { continue };
        let Some(triangles) = boundary_division(f, s) else { continue };
        let Some(uv) = f.uv.get(..f.fixed.len()) else { continue };
        let candidate = FacePlan {
            face: f.face,
            surface: Some(0),
            uv: uv.to_vec(),
            fixed: f.fixed.clone(),
            normals: Vec::new(),
            flip: f.flip,
            triangles,
            poles: Vec::new(),
        };
        let mut alone = TessPlan { surfaces: vec![s.clone()], faces: vec![candidate], ..TessPlan::default() };
        let Ok(deviation) = mesh_deviation::measure_within(&alone, record, tol_mm, stop) else { continue };
        let judged = deviation.largest() + SNAP_MOVE_MM;
        if judged.is_nan() || judged > tol_mm {
            continue;
        }
        let (Some(mut kept), Some(slot)) = (alone.faces.pop(), plan.faces.get_mut(i)) else { continue };
        kept.surface = Some(index);
        notes.push(format!(
            "face {}: divided over its {} boundary nodes ({} triangles for the planner's {planned})",
            kept.face,
            kept.fixed.len(),
            kept.triangles.len()
        ));
        *slot = kept;
    }
    notes
}

/// The sides of `triangles` that no neighbour runs back along, each in its
/// triangle's direction. `None`: a side used twice in one direction.
fn boundary_sides(triangles: &[[u32; 3]]) -> Option<HashSet<(u32, u32)>> {
    let mut all = HashSet::with_capacity(triangles.len().saturating_mul(3));
    for t in triangles {
        for side in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            if !all.insert(side) {
                return None;
            }
        }
    }
    Some(all.iter().copied().filter(|(a, b)| !all.contains(&(*b, *a))).collect())
}

/// The triangles of `f` over its boundary nodes alone, wound as the
/// planner's. `None` where the face is not a plain region of its (u, v)
/// plane inside one loop of its shared edge nodes (a pole, a hole, a
/// triangle turned over across a period, a boundary node that is not a
/// shared one, a pinched boundary), or where the fill does not keep that
/// boundary.
fn boundary_division(f: &FacePlan, s: &EvalSurface) -> Option<Vec<[u32; 3]>> {
    let n = f.fixed.len();
    if n < 3 || f.uv.len() < n || !f.poles.is_empty() || !f.normals.is_empty() {
        return None;
    }
    // One sense for every planned triangle in (u, v): the face is flat there.
    let (mut ccw, mut cw) = (false, false);
    for t in &f.triangles {
        let [a, b, c] = [f.uv.get(t[0] as usize)?, f.uv.get(t[1] as usize)?, f.uv.get(t[2] as usize)?];
        let area = area2(*a, *b, *c);
        ccw |= area > 0.0;
        cw |= area < 0.0;
    }
    if ccw == cw {
        return None;
    }
    let boundary = boundary_sides(&f.triangles)?;
    // Each boundary node is a shared one and leaves along one side only.
    let mut next: Vec<Option<u32>> = vec![None; n];
    for (a, b) in &boundary {
        let slot = next.get_mut(*a as usize)?;
        if slot.is_some() || *b as usize >= n {
            return None;
        }
        *slot = Some(*b);
    }
    // A shared node inside the face (not on its boundary) cannot be kept.
    if f.triangles.iter().flatten().any(|i| next.get(*i as usize).is_some_and(Option::is_none)) {
        return None;
    }
    // The fill's plane: (u, v) scaled to the surface's mean lengths, so
    // that near in the plane is near on the surface; u mirrored for a face
    // the planner wound clockwise.
    let (mut su, mut sv, mut count) = (0.0_f64, 0.0_f64, 0.0_f64);
    for uv in f.uv.iter().take(n) {
        let (_, du, dv) = s.derivs(uv[0], uv[1])?;
        su += (du.x * du.x + du.y * du.y + du.z * du.z).sqrt();
        sv += (dv.x * dv.x + dv.y * dv.y + dv.z * dv.z).sqrt();
        count += 1.0;
    }
    let (su, sv) = (if ccw { su / count } else { -su / count }, sv / count);
    if !(su.is_finite() && sv.is_finite() && su != 0.0 && sv > 0.0) {
        return None;
    }
    // The one loop, from its lowest node (one result for one plan).
    let start = next.iter().position(Option::is_some)?;
    let (mut ring, mut node) = (Vec::with_capacity(boundary.len()), start);
    loop {
        ring.push(u32::try_from(node).ok()?);
        node = (*next.get(node)?)? as usize;
        if node == start {
            break;
        }
        if ring.len() > boundary.len() {
            return None;
        }
    }
    if ring.len() != boundary.len() {
        return None;
    }
    let points: Vec<[f64; 2]> =
        ring.iter().map(|i| f.uv.get(*i as usize).map(|uv| [uv[0] * su, uv[1] * sv])).collect::<Option<_>>()?;
    let out: Vec<[u32; 3]> = fill(&points)?
        .iter()
        .map(|t| Some([*ring.get(t[0] as usize)?, *ring.get(t[1] as usize)?, *ring.get(t[2] as usize)?]))
        .collect::<Option<_>>()?;
    // Counter-clockwise in the fill's plane (so wound as the planner's,
    // in the mirrored plane too) with the planner's boundary: they cover
    // the face once.
    (boundary_sides(&out)? == boundary).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use solidcore_record::Vec3;

    fn plane() -> EvalSurface {
        EvalSurface::Plane {
            origin: solidcore_tess::vec3(Vec3::new(0.0, 0.0, 0.0)),
            du: solidcore_tess::vec3(Vec3::new(1.0, 0.0, 0.0)),
            dv: solidcore_tess::vec3(Vec3::new(0.0, 1.0, 0.0)),
        }
    }

    /// A 4 x 4 square of cells, 100 x 1 in (u, v): the 16 boundary nodes
    /// first (the shared ones), then the 9 inside; two triangles a cell.
    fn grid(clockwise: bool) -> FacePlan {
        let cell = |i: usize, j: usize| [i as f64 * 25.0, j as f64 * 0.25];
        let on_rim = |i: usize, j: usize| i == 0 || j == 0 || i == 4 || j == 4;
        let all: Vec<(usize, usize)> = (0..5).flat_map(|j| (0..5).map(move |i| (i, j))).collect();
        let mut nodes: Vec<(usize, usize)> = all.iter().copied().filter(|(i, j)| on_rim(*i, *j)).collect();
        nodes.extend(all.iter().copied().filter(|(i, j)| !on_rim(*i, *j)));
        let id = |i: usize, j: usize| nodes.iter().position(|n| *n == (i, j)).unwrap() as u32;
        let mut triangles = Vec::new();
        for j in 0..4 {
            for i in 0..4 {
                triangles.push([id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                triangles.push([id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
        if clockwise {
            triangles.iter_mut().for_each(|t| t.swap(1, 2));
        }
        let uv: Vec<[f64; 2]> = nodes.iter().map(|(i, j)| cell(*i, *j)).collect();
        let fixed = uv.iter().take(16).map(|p| Vec3::new(p[0], p[1], 0.0)).collect();
        FacePlan { face: 0, surface: Some(0), uv, fixed, flip: clockwise, triangles, ..FacePlan::default() }
    }

    #[test]
    fn a_face_is_divided_over_its_boundary_nodes_with_the_same_boundary_and_sense() {
        for clockwise in [false, true] {
            let f = grid(clockwise);
            let out = boundary_division(&f, &plane()).expect("a plain region divides");
            assert_eq!(out.len(), 14, "16 boundary nodes make 14 triangles");
            assert!(out.iter().flatten().all(|i| *i < 16), "no inside node is used");
            assert_eq!(boundary_sides(&out), boundary_sides(&f.triangles));
            let area: f64 = out.iter().map(|t| area2(f.uv[t[0] as usize], f.uv[t[1] as usize], f.uv[t[2] as usize])).sum();
            assert_eq!(area, if clockwise { -200.0 } else { 200.0 }, "the whole face, wound as planned");
        }
    }

    #[test]
    fn a_face_that_is_not_a_plain_region_of_its_shared_nodes_is_not_divided() {
        // A boundary node that is not a shared edge node.
        let mut f = grid(false);
        f.fixed.truncate(15);
        assert_eq!(boundary_division(&f, &plane()), None);
        // A shared node inside the face.
        let mut f = grid(false);
        f.fixed.push(Vec3::new(25.0, 0.25, 0.0));
        assert_eq!(boundary_division(&f, &plane()), None);
        // A triangle turned over against the others.
        let mut f = grid(false);
        f.triangles[5].swap(1, 2);
        assert_eq!(boundary_division(&f, &plane()), None);
        // A pole node.
        let mut f = grid(false);
        f.poles.push(solidcore_tess::PoleNode { node: 0, rule: solidcore_tess::PoleRule::Limit { along: 0, reach: 0.0 } });
        assert_eq!(boundary_division(&f, &plane()), None);
    }
}
