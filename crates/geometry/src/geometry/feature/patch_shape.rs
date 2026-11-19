//! Cylindrical wall classification for welded smooth patches (#2403):
//! distinguishes concave bores, convex cylinders, and general curved patches.
//!
//! Provenance: standard normal-covariance axis fit and Kasa algebraic circle fit
//! over mesh triangles; our own code, no vendor material consulted.

use std::f64::consts::PI;

use super::Triangle;

/// Geometric shape kind of a multi-strip smooth welded patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchShape {
    /// Concave cylindrical wall (>= ~162 deg wrap, normals pointing inward).
    Bore,
    /// Convex cylindrical wall (>= ~162 deg wrap, normals pointing outward).
    Cylinder,
    /// Fillet, cone, sphere, or non-cylindrical freeform smooth patch.
    CurvedFace,
}

/// Classify a multi-strip welded smooth patch using normal covariance + Kasa circle fit.
#[must_use]
pub fn classify_patch(tris: &[&Triangle]) -> PatchShape {
    if tris.len() < 6 {
        return PatchShape::CurvedFace;
    }
    let mut cov = [[0.0f64; 3]; 3];
    for t in tris {
        let n = tri_cross(t);
        let len = dot3(n, n).sqrt();
        if !len.is_finite() || len <= 1e-18 {
            continue;
        }
        let w = 0.5 * len;
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] += w * n[i] * n[j] / (len * len);
            }
        }
    }
    let (eval, evec) = jacobi_sym3(cov);
    if !eval.iter().all(|v| v.is_finite())
        || eval[2] <= 0.0
        || eval[1] <= 1e-6 * eval[2]
        || eval[0] > 0.05 * eval[1]
    {
        return PatchShape::CurvedFace;
    }
    let mut axis = evec[0];
    if axis[2] < 0.0 || (axis[2] == 0.0 && (axis[1] < 0.0 || (axis[1] == 0.0 && axis[0] < 0.0))) {
        axis = [-axis[0], -axis[1], -axis[2]];
    }
    let (u, v) = perp_basis(axis);
    let mut pts = Vec::with_capacity(tris.len() * 3);
    for t in tris {
        for q in [t.a, t.b, t.c] {
            let p = [q.x, q.y, q.z];
            if !p.iter().all(|c| c.is_finite()) {
                return PatchShape::CurvedFace;
            }
            pts.push([dot3(p, u), dot3(p, v)]);
        }
    }
    let Some((cx, cy, r)) = fit_circle_2d(&pts) else {
        return PatchShape::CurvedFace;
    };
    let tol = r.max(1e-3) * 0.08;
    if r <= 1e-4 || pts.iter().any(|[x, y]| ((x - cx).hypot(y - cy) - r).abs() > tol) {
        return PatchShape::CurvedFace;
    }

    let mut angles = Vec::with_capacity(tris.len());
    let mut radial_sign = 0.0f64;
    for t in tris {
        let c = [
            (t.a.x + t.b.x + t.c.x) / 3.0,
            (t.a.y + t.b.y + t.c.y) / 3.0,
            (t.a.z + t.b.z + t.c.z) / 3.0,
        ];
        let (cu, cv) = (dot3(c, u) - cx, dot3(c, v) - cy);
        if cu.is_finite() && cv.is_finite() {
            angles.push(cv.atan2(cu));
        }
        let un = tri_cross(t);
        radial_sign += dot3(un, u) * cu + dot3(un, v) * cv;
    }
    if angular_span(&mut angles) < 0.9 * PI || !radial_sign.is_finite() || radial_sign == 0.0 {
        return PatchShape::CurvedFace;
    }
    if radial_sign < 0.0 { PatchShape::Bore } else { PatchShape::Cylinder }
}

fn fit_circle_2d(pts: &[[f64; 2]]) -> Option<(f64, f64, f64)> {
    let m = pts.len() as f64;
    if m < 3.0 {
        return None;
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for [x, y] in pts {
        sx += *x;
        sy += *y;
    }
    let (mx, my) = (sx / m, sy / m);
    let (mut sxx, mut sxy, mut syy, mut b1, mut b2) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for [x, y] in pts {
        let (dx, dy) = (*x - mx, *y - my);
        let r2 = dx * dx + dy * dy;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
        b1 += 0.5 * dx * r2;
        b2 += 0.5 * dy * r2;
    }
    let det = sxx * syy - sxy * sxy;
    if !det.is_finite() || det.abs() <= 1e-12 * (sxx + syy).powi(2).max(1e-30) {
        return None;
    }
    let ux = (b1 * syy - b2 * sxy) / det;
    let uy = (sxx * b2 - sxy * b1) / det;
    let (cx, cy) = (mx + ux, my + uy);
    let r = (ux * ux + uy * uy + (sxx + syy) / m).sqrt();
    (cx.is_finite() && cy.is_finite() && r.is_finite()).then_some((cx, cy, r))
}

fn angular_span(angles: &mut [f64]) -> f64 {
    if angles.len() < 2 {
        return 0.0;
    }
    angles.sort_by(f64::total_cmp);
    let mut max_gap = 0.0f64;
    for w in angles.windows(2) {
        max_gap = max_gap.max(w[1] - w[0]);
    }
    if let (Some(first), Some(last)) = (angles.first(), angles.last()) {
        max_gap = max_gap.max(2.0 * PI - (*last - *first));
    }
    (2.0 * PI - max_gap).max(0.0)
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn tri_cross(t: &Triangle) -> [f64; 3] {
    let ab = [t.b.x - t.a.x, t.b.y - t.a.y, t.b.z - t.a.z];
    let ac = [t.c.x - t.a.x, t.c.y - t.a.y, t.c.z - t.a.z];
    [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ]
}

fn perp_basis(axis: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let seed = if axis[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    let u_raw = [
        seed[1] * axis[2] - seed[2] * axis[1],
        seed[2] * axis[0] - seed[0] * axis[2],
        seed[0] * axis[1] - seed[1] * axis[0],
    ];
    let u_len = dot3(u_raw, u_raw).sqrt().max(1e-18);
    let u = [u_raw[0] / u_len, u_raw[1] / u_len, u_raw[2] / u_len];
    let v = [
        axis[1] * u[2] - axis[2] * u[1],
        axis[2] * u[0] - axis[0] * u[2],
        axis[0] * u[1] - axis[1] * u[0],
    ];
    (u, v)
}

fn jacobi_sym3(mut a: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..32 {
        let mut p = 0;
        let mut q = 1;
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            if a[i][j].abs() > a[p][q].abs() {
                p = i;
                q = j;
            }
        }
        if a[p][q].abs() <= 1e-15 {
            break;
        }
        let tau = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        let hyp = tau.hypot(1.0);
        let t = if tau >= 0.0 { 1.0 / (tau + hyp) } else { -1.0 / (-tau + hyp) };
        let c = 1.0 / (1.0 + t * t).sqrt();
        let s = t * c;
        let app = a[p][p];
        let aqq = a[q][q];
        let apq = a[p][q];
        a[p][p] = app - t * apq;
        a[q][q] = aqq + t * apq;
        a[p][q] = 0.0;
        a[q][p] = 0.0;
        for k in 0..3 {
            if k != p && k != q {
                let (akp, akq) = (a[k][p], a[k][q]);
                a[k][p] = c * akp - s * akq;
                a[p][k] = a[k][p];
                a[k][q] = s * akp + c * akq;
                a[q][k] = a[k][q];
            }
            let (vkp, vkq) = (v[k][p], v[k][q]);
            v[k][p] = c * vkp - s * vkq;
            v[k][q] = s * vkp + c * vkq;
        }
    }
    let mut pairs = [
        (a[0][0], [v[0][0], v[1][0], v[2][0]]),
        (a[1][1], [v[0][1], v[1][1], v[2][1]]),
        (a[2][2], [v[0][2], v[1][2], v[2][2]]),
    ];
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
    ([pairs[0].0, pairs[1].0, pairs[2].0], [pairs[0].1, pairs[1].1, pairs[2].1])
}
