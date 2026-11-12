//! Core geometry types and mesh operations.
//!
//! Provenance: the mesh types and ops are our own design, specced from
//! standard computational-geometry literature; STL/OBJ are public formats
//! and their parsers (which need truck) stayed in the app crate at the
//! M4.1 R1 split (docs/M4.1-CRATE-SPLIT.md) - written from those public
//! specs only.
//!
//! Robustness bar (AGENTS.md Prime Directive): every path here is total -
//! malformed input returns an error, never panics. All float input is
//! validated finite before it reaches a mesh. Facet/vertex counts are
//! capped before any bulk allocation, so a corrupt header cannot OOM us.

pub mod feature;
mod primitives;
pub mod repair;
pub mod revision;
pub mod slice;
mod slice_weld;
pub mod stock;
pub mod units;

pub use primitives::{cube, cylinder};

/// A point in world space. f64 throughout the domain layer; the GPU converts
/// to f32 at the draw boundary, never the other way around.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Point3 {
    pub const ZERO: Point3 = Point3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Point3 { x, y, z }
    }

    /// False for NaN/inf; the only sanctioned gate for imported coordinates.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }


    pub fn cross(self, other: Point3) -> Point3 {
        Point3::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn dot(self, other: Point3) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
}

impl std::ops::Sub for Point3 {
    type Output = Point3;
    fn sub(self, other: Point3) -> Point3 {
        Point3::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

/// A rotation as a unit quaternion, stored (x, y, z, w); identity is
/// `[0, 0, 0, 1]`.
///
/// Provenance: standard rigid-transform mathematics - quaternion rotation
/// as documented in any graphics reference (Shoemake, "Rotating Elegantly",
/// ACM SIGGRAPH 1994 course notes); reimplemented from the published
/// formulas, no third-party code.
pub const IDENTITY_Q: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

/// Scale `q` to unit norm; `None` for a zero or non-finite quaternion (the
/// caller decides between keeping the old value and falling back to
/// identity - we never silently "fix" garbage).
#[must_use]
pub fn normalize_quat(q: [f64; 4]) -> Option<[f64; 4]> {
    let n2 = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3];
    if n2.is_finite() && n2 > 0.0 {
        let inv = 1.0 / n2.sqrt();
        Some([q[0] * inv, q[1] * inv, q[2] * inv, q[3] * inv])
    } else {
        None
    }
}

/// Pure Z-axis rotation (yaw) by `rad` radians; non-finite input yields
/// identity rather than NaN.
#[must_use]
pub fn quat_from_yaw(rad: f64) -> [f64; 4] {
    if rad.is_finite() {
        let (s, c) = (0.5f64 * rad).sin_cos();
        [0.0, 0.0, s, c]
    } else {
        IDENTITY_Q
    }
}

/// The inverse of a UNIT quaternion (rotation part negated).
#[must_use]
pub fn quat_conjugate(q: [f64; 4]) -> [f64; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

/// Hamilton product `r` ⊗ `q`: applying it to a point rotates by `q` first,
/// then by `r`. Provenance: published quaternion algebra.
#[must_use]
pub fn quat_mul(r: [f64; 4], q: [f64; 4]) -> [f64; 4] {
    [
        r[3] * q[0] + r[0] * q[3] + r[1] * q[2] - r[2] * q[1],
        r[3] * q[1] + r[1] * q[3] + r[2] * q[0] - r[0] * q[2],
        r[3] * q[2] + r[2] * q[3] + r[0] * q[1] - r[1] * q[0],
        r[3] * q[3] - (r[0] * q[0] + r[1] * q[1] + r[2] * q[2]),
    ]
}

/// Rotation by `rad` about one unit world axis: 0 = X, 1 = Y, 2 = Z.
/// Non-finite input yields identity, an unknown axis too.
#[must_use]
pub fn quat_from_axis_angle(axis: usize, rad: f64) -> [f64; 4] {
    if !rad.is_finite() {
        return IDENTITY_Q;
    }
    let (s, c) = (0.5f64 * rad).sin_cos();
    let mut q = [0.0, 0.0, 0.0, c];
    match axis {
        0 => q[0] = s,
        1 => q[1] = s,
        2 => q[2] = s,
        _ => return IDENTITY_Q,
    }
    q
}

/// An orientation from yaw-pitch-roll Tait-Bryan angles (radians):
/// R = Rz(yaw) ⊗ Ry(pitch) ⊗ Rx(roll), i.e. a point is rolled about X
/// first, then pitched about Y, then yawed about Z. Provenance: standard
/// rigid-transform math, same formulas as any graphics reference.
#[must_use]
pub fn quat_from_euler(roll: f64, pitch: f64, yaw: f64) -> [f64; 4] {
    quat_mul(
        quat_from_axis_angle(2, yaw),
        quat_mul(
            quat_from_axis_angle(1, pitch),
            quat_from_axis_angle(0, roll),
        ),
    )
}

/// The inverse of [`quat_from_euler`]: `(roll, pitch, yaw)` in radians. At
/// pitch = ±90° (gimbal lock) the split between roll and yaw is ambiguous,
/// so roll is reported as 0 and yaw carries the whole twist; re-encoding
/// still yields the same rotation, which is what the UI round-trip needs.
#[must_use]
pub fn quat_to_euler(q: [f64; 4]) -> (f64, f64, f64) {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let sinp = (2.0 * (w * y - x * z)).clamp(-1.0, 1.0);
    if sinp.abs() >= 1.0 - 1e-12 {
        let pitch = std::f64::consts::FRAC_PI_2.copysign(sinp);
        let yaw = (-2.0 * (x * y - z * w)).atan2(1.0 - 2.0 * (x * x + z * z));
        (0.0, pitch, yaw)
    } else {
        (
            (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y)),
            sinp.asin(),
            (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z)),
        )
    }
}

/// Rotate `p` by unit quaternion `q` (v' = v + w·t + qv×t with t = 2·qv×v).
/// Pass the conjugate for the inverse rotation.
#[must_use]
pub fn quat_rotate_point(q: [f64; 4], p: Point3) -> Point3 {
    let qv = Point3::new(q[0], q[1], q[2]);
    let c = qv.cross(p);
    let t = Point3::new(2.0 * c.x, 2.0 * c.y, 2.0 * c.z);
    let u = qv.cross(t);
    Point3::new(
        p.x + q[3] * t.x + u.x,
        p.y + q[3] * t.y + u.y,
        p.z + q[3] * t.z + u.z,
    )
}

/// local -> world: rotate by `rot` (unit), then translate by `pos`.
#[must_use]
pub fn transform_point(p: Point3, pos: Point3, rot: [f64; 4]) -> Point3 {
    let r = quat_rotate_point(rot, p);
    Point3::new(r.x + pos.x, r.y + pos.y, r.z + pos.z)
}

/// world -> local: undo the translation, then the rotation. Exact inverse of
/// [`transform_point`] for any unit `rot`.
#[must_use]
pub fn transform_inverse_point(p: Point3, pos: Point3, rot: [f64; 4]) -> Point3 {
    quat_rotate_point(
        quat_conjugate(rot),
        Point3::new(p.x - pos.x, p.y - pos.y, p.z - pos.z),
    )
}

/// One planar facet: the three vertices are the whole story (72 bytes,
/// `Copy`), and orientation comes from [`Triangle::normal`] recomputed
/// from them. The old stored-normal field (M1-era, "reserved for
/// file-declared values") was always zero from every importer and read
/// by nothing - dropped for M5.3 E, where a quarter of the facet's
/// width is the difference between a 10M-triangle model fitting twice
/// over or once. The .cam wire format never carried it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub a: Point3,
    pub b: Point3,
    pub c: Point3,
}

impl Triangle {
    pub fn new(a: Point3, b: Point3, c: Point3) -> Self {
        Triangle { a, b, c }
    }

    /// Right-hand-rule facet normal; zero if the triangle is degenerate.
    pub fn normal(&self) -> Point3 {
        let n = (self.b - self.a).cross(self.c - self.a);
        let len2 = n.dot(n);
        if len2 > 0.0 {
            let inv = 1.0 / len2.sqrt();
            Point3::new(n.x * inv, n.y * inv, n.z * inv)
        } else {
            Point3::ZERO
        }
    }
}

/// Axis-aligned bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Point3,
    pub max: Point3,
}

impl Aabb {
    pub fn center(self) -> Point3 {
        Point3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }
}

/// A triangulated surface: the Phase-1 working representation of a model and
/// of stock. `Triangle`-only storage (no vertex sharing) keeps facet order
/// authoritative for later stock comparison; dedup happens at import for OBJ.
/// The buffer is `Arc<[Triangle]>` (M5.3 E): a 10M-triangle model is 720 MB,
/// and the one copy is THE model - every clone (a command snapshot, a
/// per-frame read) is a refcount, so undo history no longer duplicates the
/// solid it merely remembers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub facets: std::sync::Arc<[Triangle]>,
}

impl Mesh {
    pub fn new() -> Self {
        Mesh::default()
    }

    pub fn from_facets(facets: Vec<Triangle>) -> Self {
        Mesh {
            facets: facets.into(),
        }
    }

    /// Share an existing immutable buffer - the streaming importers hand
    /// over their finished slice without a copy.
    pub fn from_shared(facets: std::sync::Arc<[Triangle]>) -> Self {
        Mesh { facets }
    }

    pub fn is_empty(&self) -> bool {
        self.facets.is_empty()
    }

    pub fn len(&self) -> usize {
        self.facets.len()
    }

    pub fn bounds(&self) -> Option<Aabb> {
        let mut iter = self.facets.iter();
        let first = iter.next()?;
        let mut min = Point3::new(
            first.a.x.min(first.b.x).min(first.c.x),
            first.a.y.min(first.b.y).min(first.c.y),
            first.a.z.min(first.b.z).min(first.c.z),
        );
        let mut max = Point3::new(
            first.a.x.max(first.b.x).max(first.c.x),
            first.a.y.max(first.b.y).max(first.c.y),
            first.a.z.max(first.b.z).max(first.c.z),
        );
        for t in iter {
            for p in [t.a, t.b, t.c] {
                min.x = min.x.min(p.x);
                min.y = min.y.min(p.y);
                min.z = min.z.min(p.z);
                max.x = max.x.max(p.x);
                max.y = max.y.max(p.y);
                max.z = max.z.max(p.z);
            }
        }
        Some(Aabb { min, max })
    }

    /// Reject any non-finite coordinate; called by importers before a Mesh
    /// exists in the world (Prime Directive: validate early).
    #[must_use]
    pub fn all_finite(&self) -> bool {
        self.facets
            .iter()
            .all(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangle_normal_right_hand_rule() {
        let t = Triangle::new(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        );
        let n = t.normal();
        assert!(n.z > 0.99);
    }

    #[test]
    fn degenerate_normal_is_zero_not_nan() {
        let p = Point3::new(1.0, 2.0, 3.0);
        let t = Triangle::new(p, p, p);
        assert_eq!(t.normal(), Point3::ZERO);
    }

    #[test]
    fn aabb_from_facets() {
        let m = cube(Point3::ZERO, Point3::new(2.0, 2.0, 2.0));
        let b = m.bounds().expect("cube has bounds");
        assert!(b.min.x == 0.0 && b.max.z == 2.0);
        assert!(m.all_finite());
    }

    fn close(a: Point3, b: Point3) -> bool {
        (a - b).dot(a - b) < 1e-24
    }

    #[test]
    fn identity_quat_is_a_no_op() {
        let p = Point3::new(1.5, -2.25, 3.0);
        assert!(close(quat_rotate_point(IDENTITY_Q, p), p));
    }

    #[test]
    fn yaw_90_maps_x_to_y_exactly() {
        let q = quat_from_yaw(std::f64::consts::FRAC_PI_2);
        let p = quat_rotate_point(q, Point3::new(1.0, 0.0, 0.0));
        assert!(close(p, Point3::new(0.0, 1.0, 0.0)), "{p:?}");
        // z is invariant under a z-rotation
        assert!(close(quat_rotate_point(q, Point3::new(0.0, 0.0, 7.0)), Point3::new(0.0, 0.0, 7.0)));
    }

    #[test]
    fn transform_round_trip() {
        let pos = Point3::new(10.0, -4.0, 0.5);
        let rot = quat_from_yaw(0.7);
        let p = Point3::new(1.25, 2.5, -3.75);
        let w = transform_point(p, pos, rot);
        assert!(close(transform_inverse_point(w, pos, rot), p));
    }

    #[test]
    fn normalize_garbage_is_none_not_nan() {
        assert!(normalize_quat([0.0, 0.0, 0.0, 0.0]).is_none());
        assert!(normalize_quat([f64::NAN, 0.0, 0.0, 1.0]).is_none());
        assert!(normalize_quat([f64::INFINITY, 0.0, 0.0, 0.0]).is_none());
        let q = normalize_quat([2.0, 0.0, 0.0, 0.0]).expect("unit x-quat");
        assert!((q[3] - 0.0).abs() < 1e-15 && (q[0] - 1.0).abs() < 1e-15);
        // A unit quat normalizes to itself (rotation unchanged).
        let yaw = quat_from_yaw(1.1);
        let again = normalize_quat(yaw).expect("already unit");
        assert!(yaw.iter().zip(again).all(|(a, b)| (a - b).abs() < 1e-12));
    }

    #[test]
    fn yaw_of_nonfinite_is_identity() {
        assert_eq!(quat_from_yaw(f64::NAN), IDENTITY_Q);
        assert_eq!(quat_from_yaw(f64::INFINITY), IDENTITY_Q);
    }

    /// Unit quaternions describe the same rotation when equal up to sign.
    fn same_rotation(a: [f64; 4], b: [f64; 4]) -> bool {
        let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        (dot.abs() - 1.0).abs() < 1e-9
    }

    #[test]
    fn quat_multiplies_apply_right_to_left() {
        // q = qz ⊗ qy rotates a point by qy first: Ry(90) takes +Z to +X,
        // then Rz(45) turns +X toward +Y by 45 degrees.
        let q = quat_mul(
            quat_from_axis_angle(2, 45f64.to_radians()),
            quat_from_axis_angle(1, 90f64.to_radians()),
        );
        let p = quat_rotate_point(q, Point3::new(0.0, 0.0, 1.0));
        let h = std::f64::consts::FRAC_1_SQRT_2;
        assert!((p.x - h).abs() < 1e-12 && (p.y - h).abs() < 1e-12 && p.z.abs() < 1e-12, "{p:?}");
    }

    #[test]
    fn axis_quarters_are_exact_and_guards_hold() {
        // Rx(+90): +Z -> -Y (y to z, z to -y for a right-handed turn).
        let p = quat_rotate_point(quat_from_axis_angle(0, 90f64.to_radians()), Point3::new(0.0, 0.0, 1.0));
        assert!(p.x.abs() < 1e-12 && (p.y + 1.0).abs() < 1e-12 && p.z.abs() < 1e-12, "{p:?}");
        assert_eq!(quat_from_axis_angle(2, f64::NAN), IDENTITY_Q);
        assert_eq!(quat_from_axis_angle(7, 1.0), IDENTITY_Q, "unknown axis");
        assert_eq!(quat_from_euler(0.0, 0.0, 0.7), quat_from_yaw(0.7));
    }

    #[test]
    fn euler_round_trips_to_the_same_rotation() {
        for (r, p, y) in [
            (0.0, 0.0, 0.0),
            (30.0, -45.0, 60.0),
            (-170.0, 12.0, 179.0),
            (90.0, 90.0, 90.0),
            (0.0, 180.0, 0.0),
        ] {
            let rad = [r, p, y].map(f64::to_radians);
            let q1 = quat_from_euler(rad[0], rad[1], rad[2]);
            let (e0, e1, e2) = quat_to_euler(q1);
            let q2 = quat_from_euler(e0, e1, e2);
            assert!(same_rotation(q1, q2), "euler round trip {r},{p},{y}");
            if p.abs() < 89.0 {
                // Off gimbal the angles themselves come back.
                let back = [e0.to_degrees(), e1.to_degrees(), e2.to_degrees()];
                for (got, want) in back.iter().zip([r, p, y]) {
                    assert!((got - want).abs() < 1e-9, "{r},{p},{y} -> {back:?}");
                }
            }
        }
    }

    #[test]
    fn euler_gimbal_lock_keeps_the_rotation() {
        // pitch = +90: roll and yaw split ambiguously; the extraction
        // reports roll 0 and folds it into yaw, and the rotation survives.
        let q = quat_from_euler(30f64.to_radians(), 90f64.to_radians(), 20f64.to_radians());
        let (r, p, y) = quat_to_euler(q);
        assert_eq!(r, 0.0);
        assert!((p - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "{p}");
        assert!(same_rotation(q, quat_from_euler(r, p, y)));
        // And a documented control case: yaw 30 at gimbal decodes exact.
        let q = quat_from_euler(0.0, 90f64.to_radians(), 30f64.to_radians());
        let (_, _, y) = quat_to_euler(q);
        assert!((y.to_degrees() - 30.0).abs() < 1e-9, "{}", y.to_degrees());
    }
}
