//! The two closed shells of the generated machine model (#2584): a block
//! and a cylinder, as outward triangles.
//!
//! Provenance: our own construction of a box and a prism from their
//! corners (general solid geometry). The round face follows the law of
//! `crate::chuck`: a circumscribed polygon, so the drawn body is never
//! smaller than the stated one.

use std::f64::consts::PI;

use super::vec::{add, cross, dot, scale, sub, V3};
use crate::geometry::{Point3, Triangle};

/// Sides of the round face of a cylinder.
pub const SIDES: usize = 32;

fn tri(a: V3, b: V3, c: V3) -> Triangle {
    let p = |v: V3| Point3::new(v[0], v[1], v[2]);
    Triangle::new(p(a), p(b), p(c))
}

/// A closed block about `centre`: `half[k]` each way along the unit
/// direction `dirs[k]`. The three directions are at right angles.
pub(super) fn block(centre: V3, dirs: [V3; 3], half: [f64; 3]) -> Vec<Triangle> {
    // Corner `i`: bit `k` of `i` set means the + side of direction `k`.
    let corner = |i: usize| {
        (0..3).fold(centre, |at, k| {
            let side = if i >> k & 1 == 1 { half[k] } else { -half[k] };
            add(at, scale(dirs[k], side))
        })
    };
    let corners: Vec<V3> = (0..8).map(corner).collect();
    let mut out = Vec::with_capacity(12);
    for (k, dir) in dirs.iter().enumerate() {
        let (u, v) = (1 << ((k + 1) % 3), 1 << ((k + 2) % 3));
        for plus in [false, true] {
            let base = if plus { 1 << k } else { 0 };
            let quad = [corners[base], corners[base | u], corners[base | u | v], corners[base | v]];
            let outward = scale(*dir, if plus { 1.0 } else { -1.0 });
            let normal = cross(sub(quad[1], quad[0]), sub(quad[3], quad[0]));
            // The basis can be left-handed: wind each face by its own normal.
            if dot(normal, outward) >= 0.0 {
                out.push(tri(quad[0], quad[1], quad[2]));
                out.push(tri(quad[0], quad[2], quad[3]));
            } else {
                out.push(tri(quad[0], quad[2], quad[1]));
                out.push(tri(quad[0], quad[3], quad[2]));
            }
        }
    }
    out
}

/// A closed cylinder of radius `r` on the +z axis from `z0` up to `z1`.
pub(super) fn cylinder(r: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    let step = 2.0 * PI / SIDES as f64;
    // Circumscribed: the flats touch the true circle.
    let rc = r / (0.5 * step).cos();
    let ring: Vec<(f64, f64)> = (0..SIDES)
        .map(|i| {
            let (s, c) = (step * i as f64).sin_cos();
            (rc * c, rc * s)
        })
        .collect();
    let mut out = Vec::with_capacity(4 * SIDES);
    for (i, &(x0, y0)) in ring.iter().enumerate() {
        let (x1, y1) = ring[(i + 1) % SIDES];
        let (b0, b1, t0, t1) = ([x0, y0, z0], [x1, y1, z0], [x0, y0, z1], [x1, y1, z1]);
        out.push(tri(b0, b1, t1));
        out.push(tri(b0, t1, t0));
        out.push(tri([0.0, 0.0, z1], t0, t1));
        out.push(tri([0.0, 0.0, z0], b1, b0));
    }
    out
}
