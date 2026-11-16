//! Test fixture (#1602): a thread's faces as the CAD exports them - a
//! rational strip, degree 1 across u, degree 2 along v in 36-degree circular
//! arcs that climb one pitch a turn, many turns long.
//!
//! Provenance: the rational quadratic circular arc (Piegl & Tiller, "The
//! NURBS Book", 2nd ed., sec. 7.2: end points on the circle, the middle
//! control point at r / cos(half-angle), weight cos(half-angle)), with z
//! linear in the angle. Our own fixture; no user file and no vendor data.

use truck_stepio::r#in::alias::{BSplineSurface, KnotVec, NurbsSurface, Surface, Vector4};

/// Degrees of arc per span along v.
const ARC: f64 = 36.0;

/// One rail of the strip: radius `r`, height `z0` at angle 0, rising
/// `pitch` per turn, for `turns` turns - homogeneous control points.
fn rail(r: f64, z0: f64, pitch: f64, turns: usize) -> Vec<Vector4> {
    let arcs = turns * (360.0 / ARC) as usize;
    let w = (ARC / 2.0).to_radians().cos();
    let z = |a: f64| z0 + pitch * a / std::f64::consts::TAU;
    let at = |rad: f64, a: f64, w: f64| Vector4::new(rad * a.cos() * w, rad * a.sin() * w, z(a) * w, w);
    let mut out = vec![at(r, 0.0, 1.0)];
    for k in 0..arcs {
        let mid = (k as f64 * ARC + ARC / 2.0).to_radians();
        let end = ((k + 1) as f64 * ARC).to_radians();
        out.push(at(r / w, mid, w));
        out.push(at(r, end, 1.0));
    }
    out
}

/// The knots along v: one span per arc on [0, 1], each inner knot doubled
/// (a circle in arcs is only C0 in its parameter at the joins).
pub(super) fn arc_knots(turns: usize) -> KnotVec {
    let arcs = turns * (360.0 / ARC) as usize;
    let mut k = vec![0.0; 3];
    for i in 1..arcs {
        let t = i as f64 / arcs as f64;
        k.extend([t, t]);
    }
    k.extend([1.0; 3]);
    KnotVec::from(k)
}

/// The strip between rail `(r0, z0)` (u = 0) and rail `(r1, z1)` (u = 1).
/// A flank when the radii differ; a root or crest band when they match.
pub(super) fn strip(turns: usize, pitch: f64, (r0, z0): (f64, f64), (r1, z1): (f64, f64)) -> Surface {
    let net = vec![rail(r0, z0, pitch, turns), rail(r1, z1, pitch, turns)];
    let knots = (KnotVec::bezier_knot(1), arc_knots(turns));
    Surface::NurbsSurface(Box::new(NurbsSurface::new(BSplineSurface::new(knots, net))))
}

/// A thread flank in millimetres: 6.35 mm out to 5.08 mm in, 0.75 mm
/// lower, 1.95 mm pitch - the proportions of the #1602 parts.
pub(super) fn flank(turns: usize) -> Surface {
    strip(turns, 1.95, (6.35, 0.0), (5.08, -0.75))
}

/// A thread root band: radius 5.08 mm, 0.1 mm wide along the axis.
pub(super) fn root(turns: usize) -> Surface {
    strip(turns, 1.95, (5.08, -0.75), (5.08, -0.85))
}
