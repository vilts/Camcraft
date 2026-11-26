//! The radial field law's geometry ([`super::rotary`]): the tool
//! assembly as convex solids at a pose (finite cylinders and the
//! ball's sphere), their intervals along a ray out of the work's axis,
//! and the ray-triangle hit the field is cast with.
//! Provenance: ray-cylinder and ray-sphere intervals are textbook
//! quadratics; ray-triangle after Möller and Trumbore, "Fast, Minimum
//! Storage Ray/Triangle Intersection" (JGT 1997, public). Our own code.

use crate::geometry::{Point3, Triangle};

use super::probe::{Atom, Prof};

pub(super) type V3 = [f64; 3];

pub(super) fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(super) fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(super) fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub(super) fn add_s(a: V3, d: V3, s: f64) -> V3 {
    [a[0] + d[0] * s, a[1] + d[1] * s, a[2] + d[2] * s]
}
pub(super) fn unit(a: V3) -> Option<V3> {
    let l = dot(a, a).sqrt();
    (l.is_finite() && l > 1e-12).then(|| [a[0] / l, a[1] / l, a[2] / l])
}
pub(super) fn p3(p: Point3) -> V3 {
    [p.x, p.y, p.z]
}

/// One convex piece of the tool: a finite cylinder from `p0` along
/// `ax` for `len`, or a sphere about `p0`.
#[derive(Clone, Copy)]
pub(super) enum Shape {
    Cyl { p0: V3, ax: V3, len: f64, r: f64 },
    Sph { c: V3, r: f64 },
}

impl Shape {
    pub(super) fn ends(&self) -> (V3, V3, f64) {
        match *self {
            Shape::Cyl { p0, ax, len, r } => (p0, add_s(p0, ax, len), r),
            Shape::Sph { c, r } => (c, c, r),
        }
    }

    /// The ray `o + t·d` (unit d, t >= 0) inside this solid, if any.
    pub(super) fn interval(&self, o: V3, d: V3) -> Option<(f64, f64)> {
        match *self {
            Shape::Sph { c, r } => {
                let w = sub(o, c);
                let b = dot(w, d);
                let disc = b * b - (dot(w, w) - r * r);
                if disc < 0.0 {
                    return None;
                }
                let q = disc.sqrt();
                clip(-b - q, -b + q)
            }
            Shape::Cyl { p0, ax, len, r } => {
                let w = sub(o, p0);
                let (wa, da) = (dot(w, ax), dot(d, ax));
                let wp = add_s(w, ax, -wa);
                let dp = add_s(d, ax, -da);
                let qa = dot(dp, dp);
                let qc = dot(wp, wp) - r * r;
                let (mut lo, mut hi) = if qa <= 1e-14 {
                    if qc > 0.0 {
                        return None;
                    }
                    (f64::NEG_INFINITY, f64::INFINITY)
                } else {
                    let qb = dot(wp, dp);
                    let disc = qb * qb - qa * qc;
                    if disc < 0.0 {
                        return None;
                    }
                    let q = disc.sqrt();
                    ((-qb - q) / qa, (-qb + q) / qa)
                };
                // The slab 0 <= wa + t·da <= len.
                if da.abs() <= 1e-14 {
                    if wa < 0.0 || wa > len {
                        return None;
                    }
                } else {
                    let (t0, t1) = ((-wa) / da, (len - wa) / da);
                    lo = lo.max(t0.min(t1));
                    hi = hi.min(t0.max(t1));
                }
                clip(lo, hi)
            }
        }
    }
}

fn clip(lo: f64, hi: f64) -> Option<(f64, f64)> {
    let lo = lo.max(0.0);
    (hi > lo && lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// The assembly's solids at a pose: (shape, is_flute, for_carving).
pub(super) fn shapes(atoms: &[Atom], tip: Point3, a: V3, fl: f64) -> Vec<(Shape, bool, bool)> {
    let t = p3(tip);
    let mut v = Vec::with_capacity(atoms.len() + 1);
    for at in atoms {
        let flute = at.z0 < fl - 1e-12;
        let cyl = |r: f64, z0: f64, z1: f64| Shape::Cyl { p0: add_s(t, a, z0), ax: a, len: (z1 - z0).max(0.0), r };
        match at.prof {
            Prof::Flat(r) => v.push((cyl(r, at.z0, at.z1), flute, flute)),
            Prof::Bull { flat, cr } if flat <= 1e-12 => {
                // A ball: the sphere about its center is the corner
                // exactly below the center, inside the flute above it.
                v.push((Shape::Sph { c: add_s(t, a, at.z0 + cr), r: cr }, flute, flute));
            }
            Prof::Bull { flat, .. } => {
                v.push((cyl(at.rho_max(), at.z0, at.z1), flute, false));
                v.push((cyl(flat, at.z0, at.z1), flute, flute));
            }
            Prof::Cone { .. } | Prof::Dove { .. } => v.push((cyl(at.rho_max(), at.z0, at.z1), flute, false)),
            Prof::Ball(r) => v.push((Shape::Sph { c: add_s(t, a, at.z0 + r), r }, flute, flute)),
        }
    }
    v.retain(|(s, _, carve)| match *s {
        Shape::Cyl { len, .. } => len > 0.0 || *carve,
        Shape::Sph { .. } => true,
    });
    v
}

/// Möller-Trumbore: the ray parameter of a front-or-back hit, t > 0.
pub(super) fn ray_tri(o: V3, d: V3, t: &Triangle) -> Option<f64> {
    let (a, b, c) = (p3(t.a), p3(t.b), p3(t.c));
    let (e1, e2) = (sub(b, a), sub(c, a));
    let h = cross(d, e2);
    let det = dot(e1, h);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(o, a);
    let uu = inv * dot(s, h);
    if !(-1e-12..=1.0 + 1e-12).contains(&uu) {
        return None;
    }
    let q = cross(s, e1);
    let vv = inv * dot(d, q);
    if vv < -1e-12 || uu + vv > 1.0 + 1e-12 {
        return None;
    }
    let tt = inv * dot(e2, q);
    (tt > 1e-12 && tt.is_finite()).then_some(tt)
}
