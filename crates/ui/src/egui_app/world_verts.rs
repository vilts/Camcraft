//! The world-space vertices of one drawn body (#2201): its facets through
//! the pose that the viewport shows (the active setup's alignment, a handle
//! drag's slide, a ring drag's turn), read from the per-`State` vertex
//! cache. Moved out of `world.rs` unchanged when the shade caches (#2834)
//! began to read the facets and the pose of a body too.
//! Provenance: our own code, from `world.rs`.

use camcraft_app::geometry::{Point3, transform_point};
use camcraft_app::model;

use crate::interact::ftoz;
use crate::shade_cache::ModelSrc;
use crate::state::State;

pub(super) fn world_verts(state: &State, entity: &flecs_ecs::prelude::Entity) -> Vec<[f32; 3]> {
    world_verts_by(state, entity, [0.0; 3])
}

/// [`world_verts`] slid by `by` (shown frame) - the handle drag's preview
/// (#1198 b): display only, the world is untouched until the release.
fn world_verts_by(state: &State, entity: &flecs_ecs::prelude::Entity, by: [f64; 3]) -> Vec<[f32; 3]> {
    model_src(state, entity, by).map(|src| (*src.verts).clone()).unwrap_or_default()
}

/// The facets, the shown pose and the cached world vertices of a body
/// (#2834: the shade caches key on the first two).
pub(super) fn model_src(state: &State, entity: &flecs_ecs::prelude::Entity, by: [f64; 3]) -> Option<ModelSrc> {
    let tris = model::facets_of(&state.world, entity)?;
    // #326: the part where the active setup holds it.
    let mut loc = camcraft_app::setup_align::view_pose(&state.world, entity).unwrap_or_default();
    loc.pos = Point3::new(loc.pos.x + by[0], loc.pos.y + by[1], loc.pos.z + by[2]);
    // #2047: a ring drag's preview turns the shown pose about the centre.
    if state.body_turn.is_some()
        && let Some(turn) = model::name_of(&state.world, entity).and_then(|n| crate::turn_drag::preview_turn(state, &n))
    {
        loc = camcraft_app::setup_align::compose(turn, loc);
    }
    // #2201: keyed by the facet allocation and the shown pose, so an idle
    // frame transforms nothing again.
    let pose = crate::frame_cache::pose_bits(&loc);
    let cached = state.frame_cache.world_verts_cached(&tris, pose, || {
        let mut verts = Vec::with_capacity(tris.len() * 3);
        for t in tris.iter() {
            for p in [t.a, t.b, t.c] {
                let p = transform_point(p, loc.pos, loc.rot);
                verts.push([ftoz(p.x), ftoz(p.y), ftoz(p.z)]);
            }
        }
        verts
    });
    Some(ModelSrc { facets: tris, pose, verts: cached })
}
