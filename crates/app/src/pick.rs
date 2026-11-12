//! Ray picking over named models: the viewport click target.
//!
//! Provenance: the Möller–Trumbore ray/triangle intersection, reimplemented
//! from the published algorithm (Möller & Trumbore, J. Graphics Tools 1997);
//! no third-party code.

pub mod align;
pub mod axis;
pub mod face;
pub mod feature_arc;
pub mod feature_graph;
pub mod feature_pick;
pub mod feature_sel;
// #561: face / point datums for the fixture locate mates.
pub mod mate;
pub mod plane;
pub mod points;
pub mod roles;
pub mod wall_fit;
pub mod winding;

pub use align::{Alignment, align_321};
pub use axis::{AxisPick, InPlaneAxes, InPlaneAxis, in_plane_turn_deg, pick_coords, turn_in_plane};
pub use plane::{PlaneLay, lay_on_plane, plane_label, plane_side};
pub use points::{PointsLay, lay_on_points, points_side};
pub use face::{FaceOrigin, face_origin, facets_centroid};
pub use roles::{Role, align_roles};
pub use feature_arc::ArcCentre;
pub use feature_graph::{
    FeatureGraph, FeaturePoint, PointKind, WeldedFace, feature_graph, feature_graph_cached, feature_graph_shared,
};
pub use feature_pick::{
    FeaturePick, PickBody, PickKinds, PickQuery, PickRay, PickTarget, Projection, pick_bodies, pick_bodies_of,
    pick_feature,
};
pub use feature_sel::{
    EdgeUnit, check_pickable, feature_chains, graph_of, is_feature_chain, pick_selection, selection_of,
};
pub use mate::{MateDatum, mate_datum};

use flecs_ecs::prelude::World;

use crate::geometry::{
    Point3, Triangle, quat_conjugate, quat_rotate_point, transform_inverse_point, transform_point,
};
use crate::model;

/// Nearest named solid whose facet sits along `origin + t*dir`, `t > 0`.
/// Wrapper over [`first_hit_f`] that keeps only the entity name.
#[must_use]
pub fn first_hit(world: &World, origin: [f64; 3], dir: [f64; 3]) -> Option<String> {
    first_hit_f(world, origin, dir, |_| false).map(|(name, _, _)| name)
}

/// The full pick: nearest entity NAME, the index of the facet it hit (into
/// `model::facets_of` for that entity - the same order `world_facets`
/// transforms, so a viewer can light up exactly the triangle the ray hit),
/// and the world-space hit point. `skip` names entities the ray passes
/// through as if they weren't there - browser-eye-hidden models are
/// display-off, so picking what sits behind them is what the user sees.
///
/// Double-sided; degenerate or non-finite geometry is skipped, never fatal
/// (Prime Directive). Entities' rigid placements are honored by transforming
/// the ray into each entity's local frame.
///
/// Models win at any depth; stock is a fallback. The translucent box is
/// always in the way, and clicking "through the stock at the model" is what
/// the CAM user means (the roadmap's stock-is-pickable promise: click stock
/// where nothing is behind it). WCS entities never carry facets, so the
/// facet test already excludes them from picking.
#[must_use]
pub fn first_hit_f(
    world: &World,
    origin: [f64; 3],
    dir: [f64; 3],
    skip: impl Fn(&str) -> bool,
) -> Option<(String, usize, [f64; 3])> {
    if !origin.iter().all(|v| v.is_finite()) || !dir.iter().all(|v| v.is_finite()) {
        return None;
    }
    let o = Point3::new(origin[0], origin[1], origin[2]);
    let d = Point3::new(dir[0], dir[1], dir[2]);
    let mut best_model: Option<(f64, String, usize, [f64; 3])> = None;
    let mut best_stock: Option<(f64, String, usize, [f64; 3])> = None;
    for (name, entity) in model::named_entities(world) {
        if skip(&name) {
            continue;
        }
        let Some(tris) = model::facets_of(world, &entity) else {
            continue;
        };
        // One inverse rigid transform of the ray per entity, instead of one
        // shift per triangle. Rotations are unit quats, so distances along
        // the local ray equal world distances: depths stay comparable.
        // #326: rays land on the part where the active setup holds it.
        let loc = crate::setup_align::view_pose(world, &entity).unwrap_or_default();
        let o_local = transform_inverse_point(o, loc.pos, loc.rot);
        let d_local = quat_rotate_point(quat_conjugate(loc.rot), d);
        if !(o_local.is_finite() && d_local.is_finite()) {
            continue;
        }
        let bucket = if model::is_stock(world, &entity) {
            &mut best_stock
        } else {
            &mut best_model
        };
        for (idx, t) in tris.iter().enumerate() {
            let Some(hit_t) = ray_triangle(*t, o_local, d_local) else {
                continue;
            };
            if bucket.as_ref().is_none_or(|(b, _, _, _)| hit_t < *b) {
                // Local hit point, pushed back through the entity's rigid
                // placement into the frame the viewer draws in.
                let hit_local = Point3::new(
                    o_local.x + d_local.x * hit_t,
                    o_local.y + d_local.y * hit_t,
                    o_local.z + d_local.z * hit_t,
                );
                let p = transform_point(hit_local, loc.pos, loc.rot);
                *bucket = Some((hit_t, name.clone(), idx, [p.x, p.y, p.z]));
            }
        }
    }
    best_model
        .or(best_stock)
        .map(|(_, name, facet, xyz)| (name, facet, xyz))
}

/// Depth test for a point pick (M13.4 vertex picking): is `p` the first
/// thing the sight line from `eye` meets? Uses the same model-over-stock
/// ray as [`first_hit_f`], so a vertex behind the translucent stock stays
/// pickable exactly like the face it sits on. `tol` (mm) absorbs the
/// facets that meet AT the vertex - they hit at its own distance.
#[must_use]
pub fn point_visible(
    world: &World,
    eye: [f64; 3],
    p: [f64; 3],
    tol: f64,
    skip: impl Fn(&str) -> bool,
) -> bool {
    let d = [p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if !len.is_finite() || len < 1e-12 {
        return false; // the eye sits on the point: nothing honest to say
    }
    let dir = [d[0] / len, d[1] / len, d[2] / len];
    match first_hit_f(world, eye, dir, skip) {
        None => true,
        Some((_, _, h)) => {
            let hd = ((h[0] - eye[0]).powi(2) + (h[1] - eye[1]).powi(2) + (h[2] - eye[2]).powi(2)).sqrt();
            hd >= len - tol
        }
    }
}

/// Pure twin of [`point_visible`] over one triangle soup in world space:
/// true when some facet crosses the open sight segment `eye -> p` more
/// than `tol` (mm) before `p`. The cube law test's occluder.
#[must_use]
pub fn sight_blocked(tris: &[Triangle], eye: [f64; 3], p: [f64; 3], tol: f64) -> bool {
    let o = Point3::new(eye[0], eye[1], eye[2]);
    let d = Point3::new(p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]);
    let len = d.dot(d).sqrt();
    if !len.is_finite() || len < 1e-12 {
        return false;
    }
    let cut = 1.0 - tol / len; // t along the unnormalized ray: 1 is at p
    tris.iter().any(|t| ray_triangle(*t, o, d).is_some_and(|hit| hit < cut))
}

/// Nearest facet of a world-space triangle soup along `origin + t*dir`,
/// `t > 0`: its index and the hit point. The machined stock's extracted
/// mesh (M13.1 owed) is no entity - it sits in world space already, so
/// the viewer's hover reads it here with the same crossing test the
/// entity pick uses. Double-sided; non-finite input is a miss.
#[must_use]
pub fn soup_hit(tris: &[Triangle], origin: [f64; 3], dir: [f64; 3]) -> Option<(usize, [f64; 3])> {
    if !origin.iter().all(|v| v.is_finite()) || !dir.iter().all(|v| v.is_finite()) {
        return None;
    }
    let o = Point3::new(origin[0], origin[1], origin[2]);
    let d = Point3::new(dir[0], dir[1], dir[2]);
    let mut best: Option<(f64, usize)> = None;
    for (idx, t) in tris.iter().enumerate() {
        if let Some(hit_t) = ray_triangle(*t, o, d)
            && best.is_none_or(|(b, _)| hit_t < b)
        {
            best = Some((hit_t, idx));
        }
    }
    best.map(|(t, idx)| (idx, [o.x + d.x * t, o.y + d.y * t, o.z + d.z * t]))
}

/// Ray/triangle crossing in the entity's local frame; the ray direction need
/// not be normalized. Returns t along the ray.
pub(crate) fn ray_triangle(facet: Triangle, origin: Point3, dir: Point3) -> Option<f64> {
    let (a, b, c) = (facet.a, facet.b, facet.c);
    if !(a.is_finite() && b.is_finite() && c.is_finite()) {
        return None;
    }
    let e1 = b - a;
    let e2 = c - a;
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 || !det.is_finite() {
        return None; // parallel or degenerate facet
    }
    let s = origin - a;
    let inv = 1.0 / det;
    let u = s.dot(p) * inv;
    if !(-1e-9..=1.0 + 1e-9).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if !(v >= -1e-9 && u + v <= 1.0 + 1e-9) {
        return None;
    }
    let t = e2.dot(q) * inv;
    if t.is_finite() && t > 1e-9 {
        Some(t)
    } else {
        None
    }
}
