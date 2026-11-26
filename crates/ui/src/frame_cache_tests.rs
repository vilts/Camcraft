//! Pinning tests for `FrameCache::world_verts_cached` source allocation lifetime
//! and `world::collect` vertex freshness after facet replacement, plus the
//! counter pin: a still frame transforms no vertex again.
//! Provenance: our own unit tests for PR #2256 review finding 1.

use std::sync::Arc;

use camcraft_app::geometry::{Point3, Triangle, cube};
use camcraft_app::model;

use super::{FrameCache, lock_inner};
use crate::egui_app::world;
use crate::state::State;

fn single_tri(x: f64) -> Arc<[Triangle]> {
    Arc::from(vec![Triangle::new(
        Point3::new(x, 0.0, 0.0),
        Point3::new(x, 1.0, 0.0),
        Point3::new(x, 0.0, 1.0),
    )])
}

#[test]
fn world_verts_cached_does_not_reuse_stale_entry_after_source_release() {
    let cache = FrameCache::default();
    let pose = [0u64; 7];
    let mut builds = 0usize;

    let first = single_tri(10.0);
    let v1 = cache.world_verts_cached(&first, pose, || {
        builds += 1;
        vec![[first[0].a.x as f32, 0.0, 0.0]]
    });
    assert_eq!(v1[0][0], 10.0);
    assert_eq!(builds, 1);

    let v1_hit = cache.world_verts_cached(&first, pose, || {
        builds += 1;
        vec![[999.0, 0.0, 0.0]]
    });
    assert_eq!(v1_hit[0][0], 10.0);
    assert_eq!(builds, 1, "live source allocation must hit the cache");

    drop(first);

    for step in 0..64 {
        let replacement = single_tri(20.0);
        let v2 = cache.world_verts_cached(&replacement, pose, || {
            builds += 1;
            vec![[replacement[0].a.x as f32, 0.0, 0.0]]
        });
        assert_eq!(
            v2[0][0], 20.0,
            "step {step}: dropped source allocation must not return stale vertices"
        );
        assert_eq!(
            lock_inner(&cache).world_verts.len(),
            1,
            "step {step}: dead source entries must be pruned"
        );
    }
}

#[test]
fn world_collect_rebuilds_vertices_when_model_facets_are_replaced() {
    let mut st = State::new();
    let part = model::spawn(&st.world, "Part");
    let first = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 10.0)).facets;
    let tri_count = first.len();
    model::set_facets(&st.world, &part, first);
    st.doc_version += 1;

    let s1 = world::collect(&st);
    assert_eq!(s1.parts.len(), 1);
    let max_x1 = s1.parts[0]
        .verts
        .iter()
        .map(|v| v[0])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((max_x1 - 10.0).abs() < 1e-4, "expected initial max_x 10.0, got {max_x1}");

    for step in 0..16 {
        let lo_x = 20.0 + f64::from(step) * 5.0;
        let next = cube(Point3::new(lo_x, 0.0, 0.0), Point3::new(lo_x + 10.0, 10.0, 10.0)).facets;
        assert_eq!(next.len(), tri_count);
        model::set_facets(&st.world, &part, next);
        st.doc_version += 1;

        let s2 = world::collect(&st);
        let min_x2 = s2.parts[0]
            .verts
            .iter()
            .map(|v| v[0])
            .fold(f32::INFINITY, f32::min);
        assert!(
            (min_x2 - lo_x as f32).abs() < 1e-4,
            "step {step}: expected replaced min_x {lo_x}, got {min_x2}"
        );
    }
}

#[test]
fn world_collect_still_frames_transform_no_vertex_and_a_pose_change_does() {
    let st = State::new();
    let part = model::spawn(&st.world, "Part");
    model::set_facets(&st.world, &part, cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 10.0)).facets);
    let _ = world::collect(&st);
    let builds = st.frame_cache.world_verts_builds();
    assert_eq!(builds, 1, "the first frame transforms the body once");
    for _ in 0..8 {
        let _ = world::collect(&st);
    }
    assert_eq!(st.frame_cache.world_verts_builds(), builds, "a still frame transforms nothing");
    let loc = camcraft_app::schema::Location { pos: Point3::new(5.0, 0.0, 0.0), ..Default::default() };
    model::set_transform(&st.world, &part, loc);
    let s = world::collect(&st);
    assert_eq!(st.frame_cache.world_verts_builds(), builds + 1, "a moved body transforms again");
    let min_x = s.parts[0].verts.iter().map(|v| v[0]).fold(f32::INFINITY, f32::min);
    assert!((min_x - 5.0).abs() < 1e-4, "the moved vertices are drawn, got {min_x}");
}
