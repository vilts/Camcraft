//! #1407: the 3-D ladder's islands law names a model it cannot section
//! as the model's fault - never "invalid machining parameters".
//!
//! Gate map: covers camcraft_motion::machining3d (rest's slice-error map).
//!
//! Provenance: hand-built boxes - two closed blocks welded along one
//! vertical edge, a non-manifold shell by construction.

use camcraft_motion::geometry::{cube, Point3, Triangle};
use camcraft_motion::machining2d::GenError;
use camcraft_motion::machining3d::material_above;

#[test]
fn a_welded_non_manifold_soup_is_refused_as_a_model_fault() {
    let mut f: Vec<Triangle> = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 5.0)).facets.to_vec();
    f.extend(cube(Point3::new(10.0, 10.0, 0.0), Point3::new(20.0, 20.0, 5.0)).facets.iter().copied());
    let e = material_above(&f, 2.5).expect_err("a non-manifold soup has no islands to read");
    assert!(matches!(e, GenError::Model(_)), "{e:?}");
    let msg = e.to_string();
    assert!(msg.starts_with("the source model cannot be sliced") && msg.contains("non-manifold"), "{msg}");
}

#[test]
fn stacked_closed_solids_read_their_union_as_islands() {
    let mut f: Vec<Triangle> = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 3.0)).facets.to_vec();
    f.extend(cube(Point3::new(0.0, 0.0, 2.0), Point3::new(10.0, 10.0, 5.0)).facets.iter().copied());
    let r = material_above(&f, 2.5).expect("the union's islands");
    assert!((r.area() - 100.0).abs() < 1e-6, "{}", r.area());
}
