//! The caches of the ground shadow mesh and of the occlusion values (#2834;
//! crates/ui/AGENTS.md "Frame cost"). Each key is the input itself:
//!
//! - shadow: the facet allocation and the shown pose of each model body
//!   (the key of `FrameCache::world_verts_cached`), and the floor height;
//! - occlusion: the facet allocation of one body. The values are rigid, so
//!   a moved body keeps them.
//!
//! No document counter is part of a key. A switch that is off, and the
//! wireframe mode, build nothing and read nothing.
//!
//! Provenance: our own per-frame scene cache; no vendor material.

use std::sync::{Arc, Mutex, Weak};

use camcraft_app::geometry::Triangle;

use crate::ground_shadow::ShadowVertex;
use crate::shade_pass::Shade;
use crate::state::State;
use crate::viewport::GpuPart;

/// One drawn model body: its facets, its shown pose and the world vertices
/// that the scene uploads for it.
pub(crate) struct ModelSrc {
    pub(crate) facets: Arc<[Triangle]>,
    pub(crate) pose: [u64; 7],
    pub(crate) verts: Arc<Vec<[f32; 3]>>,
}

/// The facet allocation of a body, held weakly: a replaced or released mesh
/// never matches again.
struct FacetKey {
    facets: Weak<[Triangle]>,
    len: usize,
}

impl FacetKey {
    fn of(facets: &Arc<[Triangle]>) -> Self {
        FacetKey { facets: Arc::downgrade(facets), len: facets.len() }
    }

    fn is(&self, facets: &Arc<[Triangle]>) -> bool {
        self.facets.strong_count() > 0
            && std::ptr::eq(self.facets.as_ptr(), Arc::as_ptr(facets))
            && self.len == facets.len()
    }
}

struct ShadowEntry {
    bodies: Vec<(FacetKey, [u64; 7])>,
    floor: u32,
    mesh: Arc<Vec<ShadowVertex>>,
}

/// The joined occlusion values of one scene; `parts` holds the values of
/// each body alive, so that a pointer compare is a sound key.
struct JoinedEntry {
    parts: Vec<Arc<Vec<f32>>>,
    tail: usize,
    values: Arc<Vec<f32>>,
}

#[derive(Default)]
struct Inner {
    shadow: Option<ShadowEntry>,
    bodies: Vec<(FacetKey, Arc<Vec<f32>>)>,
    joined: Option<JoinedEntry>,
    shadow_builds: u64,
    occlusion_builds: u64,
}

/// Interior-mutable shade caches, held on `State` through `FrameCache`.
#[derive(Default)]
pub(crate) struct ShadeCache {
    inner: Mutex<Inner>,
}

impl ShadeCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The shadow mesh of `bodies` on the floor at `floor`.
    fn shadow(&self, bodies: &[ModelSrc], floor: f32) -> Arc<Vec<ShadowVertex>> {
        let mut inner = self.lock();
        if let Some(e) = &inner.shadow
            && e.floor == floor.to_bits()
            && e.bodies.len() == bodies.len()
            && e.bodies.iter().zip(bodies).all(|((key, pose), b)| key.is(&b.facets) && *pose == b.pose)
        {
            return Arc::clone(&e.mesh);
        }
        let soups: Vec<&[[f32; 3]]> = bodies.iter().map(|b| b.verts.as_slice()).collect();
        let mesh = Arc::new(crate::ground_shadow::mesh(&soups, floor));
        inner.shadow_builds = inner.shadow_builds.saturating_add(1);
        inner.shadow = Some(ShadowEntry {
            bodies: bodies.iter().map(|b| (FacetKey::of(&b.facets), b.pose)).collect(),
            floor: floor.to_bits(),
            mesh: Arc::clone(&mesh),
        });
        mesh
    }

    /// One occlusion value for each model vertex: the bodies in order, then
    /// `tail` open values for the model vertices that are not of a body.
    fn occlusion(&self, bodies: &[ModelSrc], tail: usize) -> Arc<Vec<f32>> {
        let mut inner = self.lock();
        inner.bodies.retain(|(key, _)| key.facets.strong_count() > 0);
        let mut parts = Vec::with_capacity(bodies.len());
        for b in bodies {
            if let Some((_, values)) = inner.bodies.iter().find(|(key, _)| key.is(&b.facets)) {
                parts.push(Arc::clone(values));
                continue;
            }
            let values = Arc::new(crate::occlusion::vertex_occlusion(&b.facets));
            inner.occlusion_builds = inner.occlusion_builds.saturating_add(1);
            inner.bodies.push((FacetKey::of(&b.facets), Arc::clone(&values)));
            parts.push(values);
        }
        if let Some(e) = &inner.joined
            && e.tail == tail
            && e.parts.len() == parts.len()
            && e.parts.iter().zip(&parts).all(|(a, b)| Arc::ptr_eq(a, b))
        {
            return Arc::clone(&e.values);
        }
        let mut joined: Vec<f32> = parts.iter().flat_map(|p| p.iter().copied()).collect();
        joined.resize(joined.len() + tail, 1.0);
        let values = Arc::new(joined);
        inner.joined = Some(JoinedEntry { parts, tail, values: Arc::clone(&values) });
        values
    }

    /// How many shadow meshes this cache built (the counter pin).
    #[cfg(test)]
    pub(crate) fn shadow_builds(&self) -> u64 {
        self.lock().shadow_builds
    }

    /// How many bodies this cache built occlusion values for (the counter pin).
    #[cfg(test)]
    pub(crate) fn occlusion_builds(&self) -> u64 {
        self.lock().occlusion_builds
    }
}

/// The shade data of the scene whose model bodies are `bodies` and whose
/// parts are `parts`. Empty for each switch that is off and in wireframe.
pub(crate) fn collect(state: &State, parts: &[GpuPart], bodies: &[ModelSrc]) -> Shade {
    let fx = state.effects();
    let mut shade = Shade::default();
    // Both wireframe modes (#2636 added "Wireframe, see-through") draw no
    // lit face and no shadow of one: build nothing.
    if state.display.is_wireframe() {
        return shade;
    }
    let cache = &state.frame_cache.shade;
    if fx.ground_shadow {
        let _span = crate::frame_cost::span("scene/shade.ground_shadow");
        shade.shadow = cache.shadow(bodies, crate::interact::ftoz(crate::floor::floor_z(state)));
    }
    if fx.ambient_occlusion {
        let _span = crate::frame_cost::span("scene/shade.occlusion");
        // The model vertices in upload order are the bodies, then the rest
        // (the simulation tool body): the rest reads open.
        let model: usize = parts.iter().filter(|p| !p.stock && !p.fill).map(|p| p.verts.len()).sum();
        let of_bodies: usize = bodies.iter().map(|b| b.verts.len()).sum();
        shade.occlusion = cache.occlusion(bodies, model.saturating_sub(of_bodies));
    }
    shade
}

#[cfg(test)]
#[path = "shade_cache_tests.rs"]
mod tests;
