//! The built-in accessories' solids (#789): closed, outward shells that
//! only ever touch, never overlap - a strap clamp's stud is drawn below
//! and above the strap, not through it, so a point inside the steel is
//! inside exactly one shell and the mesh law's ray-crossing test holds.
//! Holes and slots are drawn solid and the stud as a polygon around its
//! circle: a conservative envelope, never less than the real steel.
//!
//! Provenance: boxes, a prism extruded from a simple polygon (ear-clipped
//! by `vise::mesh::triangulate`) and a circumscribed polygonal cylinder;
//! our own code.

use crate::geometry::{Point3, Triangle};
use crate::vise::mesh::triangulate;

use super::Accessory;

/// The stud's sides.
pub const STUD_SIDES: usize = 16;

/// The radius of the polygon drawn around a stud of `diameter`: its
/// flats touch the true circle, so the polygon encloses it.
#[must_use]
pub fn stud_radius(diameter: f64) -> f64 {
    0.5 * diameter / (std::f64::consts::PI / STUD_SIDES as f64).cos()
}

/// A closed, outward box `lo..hi`.
#[must_use]
pub fn cuboid(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let c = |i: usize| {
        Point3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads
        .iter()
        .flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))])
        .collect()
}

/// Twice the signed area of a polygon (positive: counter-clockwise).
fn area2(poly: &[(f64, f64)]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum()
}

/// The closed, outward prism of simple polygon `poly` in (y, z),
/// extruded over x in `x0..x1`.
pub fn prism(poly: &[(f64, f64)], x0: f64, x1: f64) -> Result<Vec<Triangle>, String> {
    let mut poly = poly.to_vec();
    if area2(&poly) < 0.0 {
        poly.reverse();
    }
    let caps = triangulate(&poly)?;
    let at = |x: f64, k: usize| Point3::new(x, poly[k].0, poly[k].1);
    let mut tris = Vec::with_capacity(2 * caps.len() + 2 * poly.len());
    for [a, b, c] in &caps {
        tris.push(Triangle::new(at(x1, *a), at(x1, *b), at(x1, *c)));
        tris.push(Triangle::new(at(x0, *a), at(x0, *c), at(x0, *b)));
    }
    let n = poly.len();
    for i in 0..n {
        let j = (i + 1) % n;
        tris.push(Triangle::new(at(x0, i), at(x0, j), at(x1, j)));
        tris.push(Triangle::new(at(x0, i), at(x1, j), at(x1, i)));
    }
    Ok(tris)
}

/// A closed, outward polygonal cylinder around the z axis at (`x`, `y`),
/// `z0..z1`, its flats touching a circle of `diameter`.
#[must_use]
pub fn stud(x: f64, y: f64, diameter: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    let r = stud_radius(diameter);
    let ring = |k: usize, z: f64| {
        let a = std::f64::consts::TAU * (k % STUD_SIDES) as f64 / STUD_SIDES as f64;
        Point3::new(x + r * a.cos(), y + r * a.sin(), z)
    };
    let (bot, top) = (Point3::new(x, y, z0), Point3::new(x, y, z1));
    let mut tris = Vec::with_capacity(4 * STUD_SIDES);
    for k in 0..STUD_SIDES {
        let (a0, a1, b0, b1) = (ring(k, z0), ring(k + 1, z0), ring(k, z1), ring(k + 1, z1));
        tris.push(Triangle::new(bot, a1, a0));
        tris.push(Triangle::new(top, b0, b1));
        tris.push(Triangle::new(a0, a1, b1));
        tris.push(Triangle::new(a0, b1, b0));
    }
    tris
}

/// A step block's side profile in (y, z): steps rising toward +y.
#[must_use]
pub fn steps_profile(steps: u32, rise: f64, run: f64) -> Vec<(f64, f64)> {
    let n = f64::from(steps);
    let mut poly = vec![(0.0, 0.0), (n * run, 0.0)];
    for i in (0..steps).rev() {
        let i = f64::from(i);
        poly.push(((i + 1.0) * run, (i + 1.0) * rise));
        poly.push((i * run, (i + 1.0) * rise));
    }
    // The last corner pushed is (0, rise); the ring closes to (0, 0).
    poly
}

/// The accessory's shells in its own frame (see `accessory`'s frames).
/// `a` is assumed checked.
pub fn solid(a: &Accessory) -> Result<Vec<Triangle>, String> {
    let p = Point3::new;
    Ok(match *a {
        Accessory::Parallels { height, thickness, length, spacing } => {
            let (hx, g) = (0.5 * length, 0.5 * spacing);
            let mut t = cuboid(p(-hx, -g - thickness, 0.0), p(hx, -g, height));
            t.extend(cuboid(p(-hx, g, 0.0), p(hx, g + thickness, height)));
            t
        }
        Accessory::Block { length, width, height } => {
            cuboid(p(-0.5 * length, -0.5 * width, 0.0), p(0.5 * length, 0.5 * width, height))
        }
        Accessory::SoftJaw { width, height, thickness } => {
            cuboid(p(-0.5 * width, 0.0, 0.0), p(0.5 * width, thickness, height))
        }
        // The bores and bolt counterbores drawn solid: never less steel.
        Accessory::RotarySubplate { length, width, thickness, bore_x, bore_y, .. } => {
            cuboid(p(-bore_x, -bore_y, 0.0), p(length - bore_x, width - bore_y, thickness))
        }
        Accessory::StepBlock { steps, rise, run, width } => {
            prism(&steps_profile(steps, rise, run), -0.5 * width, 0.5 * width)?
        }
        Accessory::StrapClamp {
            strap_length,
            strap_width,
            strap_thickness,
            stud_diameter,
            stud_height,
            stud_offset,
            heel_length,
            clamp_height,
        } => {
            let (hw, top) = (0.5 * strap_width, clamp_height + strap_thickness);
            let mut t = cuboid(p(0.0, -hw, clamp_height), p(strap_length, hw, top));
            t.extend(cuboid(p(strap_length - heel_length, -hw, 0.0), p(strap_length, hw, clamp_height)));
            t.extend(stud(stud_offset, 0.0, stud_diameter, 0.0, clamp_height));
            t.extend(stud(stud_offset, 0.0, stud_diameter, top, top + stud_height));
            t
        }
        Accessory::ThreeJawChuck(c) => c.solid(3),
        Accessory::FourJawChuck(c) => c.solid(4),
        Accessory::ColletChuck(c) => c.solid(),
    })
}
