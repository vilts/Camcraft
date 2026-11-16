//! Certified continuous between-sample upper bounds on triangle-to-surface
//! distance for every record and evaluator surface form (M12.1, #1029).
//! Provenance: our own proof synthesis from textbook geometry:
//! - Closest point to the origin on a triangle by Voronoi regions (Ericson,
//!   *Real-Time Collision Detection*, ch. 5.1.5).
//! - Exact barycentric variance identity and circumscribed-circle maximum of
//!   `sum_{i<j} b_i b_j |p_i - p_j|^2` over the unit 2-simplex.
//! - Cone `(rho, h)` Euclidean generator distance convexity and 1-Lipschitz bound.
//!
//! Parametric spline / revolved bounds live in [`crate::mesh_bound_spline`].

use solidcore::Vec3;
use solidcore::eval::plan::{
    EvalSurface,
    uv::{Domain, Starts, project_with, unwrap},
};
use solidcore_record::brep::{self, Frame};

use crate::P3;
pub use crate::mesh_bound_spline::parametric_eps;

pub(crate) fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(crate) fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub(crate) fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(crate) fn norm2(a: P3) -> f64 {
    dot(a, a)
}
pub(crate) fn norm(a: P3) -> f64 {
    norm2(a).sqrt()
}
pub(crate) fn unit(a: P3) -> Option<P3> {
    let n = norm(a);
    (n.is_finite() && n > 0.0).then(|| [a[0] / n, a[1] / n, a[2] / n])
}
fn cyl_basis(f: &Frame) -> Option<(P3, P3, P3)> {
    let u = unit(f.x_axis)?;
    let z = unit(cross(u, unit(f.y_axis)?))?;
    let v = unit(cross(z, u))?;
    Some((u, v, z))
}
fn seg_min_norm(a: P3, b: P3) -> f64 {
    let ab = sub(b, a);
    let den = norm2(ab);
    if !den.is_finite() || den <= 0.0 {
        return norm(a);
    }
    let t = (-dot(a, ab) / den).clamp(0.0, 1.0);
    norm(add(a, scale(ab, t)))
}
/// Exact minimum Euclidean norm on triangle `(a, b, c)` (closest point to `[0, 0, 0]`).
fn min_norm_on_triangle(a: P3, b: P3, c: P3) -> f64 {
    let (ab, ac) = (sub(b, a), sub(c, a));
    let (d1, d2) = (-dot(ab, a), -dot(ac, a));
    if d1 <= 0.0 && d2 <= 0.0 {
        return norm(a);
    }
    let (d3, d4) = (-dot(ab, b), -dot(ac, b));
    if d3 >= 0.0 && d4 <= d3 {
        return norm(b);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 && d1 - d3 > 0.0 {
        return norm(add(a, scale(ab, d1 / (d1 - d3))));
    }
    let (d5, d6) = (-dot(ab, c), -dot(ac, c));
    if d6 >= 0.0 && d5 <= d6 {
        return norm(c);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 && d2 - d6 > 0.0 {
        return norm(add(a, scale(ac, d2 / (d2 - d6))));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let den = (d4 - d3) + (d5 - d6);
        if den > 0.0 {
            return norm(add(b, scale(sub(c, b), (d4 - d3) / den)));
        }
    }
    let denom = va + vb + vc;
    if denom > 0.0 {
        norm(add(a, add(scale(ab, vb / denom), scale(ac, vc / denom))))
    } else {
        seg_min_norm(a, b).min(seg_min_norm(b, c)).min(seg_min_norm(c, a))
    }
}
fn quad_form_max(p: [P3; 3]) -> f64 {
    let mut s = [norm2(sub(p[1], p[0])), norm2(sub(p[2], p[1])), norm2(sub(p[0], p[2]))];
    s.sort_by(|x, y| y.total_cmp(x));
    let [a, b, c] = s;
    if !a.is_finite() || a <= 0.0 {
        return 0.0;
    }
    if a >= b + c {
        return 0.25 * a;
    }
    let area2_sq = norm2(cross(sub(p[1], p[0]), sub(p[2], p[0])));
    let raw = if area2_sq > 0.0 && area2_sq.is_finite() {
        let q = 0.25 * a * b * c / area2_sq;
        if q.is_finite() {
            q.max(0.25 * a).min(a / 3.0)
        } else {
            a / 3.0
        }
    } else {
        0.25 * a
    };
    raw * (1.0 + 1e-12)
}
fn radial_dip(rho: [f64; 3], q_max: f64) -> f64 {
    let lo = rho[0].min(rho[1]).min(rho[2]);
    let hi = rho[0].max(rho[1]).max(rho[2]);
    let a = q_max.max(0.0).sqrt().max(lo).min(hi);
    (a - (a * a - q_max).max(0.0).sqrt()).max(0.0)
}
/// Whether `analytic_tri_bound` returns the exact continuous supremum on one triangle.
#[must_use]
pub fn analytic_is_exact(s: &brep::Surface) -> bool {
    use brep::Surface as S;
    matches!(
        s,
        S::Plane { .. } | S::PlaneThrough { .. } | S::Sphere { .. } | S::BallPatch { .. } | S::Cylinder { .. }
    )
}
/// Certified upper bound on `sup_{x in tri} analytic_distance(s, x)`.
#[must_use]
pub fn analytic_tri_bound(s: &brep::Surface, tri: [P3; 3], d_corners: [f64; 3]) -> Option<f64> {
    use brep::Surface as S;
    if d_corners.iter().any(|d| !d.is_finite()) {
        return None;
    }
    let dmax = d_corners[0].max(d_corners[1]).max(d_corners[2]);
    let b = match s {
        S::Plane { u_axis, v_axis, .. } => {
            let _ = unit(cross(*u_axis, *v_axis))?;
            dmax
        }
        S::PlaneThrough { origin, u_point, v_point } => {
            let _ = unit(cross(sub(*u_point, *origin), sub(*v_point, *origin)))?;
            dmax
        }
        S::Sphere { frame, radius } => sphere_bound(frame.origin, *radius, tri),
        S::BallPatch { center, radius } => sphere_bound(*center, *radius, tri),
        S::Cylinder { frame, radius } => {
            let (u, v, _) = cyl_basis(frame)?;
            let y = tri.map(|p| {
                let d = sub(p, frame.origin);
                [dot(d, u), dot(d, v), 0.0]
            });
            let rho_max = y.iter().map(|q| norm(*q)).fold(0.0_f64, f64::max);
            let rho_min = min_norm_on_triangle(y[0], y[1], y[2]);
            (rho_max - radius).abs().max((radius - rho_min).abs())
        }
        S::Cone { frame, radius_at_origin, half_angle } => {
            let (u, v, z) = cyl_basis(frame)?;
            let mut h = [0.0; 3];
            let mut y = [[0.0; 3]; 3];
            let mut rho = [0.0; 3];
            for (k, p) in tri.iter().enumerate() {
                let d = sub(*p, frame.origin);
                h[k] = dot(d, z);
                y[k] = [dot(d, u), dot(d, v), 0.0];
                rho[k] = norm(y[k]);
            }
            let d_rho = radial_dip(rho, quad_form_max(y));
            let rho_lo = (rho[0].min(rho[1]).min(rho[2]) - d_rho).max(0.0);
            let (h_lo, h_hi) = (h[0].min(h[1]).min(h[2]), h[0].max(h[1]).max(h[2]));
            let (sa, ca) = half_angle.sin_cos();
            let h_ext = if ca * sa >= 0.0 { h_lo } else { h_hi };
            let r_proj = radius_at_origin + ((rho_lo - radius_at_origin) * sa + h_ext * ca) * sa;
            dmax + if r_proj >= 0.0 { ca.abs() } else { 1.0 } * d_rho
        }
        S::Torus { frame, major_radius, minor_radius, .. } => {
            let (u, v, z) = cyl_basis(frame)?;
            let mut h = [0.0; 3];
            let mut y = [[0.0; 3]; 3];
            let mut rho = [0.0; 3];
            for (k, p) in tri.iter().enumerate() {
                let d = sub(*p, frame.origin);
                h[k] = dot(d, z);
                y[k] = [dot(d, u), dot(d, v), 0.0];
                rho[k] = norm(y[k]);
            }
            let d_rho = radial_dip(rho, quad_form_max(y));
            let t_pts = [0, 1, 2].map(|k| [rho[k] - major_radius, h[k], 0.0]);
            let tau_max = t_pts.iter().map(|q| norm(*q)).fold(0.0_f64, f64::max) + d_rho;
            let tau_min = (min_norm_on_triangle(t_pts[0], t_pts[1], t_pts[2]) - d_rho).max(0.0);
            (tau_max - minor_radius).abs().max((minor_radius - tau_min).abs())
        }
        S::Spline { .. } | S::RationalSpline { .. } | S::PlacedRevolution { .. } => return None,
    };
    b.is_finite().then_some(b.max(dmax))
}
fn sphere_bound(center: P3, radius: f64, tri: [P3; 3]) -> f64 {
    let rel = tri.map(|p| sub(p, center));
    let r_max = rel.iter().map(|q| norm(*q)).fold(0.0_f64, f64::max);
    let r_min = min_norm_on_triangle(rel[0], rel[1], rel[2]);
    (r_max - radius).abs().max((radius - r_min).abs())
}
/// Orthogonally projects `tp` onto `s`, returning `(unwrapped_uv, distance)`.
pub fn project_sample(
    s: &EvalSurface,
    dom: &Domain,
    starts: &Starts,
    tp: P3,
    hint: Option<[f64; 2]>,
) -> Option<([f64; 2], f64)> {
    let p = Vec3::new(tp[0], tp[1], tp[2]);
    let mut best = hint.and_then(|h| {
        let (q, _, _) = s.derivs(h[0], h[1])?;
        let d = norm(sub([q.x, q.y, q.z], tp));
        d.is_finite().then_some((h, d))
    });
    if let Some(cand) = project_with(s, dom, starts, p, hint, f64::INFINITY)
        && let Some((q, _, _)) = s.derivs(cand[0], cand[1])
    {
        let d = norm(sub([q.x, q.y, q.z], tp));
        if d.is_finite() && best.is_none_or(|(_, bd)| d <= bd) {
            best = Some((cand, d));
        } else if hint.is_some()
            && let Some(g) = project_with(s, dom, starts, p, None, f64::INFINITY)
            && let Some((gq, _, _)) = s.derivs(g[0], g[1])
        {
            let gd = norm(sub([gq.x, gq.y, gq.z], tp));
            if gd.is_finite() && best.is_none_or(|(_, bd)| gd < bd) {
                best = Some((g, gd));
            }
        }
    } else if hint.is_some()
        && let Some(g) = project_with(s, dom, starts, p, None, f64::INFINITY)
        && let Some((gq, _, _)) = s.derivs(g[0], g[1])
    {
        let gd = norm(sub([gq.x, gq.y, gq.z], tp));
        if gd.is_finite() && best.is_none_or(|(_, bd)| gd < bd) {
            best = Some((g, gd));
        }
    }
    let (mut uvc, d) = best?;
    if let Some(h) = hint {
        uvc = [0, 1].map(|i| unwrap(uvc[i], h[i], dom.period[i]));
    }
    d.is_finite().then_some((uvc, d))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analytic_bounds_dominate_dense_barycentric_samples() {
        let f = Frame { origin: [1.0, -2.0, 0.5], x_axis: [1.0, 0.0, 0.0], y_axis: [0.0, 1.0, 0.0] };
        let tris = [
            [[2.5, -1.5, 4.0], [3.2, -0.4, 3.8], [1.9, -0.8, 4.3]],
            [[0.8, -2.2, 0.45], [1.2, -1.8, 0.55], [0.95, -2.05, 0.6]],
        ];
        let surfs = [
            brep::Surface::Sphere { frame: f.clone(), radius: 4.1 },
            brep::Surface::Cylinder { frame: f.clone(), radius: 1.5 },
            brep::Surface::Cone { frame: f.clone(), radius_at_origin: 0.05, half_angle: 0.35 },
            brep::Surface::Torus { frame: f, major_radius: 2.2, minor_radius: 0.7, tube_start: 0.0 },
        ];
        for tri in tris {
            for s in &surfs {
                let dc = tri.map(|p| crate::mesh_deviation::analytic_distance(s, p).unwrap());
                let b = analytic_tri_bound(s, tri, dc).unwrap();
                for i in 0..=36 {
                    for j in 0..=(36 - i) {
                        let (u, v, w) = (i as f64 / 36.0, j as f64 / 36.0, (36 - i - j) as f64 / 36.0);
                        let p = [0, 1, 2].map(|k| u * tri[0][k] + v * tri[1][k] + w * tri[2][k]);
                        let d = crate::mesh_deviation::analytic_distance(s, p).unwrap();
                        assert!(d <= b + 1e-12, "sample {d} exceeded bound {b} for {s:?}");
                    }
                }
            }
        }
    }
}
