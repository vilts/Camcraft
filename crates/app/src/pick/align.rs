//! 3-2-1 work-offset alignment from picks (#187): a flat face fixes the
//! WCS Z (its outward normal), a straight edge fixes X (projected into
//! that plane), and a vertex or a bore fixes the origin (projected onto
//! the face) - the CMM "plane, line, point" alignment. Pure: the UI reads
//! the result and sends the existing `SetWcs` command; nothing here writes
//! the world.
//!
//! Provenance: the 3-2-1 (plane-line-point) datum alignment as taught in
//! coordinate-metrology and fixture-design handbooks; the rotation-matrix
//! to quaternion conversion is Shepperd's method (J. Guidance and Control,
//! 1978). Our own code, no vendor material.

use flecs_ecs::prelude::{Entity, World};

use crate::geometry::{Point3, feature, quat_rotate_point, quat_to_euler, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};
use crate::schema::Location;

/// Which frame a datum reader places the picked geometry in: the active
/// setup's view (`setup_align::view_pose`, the 3-2-1) or the document's
/// own placement (`model::transform_of`, the #330 part alignment).
pub(super) type PoseOf = fn(&World, &Entity) -> Option<Location>;

use super::face::face_origin;

/// An edge counts as straight when no point of it strays further than
/// this from the line through its two farthest points (mm), or this
/// fraction of its length, whichever is larger.
const STRAIGHT_TOL_MM: f64 = 0.01;
const STRAIGHT_TOL_REL: f64 = 1e-4;

/// The edge must lean at least this far out of the face's normal (deg) to
/// give the plane a direction worth trusting. (#701: the general role
/// table enforces the same rule between any two axis picks.)
pub(super) const MIN_EDGE_TILT_DEG: f64 = 5.0;

/// A datum plane: a point on it and its outward unit normal (world).
type Plane = ([f64; 3], [f64; 3]);

/// A work-offset placement the WCS pane can send as-is: world position and
/// the pane's roll / pitch / yaw angles (degrees, `quat_from_euler` order).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alignment {
    pub pos: [f64; 3],
    pub roll_deg: f64,
    pub pitch_deg: f64,
    pub yaw_deg: f64,
    /// The frame's axes in world coordinates (unit, right-handed) - what
    /// the angles encode, kept for callers that want to show or check it.
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub z: [f64; 3],
}

/// Build the 3-2-1 alignment from an operation's picks: exactly one flat
/// face, exactly one straight open edge, and at most one origin pick (a
/// vertex, or a curved face such as a bore - its centre). With no origin
/// pick the origin is the edge's end that X runs away from. Every refusal
/// names what to re-pick.
pub fn align_321(world: &World, picks: &[GeoSel]) -> Result<Alignment, String> {
    let mut plane: Option<Plane> = None;
    let mut edge: Option<([f64; 3], [f64; 3])> = None; // (start, end)
    let mut origin: Option<[f64; 3]> = None;
    for g in picks {
        match g.feature {
            FeatureSel::Face(_) => {
                if let Some(p) = flat_face(world, g, crate::setup_align::view_pose)? {
                    if plane.replace(p).is_some() {
                        return Err("pick only one flat face - it sets the Z axis".into());
                    }
                } else {
                    set_origin(&mut origin, curved_center(world, g)?)?;
                }
            }
            FeatureSel::Patch(_) => set_origin(&mut origin, curved_center(world, g)?)?,
            FeatureSel::Chain(_) => {
                if edge.replace(straight_edge(world, g, crate::setup_align::view_pose, "for X")?).is_some() {
                    return Err("pick only one edge - it sets the X axis".into());
                }
            }
            FeatureSel::Vertex(_) => set_origin(&mut origin, vertex(world, g)?)?,
        }
    }
    let (p0, z) = plane.ok_or("pick a flat face - it sets the Z axis")?;
    let (a, b) = edge.ok_or("pick a straight edge - it sets the X axis")?;
    // X: the edge, projected into the plane.
    let d = sub(b, a);
    let len = norm(d);
    let along = dot(d, z);
    let ex = sub(d, scale(z, along));
    let ex_len = norm(ex);
    if !len.is_finite() || len <= 0.0 || ex_len < len * MIN_EDGE_TILT_DEG.to_radians().sin() {
        return Err("the edge stands (nearly) square to the face - pick an edge that runs along it".into());
    }
    let mut x = scale(ex, 1.0 / ex_len);
    // Origin: the pick, else the edge end X runs away from; on the plane.
    let raw = match origin {
        Some(o) => {
            // X runs from the edge end nearer the origin toward the other.
            if norm(sub(a, o)) > norm(sub(b, o)) {
                x = scale(x, -1.0);
            }
            o
        }
        None => a,
    };
    let pos = sub(raw, scale(z, dot(sub(raw, p0), z)));
    let y = cross(z, x);
    let q = quat_from_axes(x, y, z).ok_or("the picks do not make a usable frame - re-pick them")?;
    let (r, p, w) = quat_to_euler(q);
    let out = Alignment {
        pos,
        roll_deg: r.to_degrees(),
        pitch_deg: p.to_degrees(),
        yaw_deg: w.to_degrees(),
        x,
        y,
        z,
    };
    let finite = out.pos.iter().chain(&[out.roll_deg, out.pitch_deg, out.yaw_deg]).all(|v| v.is_finite());
    if !finite {
        return Err("the picks do not make a finite frame - re-pick them".into());
    }
    Ok(out)
}

fn set_origin(slot: &mut Option<[f64; 3]>, p: [f64; 3]) -> Result<(), String> {
    if slot.replace(p).is_some() {
        return Err("pick only one origin - a vertex or a bore".into());
    }
    Ok(())
}

/// A planar face's (centroid, outward unit normal) in world, or `None`
/// when the face is not planar. Outward is the facets' winding (the mesh's
/// own outside), not the feature's winding-free normal.
fn flat_face(world: &World, g: &GeoSel, pose: PoseOf) -> Result<Option<Plane>, String> {
    let Some(n) = outward_normal(world, g, pose)? else { return Ok(None) };
    let c = face_origin(world, &g.entity, g.feature).map_err(|why| why.to_owned())?;
    Ok(Some((c.centroid, n)))
}

/// A planar face's outward unit normal, placed by `pose`; `None` when the
/// face is not planar (or the pick is not a face). Refused by name when
/// the face's body is inside-out, open, non-manifold or mixed (#366).
pub(crate) fn outward_normal(world: &World, g: &GeoSel, pose: PoseOf) -> Result<Option<[f64; 3]>, String> {
    let FeatureSel::Face(fid) = g.feature else { return Ok(None) };
    let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
    let features = model::features_of(world, &e).ok_or_else(|| gone(g))?;
    let f = features.faces.iter().find(|f| f.id == fid).ok_or_else(|| gone(g))?;
    if f.kind != feature::FaceKind::Planar {
        return Ok(None);
    }
    let tris = model::facets_of(world, &e).ok_or_else(|| gone(g))?;
    // #366: the winding is only the outside when the face's body is closed
    // and wound outward; anything else is refused by name, never guessed.
    let verdict = super::winding::verdict_for(&super::winding::winding_of(&tris), &f.facets);
    if let Some(why) = super::winding::refusal(verdict) {
        return Err(format!("{} of '{}' {why}", g.feature, g.entity));
    }
    let mut wind = Point3::new(0.0, 0.0, 0.0);
    for &i in &f.facets {
        if let Some(t) = tris.get(i as usize) {
            let n = (t.b - t.a).cross(t.c - t.a);
            if n.x.is_finite() && n.y.is_finite() && n.z.is_finite() {
                wind = Point3::new(wind.x + n.x, wind.y + n.y, wind.z + n.z);
            }
        }
    }
    let mut n = f.normal;
    if wind.x * n[0] + wind.y * n[1] + wind.z * n[2] < 0.0 {
        n = scale(n, -1.0);
    }
    let loc = pose(world, &e).unwrap_or_default(); // #326: the caller's frame
    let wn = quat_rotate_point(loc.rot, Point3::new(n[0], n[1], n[2]));
    let wn = [wn.x, wn.y, wn.z];
    let l = norm(wn);
    if !(l.is_finite() && l > 0.0) {
        return Err(format!("{} of '{}' has no usable plane - re-pick it", g.feature, g.entity));
    }
    Ok(Some(scale(wn, 1.0 / l)))
}

/// A curved face's centre (a full bore: its axis at mid-height).
fn curved_center(world: &World, g: &GeoSel) -> Result<[f64; 3], String> {
    face_origin(world, &g.entity, g.feature)
        .map(|c| c.centroid)
        .map_err(|why| format!("{} of '{}': {why}", g.feature, g.entity))
}

pub(super) fn vertex(world: &World, g: &GeoSel) -> Result<[f64; 3], String> {
    let FeatureSel::Vertex(id) = g.feature else { return Err(gone(g)) };
    let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
    let features = model::features_of(world, &e).ok_or_else(|| gone(g))?;
    let vi = feature::resolve_vertex(&features, id).ok_or_else(|| gone(g))?;
    let v = features.verts.get(vi as usize).ok_or_else(|| gone(g))?;
    let loc = crate::setup_align::view_pose(world, &e).unwrap_or_default(); // #326
    let w = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
    Ok([w.x, w.y, w.z])
}

/// A straight open edge's (start, end) placed by `pose`, walk order.
/// `role` ends each refusal ("for X": what the edge is picked for).
pub(super) fn straight_edge(
    world: &World,
    g: &GeoSel,
    pose: PoseOf,
    role: &str,
) -> Result<([f64; 3], [f64; 3]), String> {
    let FeatureSel::Chain(cid) = g.feature else { return Err(gone(g)) };
    let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
    let features = model::features_of(world, &e).ok_or_else(|| gone(g))?;
    let c = feature::resolve_chain(&features, cid).ok_or_else(|| gone(g))?;
    if c.closed {
        return Err(format!("{} of '{}' is a closed loop - pick one straight edge {role}", g.feature, g.entity));
    }
    let loc = pose(world, &e).unwrap_or_default(); // #326: the caller's frame
    let pts: Vec<[f64; 3]> = feature::chain_points(&features, &c)
        .iter()
        .map(|p| {
            let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
            [w.x, w.y, w.z]
        })
        .collect();
    let (Some(&a), Some(&b)) = (pts.first(), pts.last()) else {
        return Err(format!("{} of '{}' holds no points - re-pick it", g.feature, g.entity));
    };
    let d = sub(b, a);
    let len = norm(d);
    if !(len.is_finite() && len > 0.0) {
        return Err(format!("{} of '{}' has no length - re-pick it", g.feature, g.entity));
    }
    let u = scale(d, 1.0 / len);
    let tol = STRAIGHT_TOL_MM.max(len * STRAIGHT_TOL_REL);
    for p in &pts {
        let v = sub(*p, a);
        if norm(sub(v, scale(u, dot(v, u)))) > tol {
            return Err(format!("{} of '{}' is not straight - pick a straight edge {role}", g.feature, g.entity));
        }
    }
    Ok((a, b))
}

pub(super) fn gone(g: &GeoSel) -> String {
    format!("{} of '{}' is no longer in the model - re-pick it", g.feature, g.entity)
}

/// Unit quaternion `[x, y, z, w]` whose rotation takes the world axes onto
/// the columns `x, y, z` (Shepperd's method). `None` for a non-finite or
/// degenerate matrix.
pub(super) fn quat_from_axes(x: [f64; 3], y: [f64; 3], z: [f64; 3]) -> Option<[f64; 4]> {
    // m[row][col], columns are the frame's axes.
    let m = [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]];
    let tr = m[0][0] + m[1][1] + m[2][2];
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [(m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, 0.25 * s]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [0.25 * s, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s, (m[2][1] - m[1][2]) / s]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [(m[0][1] + m[1][0]) / s, 0.25 * s, (m[1][2] + m[2][1]) / s, (m[0][2] - m[2][0]) / s]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [(m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, 0.25 * s, (m[1][0] - m[0][1]) / s]
    };
    crate::geometry::normalize_quat(q)
}

pub(super) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(super) fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub(super) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(super) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub(super) fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
