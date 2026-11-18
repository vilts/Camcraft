//! The machined stock in the viewer (M13.1 owed): the mesh the field
//! view extracts (`stock_export::MeshCache`, the STL export's cache)
//! stands in the stock slot of the 3D view, and the hover's stock hit
//! is read against it - so what the pointer reports is what the eye
//! sees, not the uncut box it replaced. Provenance: our own code over
//! the M7.2 D cache; nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **One extraction per view, never per frame.** `prime` extracts
//!   when the whole-program view lands (`actions::walk_whole_program`),
//!   and says a refusal once; every frame after reads `mesh`, which
//!   only ever returns what is already cached.
//! - **The transport reads nothing here.** A playback view changes
//!   every frame; its triangles would cost a marching pass a frame.
//!   While the transport is up the traced eye and the sheet stand, as
//!   before - the mesh is the whole program's, transport down.
//! - **The eye still paints over it.** The GPU eye (`trace::eye`)
//!   replaces the stock slot when it traces; the mesh is what stands
//!   there when it does not - no eye, a refused march, a headless run.
//! - **Judged once, by the eye's law (#25).** `prime` classifies the
//!   mesh's facets with the extraction (`MeshCache::classify`, the
//!   eye's lattice and gate); the scene reads the cached classes, so
//!   the mesh paints the same clean / rest / gouge / unjudged legend.
//! - **The hovered facet lights (#25).** A stock hover answered by the
//!   mesh keeps the triangle it hit (`State::hover_mesh`, view state,
//!   never a command); `hover_facet_uv` projects it for the shell's
//!   draw list - nothing is re-uploaded when the pointer moves.

use std::sync::Arc;

use camcraft_app::geometry::Triangle;
use camcraft_app::{model, pick};

use crate::state::State;

/// The entity pick's answer: entity name, its facet, the hit point.
pub(crate) type Hit = (String, usize, [f64; 3]);

/// Extract the whole-program view's mesh once (a no-op when it is
/// already the cached view) and say a refusal on the status line.
pub(crate) fn prime(state: &mut State) {
    if state.sim.is_some() {
        return;
    }
    let Some(Ok(fv)) = &state.field_view else { return };
    let fv = Arc::clone(fv);
    if state.stock_mesh.viewed(&fv).is_none()
        && let Err(err) = state.stock_mesh.get(&fv)
    {
        state.say_once(&format!("the cut stock cannot be drawn, the uncut stock shows: {err}"));
        return;
    }
    state.stock_mesh.classify(&fv);
}

/// The mesh the viewer shows this frame: the whole program's extracted
/// stock, transport down and already extracted; `None` otherwise (the
/// slot then draws what it drew before - the judged sheet or the box).
#[must_use]
pub(crate) fn mesh(state: &State) -> Option<Arc<Vec<Triangle>>> {
    if state.sim.is_some() {
        return None;
    }
    match &state.field_view {
        Some(Ok(fv)) => state.stock_mesh.viewed(fv),
        _ => None,
    }
}

/// The mesh's deviation classes as the scene's per-vertex buffer: each
/// facet's class three times, in the mesh's own order. Empty (plain
/// material) when the classes are not judged yet or do not count the
/// mesh's facets - a buffer that disagrees with its mesh is not drawn.
#[must_use]
pub(crate) fn vertex_classes(state: &State, facets: usize) -> Vec<f32> {
    let Some(Ok(fv)) = &state.field_view else { return Vec::new() };
    match state.stock_mesh.viewed_classes(fv) {
        Some(c) if c.len() == facets => c.iter().flat_map(|&k| [k, k, k]).collect(),
        _ => Vec::new(),
    }
}

/// The hovered mesh facet in viewport uv (0..1, like
/// `interact::hover_features`): `None` without a hovered facet, when
/// the mesh no longer stands, or when any corner projects behind the
/// camera - a highlight never draws a triangle the click cannot reach.
#[must_use]
pub(crate) fn hover_facet_uv(state: &State, aspect: f64) -> Option<[[f32; 2]; 3]> {
    let t = state.hover_mesh?;
    mesh(state)?;
    let mvp = state.cam.mvp(aspect);
    let uv = |p: camcraft_app::geometry::Point3| crate::interact::project_uv(&mvp, [p.x, p.y, p.z]);
    Some([uv(t.a)?, uv(t.b)?, uv(t.c)?])
}

/// The entity pick's answer, read against the machined mesh when one
/// stands in the stock slot: a model hit is kept (models win at any
/// depth, as in `pick::first_hit_f`); a stock hit is re-asked of the
/// mesh - the box the ray crossed is not drawn, so a ray through
/// machined-away air reports nothing, and a ray onto the remaining
/// material reports the stock with no entity facet (`usize::MAX`: the
/// mesh is no entity, so no box facet may light up for it).
#[cfg(test)]
pub(crate) fn filter_hit(state: &State, origin: [f64; 3], dir: [f64; 3], hit: Option<Hit>) -> Option<Hit> {
    filter_hit_facet(state, origin, dir, hit).0
}

/// `filter_hit`, plus the mesh facet the ray landed on when the mesh
/// answered it (the hover highlight's triangle, #25).
pub(crate) fn filter_hit_facet(
    state: &State,
    origin: [f64; 3],
    dir: [f64; 3],
    hit: Option<Hit>,
) -> (Option<Hit>, Option<Triangle>) {
    if state.active_stock_hidden() {
        return (hit.filter(|(name, _, _)| !state.hidden_stocks.contains(name)), None);
    }
    let Some(mesh) = mesh(state) else { return (hit, None) };
    let on_stock = hit.as_ref().is_none_or(|(name, _, _)| {
        model::find_by_name(&state.world, name).is_some_and(|e| model::is_stock(&state.world, &e))
    });
    if !on_stock {
        return (hit, None);
    }
    let Some((k, p)) = pick::soup_hit(&mesh, origin, dir) else { return (None, None) };
    let name = hit.map(|(name, _, _)| name).unwrap_or_else(|| {
        camcraft_app::setup::stock_entity_name(&state.world, camcraft_app::setup::active(&state.world).as_deref())
    });
    (Some((name, usize::MAX, p)), mesh.get(k).copied())
}
