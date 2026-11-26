//! #917: a mesh that knows its closed solids (one facet range per solid,
//! as a STEP file with several solids gives them) slices two solids that
//! share a wall line as their union; the soup alone stays refused, an
//! inward solid keeps the refusal, and bad ranges are input errors.
//!
//! Gate map: covers camcraft_geometry::geometry::mesh_solids and
//! camcraft_geometry::geometry::slice (`slice_of` in `slice_bodies`).
//!
//! Provenance: hand-computable boxes; the section of a union is the union
//! of the sections (regularized set operations, Requicha 1980).

use camcraft_geometry::geo2d::{Point2, Region};
use camcraft_geometry::geometry::slice::{self, SliceError, slice_of};
use camcraft_geometry::geometry::{Mesh, Point3, Triangle, cube};

fn boxed(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> Vec<Triangle> {
    cube(Point3::new(x0, y0, z0), Point3::new(x1, y1, z1)).facets.to_vec()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * b.abs().max(1.0)
}

fn non_manifold(r: &Result<Region, SliceError>) -> bool {
    matches!(r, Err(SliceError::NonManifold { .. }))
}

/// Two blocks that share one vertical edge (the same corner vertices),
/// as the solids of a STEP file do when the reader samples the shared
/// edge the same way for both.
fn welded() -> (Vec<Triangle>, usize) {
    let mut f = boxed(0.0, 0.0, 0.0, 10.0, 10.0, 5.0);
    let n = f.len();
    f.extend(boxed(10.0, 10.0, 0.0, 20.0, 20.0, 5.0));
    (f, n)
}

#[test]
fn known_solids_on_a_shared_edge_slice_as_their_union() {
    let (f, n) = welded();
    assert!(non_manifold(&slice::slice(&f, 2.5)), "the soup alone stays refused");
    let mesh = Mesh::from_solids(f.clone(), vec![0..n, n..f.len()]).expect("ranges tile the facets");
    assert_eq!(mesh.solid_ranges(), &[0..n, n..f.len()]);
    let r = slice_of(&mesh.facets, mesh.solid_ranges(), 2.5).unwrap_or_else(|e| panic!("{e}"));
    assert!(close(r.area(), 200.0), "{}", r.area());
    assert!(r.contains(Point2::new(5.0, 5.0)) && r.contains(Point2::new(15.0, 15.0)));
    assert!(!r.contains(Point2::new(15.0, 5.0)), "the open quadrant stays open");
}

#[test]
fn known_solids_on_a_shared_wall_slice_as_one_outline() {
    let mut f = boxed(0.0, 0.0, 0.0, 10.0, 10.0, 5.0);
    let n = f.len();
    f.extend(boxed(10.0, 0.0, 0.0, 20.0, 10.0, 5.0));
    let r = slice_of(&f, &[0..n, n..f.len()], 2.5).unwrap_or_else(|e| panic!("{e}"));
    assert!(close(r.area(), 200.0), "{}", r.area());
    assert!(r.contains(Point2::new(10.0, 5.0)), "the shared wall is interior");
}

#[test]
fn an_inward_solid_keeps_the_refusal() {
    let (mut f, n) = welded();
    for t in f.iter_mut().skip(n) {
        *t = Triangle::new(t.a, t.c, t.b);
    }
    let r = slice_of(&f, &[0..n, n..f.len()], 2.5);
    assert!(non_manifold(&r), "{:?}", r.map(|r| r.area()));
}

#[test]
fn fewer_than_two_solids_is_the_plain_slice() {
    let (f, _) = welded();
    let whole = std::iter::once(0..f.len()).collect::<Vec<_>>();
    for bodies in [Vec::new(), whole.clone()] {
        assert!(non_manifold(&slice_of(&f, &bodies, 2.5)), "{bodies:?}");
    }
    let one = Mesh::from_solids(f.clone(), whole).expect("one range");
    assert!(one.solid_ranges().is_empty(), "one solid is the plain soup");
    assert!(Mesh::from_facets(f).solid_ranges().is_empty());
}

#[test]
fn ranges_that_do_not_tile_the_facets_are_refused() {
    let (f, n) = welded();
    let len = f.len();
    for bad in [
        vec![0..n, n + 1..len],
        vec![0..n + 1, n..len],
        vec![0..n, n..len - 1],
        vec![0..0, 0..len],
        vec![0..n, n..len + 1],
    ] {
        let e = Mesh::from_solids(f.clone(), bad.clone());
        assert!(e.is_err(), "{bad:?} must be refused");
    }
    let r = slice_of(&f, &[0..n, n..len + 5], 2.5);
    assert!(matches!(r, Err(SliceError::Invalid(_))), "{:?}", r.map(|r| r.area()));
}
