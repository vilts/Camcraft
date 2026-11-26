//! Tests of the consensus pass of the axis fit (`axis_consensus.rs`,
//! issue #113): a shaft whose keyway is too large for the plain gate
//! fits on its revolved surface, while a part mostly not revolved - an
//! off-axis lump, a box - is still refused by name.
//! Provenance: our own fixtures; no vendor material.

use crate::geometry::{Point3, Triangle};
use crate::multiaxis::axis::fit_report;

const SEGS: i64 = 72;
const R: f64 = 10.0;

fn ring(j: i64, z: f64) -> Point3 {
    let th = std::f64::consts::TAU * j as f64 / SEGS as f64;
    Point3::new(R * th.cos(), R * th.sin(), z)
}

/// A D20 shaft on world Z, z -30..30, with a flat-floored keyway on +x
/// over `key_z` whose walls meet the ring at +-`half` segments and whose
/// floor stands at x = `floor_x`.
fn keyed(half: i64, key_z: (f64, f64), floor_x: f64) -> Vec<Triangle> {
    let mut f = Vec::new();
    let zs = [-30.0, key_z.0, key_z.1, 30.0];
    for j in -SEGS / 2..SEGS / 2 {
        for (k, w) in zs.windows(2).enumerate() {
            if k == 1 && j >= -half && j < half {
                continue;
            }
            let (a1, a2, b1, b2) = (ring(j, w[0]), ring(j + 1, w[0]), ring(j, w[1]), ring(j + 1, w[1]));
            f.push(Triangle::new(a1, a2, b2));
            f.push(Triangle::new(a1, b2, b1));
        }
        let (c0, c1) = (Point3::new(0.0, 0.0, -30.0), Point3::new(0.0, 0.0, 30.0));
        f.push(Triangle::new(c0, ring(j + 1, -30.0), ring(j, -30.0)));
        f.push(Triangle::new(c1, ring(j, 30.0), ring(j + 1, 30.0)));
    }
    let (xt, ys) = (ring(half, 0.0).x, ring(half, 0.0).y);
    let (z0, z1, xf) = (key_z.0, key_z.1, floor_x);
    let q = Point3::new;
    f.push(Triangle::new(q(xf, -ys, z0), q(xf, ys, z0), q(xf, ys, z1)));
    f.push(Triangle::new(q(xf, -ys, z0), q(xf, ys, z1), q(xf, -ys, z1)));
    f.push(Triangle::new(q(xf, ys, z0), q(xt, ys, z0), q(xt, ys, z1)));
    f.push(Triangle::new(q(xf, ys, z0), q(xt, ys, z1), q(xf, ys, z1)));
    f.push(Triangle::new(q(xt, -ys, z0), q(xf, -ys, z0), q(xf, -ys, z1)));
    f.push(Triangle::new(q(xt, -ys, z0), q(xf, -ys, z1), q(xt, -ys, z1)));
    f
}

/// The issue's case: a 10 mm wide, 20 mm long keyway with a deep floor
/// (x = 4, 6 mm under the surface) on a D20 shaft.
fn deep_keyed() -> Vec<Triangle> {
    keyed(6, (0.0, 20.0), 4.0)
}

/// An axis-aligned box `[x0, x1] x [y0, y1] x [z0, z1]`, two triangles
/// a face.
fn block(f: &mut Vec<Triangle>, (x0, x1): (f64, f64), (y0, y1): (f64, f64), (z0, z1): (f64, f64)) {
    let q = Point3::new;
    let v = [q(x0, y0, z0), q(x1, y0, z0), q(x1, y1, z0), q(x0, y1, z0), q(x0, y0, z1), q(x1, y0, z1), q(x1, y1, z1), q(x0, y1, z1)];
    let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
    for [i, j, k, l] in quads {
        f.push(Triangle::new(v[i], v[j], v[k]));
        f.push(Triangle::new(v[i], v[k], v[l]));
    }
}

#[test]
fn a_deep_twenty_mm_keyway_fits_on_the_true_axis() {
    let r = fit_report(&deep_keyed()).unwrap_or_else(|e| panic!("a deep-keyed shaft is revolved: {e}"));
    let off = |p: Point3| (p.x * p.x + p.y * p.y).sqrt();
    println!("deep key: plain off {:.4e} mm, axis off {:.4e} mm, set aside {:.4}", off(r.plain.origin), off(r.axis.origin), r.outlier_share);
    assert!(r.trusted, "{r:?}");
    assert!(off(r.plain.origin) > 0.3, "the plain fit is dragged past the band (the case under test): {r:?}");
    assert!(off(r.axis.origin) < 1e-9, "the consensus fit sits on the true axis: {r:?}");
    assert!(r.axis.dir.x.abs() < 1e-12 && r.axis.dir.y.abs() < 1e-12, "no lean: {r:?}");
    assert!(r.outlier_share > 0.0 && r.outlier_share < 0.25, "the keyway's walls are named: {r:?}");
}

#[test]
fn a_long_keyway_over_a_tenth_of_the_surface_fits() {
    // 15 degrees a side (a 10 mm key at the rim), 50 mm long, floor at
    // x = 2: past the plain gate's 10 % on share alone.
    let r = fit_report(&keyed(3, (-25.0, 25.0), 2.0)).unwrap_or_else(|e| panic!("a long-keyed shaft is revolved: {e}"));
    let off = |p: Point3| (p.x * p.x + p.y * p.y).sqrt();
    assert!(r.trusted && off(r.axis.origin) < 1e-9, "{r:?}");
    assert!(r.outlier_share > 0.1 && r.outlier_share < 0.25, "{r:?}");
}

#[test]
fn a_shaft_with_a_large_off_axis_lump_is_still_refused_by_name() {
    // An 8 x 8 x 50 block standing beside the shaft, off every meridian
    // through its walls' centres: its four walls carry 30 % of the
    // answering surface and none meets the axis - the part is not
    // mostly revolved. (A block centred ON a meridian has two walls
    // facing the axis, which the concurrency law reads as flats.)
    let mut f = keyed(0, (0.0, 0.0), R);
    block(&mut f, (12.0, 20.0), (6.0, 14.0), (-25.0, 25.0));
    let e = fit_report(&f).expect_err("a lump that big is not a feature of a turned part");
    assert!(e.to_string().contains("share no rotation axis"), "{e}");
}

#[test]
fn a_box_is_still_refused_by_name() {
    let mut f = Vec::new();
    block(&mut f, (-5.0, 5.0), (-10.0, 10.0), (-30.0, 30.0));
    let e = fit_report(&f).expect_err("a box is not a surface of revolution");
    assert!(e.to_string().contains("share no rotation axis"), "{e}");
}

#[test]
fn the_freeform_refusal_names_the_4_axis_case() {
    // #2772: a 4-axis machine has no 5-axis kinds to turn to, so the
    // refusal must say that a part clamped on A that is not turned is
    // not supported yet, and name the issue that tracks it.
    let mut f = Vec::new();
    block(&mut f, (-5.0, 5.0), (-10.0, 10.0), (-30.0, 30.0));
    let e = fit_report(&f).expect_err("a box is not a surface of revolution").to_string();
    assert!(e.contains("not a shape of revolution"), "{e}");
    assert!(e.contains("4-axis") && e.contains("#2772"), "{e}");
    assert!(e.contains("5-Axis kinds"), "the 5-axis pointer stays: {e}");
}
