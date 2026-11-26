//! Plain three-vector helpers of the generated machine model (#2584).
//!
//! Provenance: standard vector algebra.

pub(super) type V3 = [f64; 3];

pub(super) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(super) fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(super) fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub(super) fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(super) fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// `a` at unit length; `None` for a zero or non-finite vector.
pub(super) fn unit(a: V3) -> Option<V3> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 1e-9).then(|| scale(a, 1.0 / n))
}
