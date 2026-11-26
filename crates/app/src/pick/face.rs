//! Work-origin snap targets on a picked face (M13.4): the face's
//! area-weighted centroid, and - when the face lies flat - its plane
//! height, for a "Z only" origin move. The UI reads these and sends the
//! existing `SetWcs` command; nothing here writes the world.
//!
//! Provenance: the area-weighted centroid of a triangulated region
//! (sum of triangle centroids weighted by triangle area) from standard
//! computational geometry texts; our own code, no vendor material.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, Triangle, feature, quat_rotate_point, transform_point};
use crate::model;
use crate::operation::FeatureSel;

/// A face counts as horizontal when its normal sits within this angle
/// of +Z or -Z (degrees).
pub const HORIZONTAL_TOL_DEG: f64 = 1.0;

/// Where a picked face can put the work origin, in world coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceOrigin {
    /// Area-weighted centroid of the face's facets. On the plane for a
    /// planar face; for a curved weld (a cylinder wall) it is the
    /// centre of mass of the surface - a full bore's axis at mid-height.
    pub centroid: [f64; 3],
    /// True for a single planar face, false for a welded curved wall.
    pub planar: bool,
    /// The plane's world Z when the face is planar AND horizontal
    /// (normal within [`HORIZONTAL_TOL_DEG`] of ±Z); `None` otherwise.
    pub plane_z: Option<f64>,
}

/// Area-weighted centroid of the facets `which` picks out of `tris`.
/// Non-finite or zero-area facets carry no weight; `None` when nothing
/// with area is left (never a NaN, never a guess at the origin).
#[must_use]
pub fn facets_centroid(tris: &[Triangle], which: impl IntoIterator<Item = usize>) -> Option<[f64; 3]> {
    let mut sum = [0.0_f64; 3];
    let mut total = 0.0_f64;
    for i in which {
        let Some(t) = tris.get(i) else { continue };
        if !(t.a.is_finite() && t.b.is_finite() && t.c.is_finite()) {
            continue;
        }
        let n = (t.b - t.a).cross(t.c - t.a);
        let area = 0.5 * n.dot(n).sqrt();
        if !area.is_finite() || area <= 0.0 {
            continue;
        }
        sum[0] += area * (t.a.x + t.b.x + t.c.x) / 3.0;
        sum[1] += area * (t.a.y + t.b.y + t.c.y) / 3.0;
        sum[2] += area * (t.a.z + t.b.z + t.c.z) / 3.0;
        total += area;
    }
    if !(total.is_finite() && total > 0.0) {
        return None;
    }
    let c = [sum[0] / total, sum[1] / total, sum[2] / total];
    c.iter().all(|v| v.is_finite()).then_some(c)
}

/// The origin targets of a picked face or welded wall of the named
/// entity, through its placement. Err is a reason the user can act on:
/// points and edges are not faces, and a stale pick says re-pick.
pub fn face_origin(world: &World, entity: &str, sel: FeatureSel) -> Result<FaceOrigin, &'static str> {
    let gone = "the picked face is no longer in the model - re-pick it";
    let e = model::find_by_name(world, entity).ok_or("the picked entity is gone - re-pick it")?;
    let features = model::features_of(world, &e).ok_or("the picked entity carries no geometry")?;
    let tris = model::facets_of(world, &e).ok_or("the picked entity carries no geometry")?;
    // #326: the face where the active setup holds the part.
    let loc = crate::setup_align::view_pose(world, &e).unwrap_or_default();
    let (facets, normal): (Vec<usize>, Option<[f64; 3]>) = match &sel {
        FeatureSel::Face(fid) => {
            let f = features.faces.iter().find(|f| f.id == *fid).ok_or(gone)?;
            if f.kind != feature::FaceKind::Planar {
                return Err("the picked face has no usable plane");
            }
            (f.facets.iter().map(|&i| i as usize).collect(), Some(f.normal))
        }
        FeatureSel::Patch(pid) => {
            // #2247: memoized per feature set - the chips ask on each frame.
            let all = model::patches(&features);
            let p = all.iter().find(|p| p.id == *pid).ok_or(gone)?;
            let facets = features
                .faces
                .iter()
                // `Patch::faces` is sorted: a search, not a scan per face.
                .filter(|f| p.faces.binary_search(&f.id).is_ok())
                .flat_map(|f| f.facets.iter().map(|&i| i as usize))
                .collect();
            (facets, None)
        }
        FeatureSel::Vertex(_)
        | FeatureSel::Chain(_)
        | FeatureSel::SketchChain(_)
        | FeatureSel::SketchLayer(_) => return Err("only a face has a centroid"),
    };
    let c = facets_centroid(&tris, facets).ok_or("the picked face has no area")?;
    let w = transform_point(Point3::new(c[0], c[1], c[2]), loc.pos, loc.rot);
    let centroid = [w.x, w.y, w.z];
    if !centroid.iter().all(|v| v.is_finite()) {
        return Err("the picked face's centroid is not finite");
    }
    let plane_z = normal.and_then(|n| {
        let wn = quat_rotate_point(loc.rot, Point3::new(n[0], n[1], n[2]));
        let len = wn.dot(wn).sqrt();
        let horizontal =
            len.is_finite() && len > 0.0 && (wn.z / len).abs() >= HORIZONTAL_TOL_DEG.to_radians().cos();
        horizontal.then_some(centroid[2])
    });
    Ok(FaceOrigin { centroid, planar: normal.is_some(), plane_z })
}
