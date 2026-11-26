//! Counter pins of the shadow cache and the occlusion cache (#2834;
//! crates/ui/AGENTS.md "Frame cost"): a still frame builds no shadow data
//! and no occlusion data; a changed input builds again; a switch that is
//! off builds nothing.
//! Provenance: our own tests over our own caches.
//! Gate map: covers ui::shade_cache, ui::egui_app::world.

use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;

use crate::egui_app::world;
use crate::state::State;

fn block(st: &State, name: &str, lo: f64) -> flecs_ecs::prelude::Entity {
    let part = model::spawn(&st.world, name);
    model::set_facets(&st.world, &part, cube(Point3::new(lo, 0.0, 0.0), Point3::new(lo + 10.0, 10.0, 10.0)).facets);
    part
}

fn builds(st: &State) -> (u64, u64) {
    (st.frame_cache.shade.shadow_builds(), st.frame_cache.shade.occlusion_builds())
}

#[test]
fn switches_that_are_off_build_no_shade_data() {
    let st = State::new();
    block(&st, "Part", 0.0);
    let s = world::collect(&st);
    assert!(s.shade.shadow.is_empty() && s.shade.occlusion.is_empty());
    assert_eq!(builds(&st), (0, 0), "Performance costs nothing");
}

/// Both wireframe modes draw no lit face and no shadow, so the switches
/// build nothing there. "Wireframe, see-through" (#2636) is a wireframe mode.
#[test]
fn the_wireframe_modes_build_no_shade_data() {
    use crate::state::DisplayMode;
    let mut st = State::new();
    block(&st, "Part", 0.0);
    st.ground_shadow = true;
    st.ambient_occlusion = true;
    for mode in [DisplayMode::Wireframe, DisplayMode::WireframeSeeThrough] {
        st.display = mode;
        let s = world::collect(&st);
        assert!(s.shade.shadow.is_empty() && s.shade.occlusion.is_empty(), "{mode:?}");
    }
    assert_eq!(builds(&st), (0, 0), "a wireframe mode builds no shade data");
    st.display = DisplayMode::ShadedEdges;
    let _ = world::collect(&st);
    assert_eq!(builds(&st), (1, 1), "a shaded mode builds the two");
}

#[test]
fn a_still_frame_builds_no_shadow_data_and_a_moved_body_builds_again() {
    let mut st = State::new();
    let part = block(&st, "Part", 0.0);
    st.ground_shadow = true;
    let first = world::collect(&st);
    assert!(!first.shade.shadow.is_empty(), "the block casts a shadow");
    assert!(first.shade.occlusion.is_empty(), "the occlusion switch is off");
    assert_eq!(builds(&st), (1, 0), "the first frame builds the shadow one time");
    for _ in 0..8 {
        let s = world::collect(&st);
        assert!(std::sync::Arc::ptr_eq(&s.shade.shadow, &first.shade.shadow), "a still frame reads the cache");
    }
    assert_eq!(builds(&st), (1, 0), "a still frame builds no shadow data");
    // An orbit is not a geometry change.
    st.cam.orbit(40.0, 10.0);
    let _ = world::collect(&st);
    assert_eq!(builds(&st), (1, 0), "a camera move builds no shadow data");
    // A moved body is a changed input.
    let loc = camcraft_app::schema::Location { pos: Point3::new(25.0, 0.0, 0.0), ..Default::default() };
    model::set_transform(&st.world, &part, loc);
    let moved = world::collect(&st);
    assert_eq!(builds(&st), (2, 0), "a moved body builds the shadow again");
    let min_x = |s: &world::Scene| s.shade.shadow.iter().map(|v| v[0]).fold(f32::INFINITY, f32::min);
    assert!(min_x(&moved) > min_x(&first) + 20.0, "the shadow follows the body");
}

#[test]
fn a_still_frame_builds_no_occlusion_data_and_new_facets_build_again() {
    let mut st = State::new();
    let part = block(&st, "Part", 0.0);
    st.ambient_occlusion = true;
    let first = world::collect(&st);
    assert_eq!(first.shade.occlusion.len(), first.parts[0].verts.len(), "one value for each model vertex");
    assert!(first.shade.shadow.is_empty(), "the shadow switch is off");
    assert_eq!(builds(&st), (0, 1), "the first frame builds the occlusion one time");
    for _ in 0..8 {
        let s = world::collect(&st);
        assert!(std::sync::Arc::ptr_eq(&s.shade.occlusion, &first.shade.occlusion), "a still frame reads the cache");
    }
    assert_eq!(builds(&st), (0, 1), "a still frame builds no occlusion data");
    // The occlusion of a body does not change with its pose.
    let loc = camcraft_app::schema::Location { pos: Point3::new(25.0, 0.0, 0.0), ..Default::default() };
    model::set_transform(&st.world, &part, loc);
    let _ = world::collect(&st);
    assert_eq!(builds(&st), (0, 1), "a moved body keeps its occlusion data");
    // New facets are a changed input.
    model::set_facets(&st.world, &part, cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 10.0, 10.0)).facets);
    let _ = world::collect(&st);
    assert_eq!(builds(&st), (0, 2), "new facets build the occlusion again");
    // A second body builds its own data only.
    block(&st, "Other", 40.0);
    let two = world::collect(&st);
    assert_eq!(builds(&st), (0, 3), "the first body is not built again");
    let model_verts: usize = two.parts.iter().filter(|p| !p.stock && !p.fill).map(|p| p.verts.len()).sum();
    assert_eq!(two.shade.occlusion.len(), model_verts, "the values follow the model vertices in upload order");
}

#[test]
fn the_scene_digest_follows_the_shade_data() {
    let mut st = State::new();
    block(&st, "Part", 0.0);
    let off = world::collect(&st).digest();
    st.ground_shadow = true;
    let shadow = world::collect(&st).digest();
    assert_ne!(off, shadow, "a new shadow is a new upload");
    st.ambient_occlusion = true;
    assert_ne!(shadow, world::collect(&st).digest(), "new occlusion data is a new upload");
}
