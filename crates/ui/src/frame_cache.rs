//! Per-`State` cache of each body's world-space vertices (#2201). An entry
//! keys on its inputs: the facet allocation (a `Weak` compared by pointer,
//! so a replaced or released mesh never hits) and the pose bits. No
//! document counter is part of the key (crates/ui/AGENTS.md, #2238).
//!
//! Provenance: our own per-frame scene cache; no external code or vendor material.

use std::sync::{Arc, Mutex, Weak};

use camcraft_app::geometry::Triangle;

const WORLD_VERTS_CAP: usize = 32;

/// The bits of a body's view pose: position, then the rotation quaternion.
#[must_use]
pub(crate) fn pose_bits(loc: &camcraft_app::schema::Location) -> [u64; 7] {
    [
        loc.pos.x.to_bits(),
        loc.pos.y.to_bits(),
        loc.pos.z.to_bits(),
        loc.rot[0].to_bits(),
        loc.rot[1].to_bits(),
        loc.rot[2].to_bits(),
        loc.rot[3].to_bits(),
    ]
}

struct WorldVertsEntry {
    facets: Weak<[Triangle]>,
    facets_len: usize,
    pose: [u64; 7],
    verts: Arc<Vec<[f32; 3]>>,
}

#[derive(Default)]
struct FrameCacheInner {
    world_verts: Vec<WorldVertsEntry>,
    builds: u64,
}

/// Interior-mutable vertex cache held on `State`.
#[derive(Default)]
pub(crate) struct FrameCache {
    inner: Mutex<FrameCacheInner>,
    /// The toolpath overlay lines and the chevron arm (#2201).
    pub(crate) paths: crate::path_overlay::PathLineCache,
    /// The DXF profile overlay lines (#1863).
    pub(crate) sketches: crate::sketch_overlay::SketchLineCache,
    /// The ground shadow mesh and the occlusion values (#2834).
    pub(crate) shade: crate::shade_cache::ShadeCache,
}

fn lock_inner(cache: &FrameCache) -> std::sync::MutexGuard<'_, FrameCacheInner> {
    cache.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl FrameCache {
    /// The cached vertices of `facets` at `pose`, or `build`'s result
    /// stored under that key. Dead entries are pruned on each call.
    pub(crate) fn world_verts_cached(
        &self,
        facets: &Arc<[Triangle]>,
        pose: [u64; 7],
        build: impl FnOnce() -> Vec<[f32; 3]>,
    ) -> Arc<Vec<[f32; 3]>> {
        let matches = |e: &WorldVertsEntry| {
            e.facets.strong_count() > 0
                && std::ptr::eq(e.facets.as_ptr(), Arc::as_ptr(facets))
                && e.facets_len == facets.len()
                && e.pose == pose
        };
        {
            let mut inner = lock_inner(self);
            inner.world_verts.retain(|e| e.facets.strong_count() > 0);
            if let Some(i) = inner.world_verts.iter().position(matches) {
                inner.world_verts[i..].rotate_left(1);
                if let Some(entry) = inner.world_verts.last() {
                    return Arc::clone(&entry.verts);
                }
            }
        }
        let verts = Arc::new(build());
        let mut inner = lock_inner(self);
        inner.builds = inner.builds.saturating_add(1);
        if let Some(i) = inner.world_verts.iter().position(matches) {
            return Arc::clone(&inner.world_verts[i].verts);
        }
        if inner.world_verts.len() >= WORLD_VERTS_CAP {
            inner.world_verts.remove(0);
        }
        inner.world_verts.push(WorldVertsEntry {
            facets: Arc::downgrade(facets),
            facets_len: facets.len(),
            pose,
            verts: Arc::clone(&verts),
        });
        verts
    }

    /// How many vertex builds this cache ran (the counter pin).
    #[cfg(test)]
    pub(crate) fn world_verts_builds(&self) -> u64 {
        lock_inner(self).builds
    }
}

#[cfg(test)]
#[path = "frame_cache_tests.rs"]
mod tests;
