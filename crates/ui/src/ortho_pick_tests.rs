//! #2375: an orthographic pick ray starts at the ortho near plane, not at
//! the eye plane. The ortho eye sits only `ORTHO_EYE_DIST` in front of the
//! target, and the near plane reaches far behind it (#1689), so a face
//! nearer the viewer than the eye plane is drawn - and must be pickable.
//!
//! Provenance: our own tests over our own camera and pick law; no vendor
//! material consulted.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::pick;

use crate::state::State;
use crate::viewport::{Camera, ORTHO_EYE_DIST, ProjectionMode};

/// Top view in ortho, looking down -Z at the origin: the eye plane is Z 100.
fn top_ortho() -> Camera {
    let cam = Camera { target: [0.0, 0.0, 0.0], dist: 50.0, projection: ProjectionMode::Orthographic, ..Camera::default() }
        .toward([0.0, 0.0, 1.0]);
    assert_eq!(cam.projection, ProjectionMode::Orthographic);
    assert!((cam.eye()[2] - ORTHO_EYE_DIST).abs() < 1e-9, "the eye plane is Z {}", cam.eye()[2]);
    cam
}

#[test]
fn an_ortho_pick_hits_the_top_face_that_stands_above_the_eye_plane() {
    let mut state = State::new();
    // A 300 mm column: its top face is 200 mm nearer the viewer than the
    // eye plane, and well inside the near plane, so the view draws it.
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("Column", cube(Point3::new(-10.0, -10.0, 0.0), Point3::new(10.0, 10.0, 300.0))),
    );
    let cam = top_ortho();
    assert!(300.0 - ORTHO_EYE_DIST < cam.ortho_behind_eye(), "the top face is inside the drawn box");
    let (origin, dir) = cam.ray_through([0.0, 0.0], 1.5).expect("finite ray");
    let (name, _, at) = pick::first_hit_f(&state.world, origin, dir, |_| false).expect("the click finds the column");
    assert_eq!(name, "Column");
    assert!((at[2] - 300.0).abs() < 1e-6, "the nearest face is the top at Z 300, not {at:?}");
}

#[test]
fn the_ortho_ray_starts_on_the_near_plane_and_keeps_its_line() {
    let cam = top_ortho();
    let e = cam.eye();
    for ndc in [[0.0f32, 0.0], [0.6, -0.4], [-0.9, 0.8]] {
        let (o, d) = cam.ray_through(ndc, 1.5).expect("finite ray");
        // The view axis is -Z here: the origin is `ortho_behind_eye` above the eye plane.
        assert!((o[2] - (e[2] + cam.ortho_behind_eye())).abs() < 1e-9, "origin {o:?} for {ndc:?}");
        assert!(d[0].abs() < 1e-12 && d[1].abs() < 1e-12 && (d[2] + 1.0).abs() < 1e-12, "parallel rays: {d:?}");
    }
    // The near plane in the matrix and the ray agree: a point on it maps to clip depth 0.
    let m = cam.mvp(1.5);
    let z_near = e[2] + cam.ortho_behind_eye();
    let depth = m[2][2] as f64 * z_near + m[3][2] as f64;
    assert!(depth.abs() < 1e-3, "the near plane is the ray's start: clip z {depth}");
}
