//! The closed-form distance from a point to an untrimmed analytic surface
//! (plane, cylinder, sphere, torus, cone) of a record face (#940, split out
//! of mesh_deviation for #917).
//! Provenance: standard analytic geometry (any solid-modelling text). No
//! HSMWorks material consulted.

use solidcore_record::brep::{self, Frame};

use crate::P3;
use crate::mesh_bound::{cross, dot, norm, sub, unit};

fn cylindrical(f: &Frame, p: P3) -> Option<(f64, f64)> {
    let z = unit(cross(unit(f.x_axis)?, unit(f.y_axis)?))?;
    let d = sub(p, f.origin);
    let h = dot(d, z);
    let rho = (dot(d, d) - h * h).max(0.0).sqrt();
    Some((h, rho))
}

/// The distance from `p` to the untrimmed analytic surface of a record
/// face, `None` for a surface with no closed form here (splines, blends,
/// revolutions) or a degenerate frame.
#[must_use]
pub fn analytic_distance(s: &brep::Surface, p: P3) -> Option<f64> {
    use brep::Surface as S;
    let d = match s {
        S::Plane { origin, u_axis, v_axis } => {
            dot(sub(p, *origin), unit(cross(*u_axis, *v_axis))?).abs()
        }
        S::PlaneThrough { origin, u_point, v_point } => {
            dot(sub(p, *origin), unit(cross(sub(*u_point, *origin), sub(*v_point, *origin)))?).abs()
        }
        S::Cylinder { frame, radius } => (cylindrical(frame, p)?.1 - radius).abs(),
        S::Sphere { frame, radius } => (norm(sub(p, frame.origin)) - radius).abs(),
        S::BallPatch { center, radius } => (norm(sub(p, *center)) - radius).abs(),
        S::Torus { frame, major_radius, minor_radius, .. } => {
            let (h, rho) = cylindrical(frame, p)?;
            ((rho - major_radius).hypot(h) - minor_radius).abs()
        }
        S::Cone { frame, radius_at_origin, half_angle } => {
            let (h, rho) = cylindrical(frame, p)?;
            let (sa, ca) = half_angle.sin_cos();
            let t = (rho - radius_at_origin) * sa + h * ca;
            if radius_at_origin + t * sa >= 0.0 {
                ((rho - radius_at_origin) * ca - h * sa).abs()
            } else {
                let apex_h = if sa != 0.0 { -radius_at_origin * ca / sa } else { 0.0 };
                rho.hypot(h - apex_h)
            }
        }
        S::Spline { .. }
        | S::RationalSpline { .. }
        | S::PlacedRevolution { .. }
        | S::Blend { .. } => return None,
    };
    d.is_finite().then_some(d)
}
