//! #1407: a soup of several closed solids that touch or overlap slices
//! as their union; genuine non-manifold input - solids welded along a
//! shared edge, an inward cavity shell - is still refused by name.
//!
//! Gate map: covers camcraft_geometry::geometry::slice (the union
//! fallback in `slice_bodies`).
//!
//! Provenance: hand-computable boxes; the section of a union is the union
//! of the sections (regularized set operations, Requicha 1980).

use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::geometry::slice::{self, SliceError};
use camcraft_geometry::geometry::{cube, Point3, Triangle};

fn boxed(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> Vec<Triangle> {
    cube(Point3::new(x0, y0, z0), Point3::new(x1, y1, z1)).facets.to_vec()
}

fn inverted(f: &[Triangle]) -> Vec<Triangle> {
    f.iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * b.abs().max(1.0)
}

fn non_manifold(r: Result<camcraft_geometry::geo2d::Region, SliceError>) -> bool {
    matches!(r, Err(SliceError::NonManifold { .. }))
}

/// The issue's shape: two solids stacked with an overlapping Z band whose
/// walls stand on the same vertical lines. Every plane in the band meets
/// both outlines at the same corner points, 4 chords each.
#[test]
fn stacked_solids_sharing_their_walls_slice_as_their_union() {
    let mut f = boxed(0.0, 0.0, 0.0, 10.0, 10.0, 3.0);
    f.extend(boxed(0.0, 0.0, 2.0, 10.0, 10.0, 5.0));
    for z in [1.0, 2.5, 4.0] {
        let r = slice::slice(&f, z).unwrap_or_else(|e| panic!("z = {z}: {e}"));
        assert!(close(r.area(), 100.0), "z = {z}: {}", r.area());
        assert!(r.contains(Point2::new(5.0, 5.0)));
    }
}

/// Two solids butted face to face (opposite normals on the shared wall),
/// their walls ending on the same vertical lines: one 20 x 10 section.
#[test]
fn solids_touching_along_a_face_slice_as_one_outline() {
    let mut f = boxed(0.0, 0.0, 0.0, 10.0, 10.0, 3.0);
    f.extend(boxed(10.0, 0.0, 1.0, 20.0, 10.0, 4.0));
    let r = slice::slice(&f, 2.0).expect("the union sections");
    assert!(close(r.area(), 200.0), "{}", r.area());
    assert!(r.contains(Point2::new(10.0, 5.0)), "the shared wall is interior");
    assert!(close(slice::project(&f, 8).expect("projects").area(), 200.0));
}

/// Real non-manifold geometry: two blocks WELDED along a vertical edge
/// (the very same corner vertices). They share that edge four ways, so
/// they are one non-manifold shell - refused, never guessed apart.
#[test]
fn solids_welded_along_a_shared_edge_are_still_refused() {
    let mut f = boxed(0.0, 0.0, 0.0, 10.0, 10.0, 5.0);
    f.extend(boxed(10.0, 10.0, 0.0, 20.0, 20.0, 5.0));
    let e = slice::slice(&f, 2.5);
    assert!(non_manifold(e.clone()), "{e:?}");
    let msg = e.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(msg.contains("non-manifold") && msg.contains("repair the mesh"), "{msg}");
    assert!(!msg.contains("chords meet"), "the old wording is gone: {msg}");
}

/// A cavity is an inward shell: sliced on its own it reads as material,
/// so a union over it would fill the hole. Any inward shell keeps the
/// refusal (the outward guard).
#[test]
fn a_soup_with_an_inward_cavity_shell_is_refused_not_filled() {
    let mut f = boxed(0.0, 0.0, 0.0, 30.0, 30.0, 5.0);
    f.extend(inverted(&boxed(10.0, 10.0, 1.0, 20.0, 20.0, 4.0)));
    // A third block touching the first along its corner line (collinear
    // vertical edges, different ends: not welded) makes the plane 4-way.
    f.extend(boxed(30.0, 30.0, 1.0, 40.0, 40.0, 4.0));
    assert!(non_manifold(slice::slice(&f, 2.5)), "{:?}", slice::slice(&f, 2.5).map(|r| r.area()));
    // Without the third block the cavity slices as it always did.
    let plain = slice::slice(&f[..24], 2.5).expect("box with cavity");
    assert!(close(plain.area(), 800.0), "{}", plain.area());
}

/// An open solid in the soup is no solid: the closed guard refuses.
#[test]
fn a_soup_with_an_open_solid_is_refused() {
    let mut f = boxed(0.0, 0.0, 0.0, 10.0, 10.0, 3.0);
    let mut lid = boxed(0.0, 0.0, 2.0, 10.0, 10.0, 5.0);
    // Drop the top cap's two facets (z = 5): open, but the plane at 2.5
    // still closes, so only the body test can catch it.
    lid.retain(|t| !(t.a.z == 5.0 && t.b.z == 5.0 && t.c.z == 5.0));
    assert_eq!(lid.len(), 10);
    f.extend(lid);
    assert!(non_manifold(slice::slice(&f, 2.5)));
}

/// Imported seams drift by nanometres: a corner copy 50 nm off still
/// welds into the same body, so the union holds.
#[test]
fn nanometre_seam_drift_still_reads_closed_solids() {
    let mut f = boxed(0.0, 0.0, 0.0, 10.0, 10.0, 3.0);
    let mut lid = boxed(0.0, 0.0, 2.0, 10.0, 10.0, 5.0);
    let top = Point3::new(10.0, 10.0, 5.0);
    let drift = Point3::new(10.0 + 5e-5, 10.0, 5.0);
    let mut moved = 0;
    for t in lid.iter_mut().take(6) {
        for p in [&mut t.a, &mut t.b, &mut t.c] {
            if *p == top {
                *p = drift;
                moved += 1;
            }
        }
    }
    assert!(moved > 0, "the corner copy moved");
    f.extend(lid);
    let r = slice::slice(&f, 2.5).expect("drifted seam still a closed solid");
    assert!(close(r.area(), 100.0), "{}", r.area());
}
