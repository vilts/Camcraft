//! Fitting an axis and centre to a curved wall (#701, split out of
//! `roles`): its axis is the null space of the area-weighted facet-normal
//! covariance and its centre an algebraic (Kåsa) circle fit of the wall
//! vertices projected square to that axis, so a tilted bore reads true.
//!
//! Provenance: the covariance null-space axis fit and Kåsa's algebraic
//! circle fit (Kåsa, 1979) are standard; the Jacobi eigenvalue sweep is
//! the classic one for a real symmetric 3x3. Our own code, no vendor
//! material consulted.

use crate::geometry::Triangle;

use super::align::{cross, dot, norm, scale, sub};

/// A wall's facet normals must span at least this much to hold an axis
/// (a flat patch has none to fit). Relative to the mean squared norm.
const FLAT_FRACTION: f64 = 1e-8;

/// A wall's radii must agree within this fraction of the mean for the
/// circle fit to hold a centre (a crumpled wall is no bore).
const RING_BAND: f64 = 0.15;

/// Fit a curved wall from its placed triangles: its axis from the
/// scatter of its facet normals, its centre from the circle its wall
/// vertices trace around that axis. Both are our own least-squares -
/// normal-covariance for the axis, Kåsa for the circle - so a bore cut
/// askew still finds its true axis, not the model's guess.
///
/// The checks are the refusal rules: too few facets, a scatter that is
/// not a ring, and a wall whose radii do not agree are all no axis.
pub(crate) fn fit_wall(name: &str, tris: &[Triangle]) -> Result<([f64; 3], [f64; 3]), String> {
    let n = tris.len();
    if n < 8 {
        return Err(format!("wall of '{}' is too small to fit an axis from", name));
    }
    // Axis: the direction the facet normals vary least. Winding does
    // not matter: a normal and its negation are the same wall normal.
    let mut cov = [[0.0f64; 3]; 3];
    for t in tris {
        let (a, b, c) = ([t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]);
        let un = cross(sub(b, a), sub(c, a));
        let len = norm(un);
        if len <= 0.0 || !len.is_finite() {
            continue;
        }
        // Weight by area: twice the triangle is |n|, and the unit
        // outer product |n_hat><n_hat| is nn / |n|^2.
        let w = 0.5 * len;
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] += w * un[i] * un[j] / (len * len);
            }
        }
    }
    let (eval, evec) = jacobi_sym3(cov);
    if eval[2] <= 0.0 || eval[1] <= FLAT_FRACTION * eval[2] {
        // Degenerate wall: the normals do not spread over a disc, so
        // their flat direction is not unique - a flat fan is no wall.
        return Err(format!("wall of '{}' is too flat to fit an axis from", name));
    }
    // The flat direction is the axis, and its sign is pinned - the
    // +Z-pointing root, then +Y, then +X - so two callers fitting the
    // same wall always see the same frame, whatever the facet winding.
    let mut axis = evec[0];
    if axis[2] < 0.0 || (axis[2] == 0.0 && (axis[1] < 0.0 || (axis[1] == 0.0 && axis[0] < 0.0))) {
        axis = [-axis[0], -axis[1], -axis[2]];
    }
    // Centre: project the wall's vertices into a plane square to the
    // axis and fit the circle they trace (Kåsa: solve the 2x2 normal
    // equations of x^2+y^2 = 2c.x + d).
    let (u, v) = perp_basis(axis);
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut st = 0.0;
    let mut pts: Vec<[f64; 2]> = Vec::with_capacity(n * 3);
    for t in tris {
        for q in [t.a, t.b, t.c] {
            let p = [q.x, q.y, q.z];
            let x = dot(p, u);
            let y = dot(p, v);
            sx += x;
            sy += y;
            st += dot(p, axis);
            pts.push([x, y]);
        }
    }
    let m = pts.len() as f64;
    let mx = sx / m;
    let my = sy / m;
    let tbar = st / m;
    let mut a11 = 0.0;
    let mut a12 = 0.0;
    let mut a22 = 0.0;
    let mut b1 = 0.0;
    let mut b2 = 0.0;
    for [x, y] in &pts {
        let dx = x - mx;
        let dy = y - my;
        a11 += dx * dx;
        a12 += dx * dy;
        a22 += dy * dy;
        b1 += dx * (dx * dx + dy * dy);
        b2 += dy * (dx * dx + dy * dy);
    }
    let det = a11 * a22 - a12 * a12;
    let scale_det = (a11 + a22) * (a11 + a22);
    if det.abs() <= 1e-12 * scale_det || !det.is_finite() {
        return Err(format!("wall of '{}' does not curve enough to fit an axis from", name));
    }
    let icx = b1 * a22 - b2 * a12;
    let icy = b2 * a11 - b1 * a12;
    let cx = icx / det;
    let cy = icy / det;
    let rads: Vec<f64> = pts.iter().map(|[x, y]| norm([x - mx - cx, y - my - cy, 0.0])).collect();
    let mean_r = rads.iter().sum::<f64>() / rads.len() as f64;
    if mean_r <= 0.0 || !mean_r.is_finite() {
        return Err(format!("wall of '{}' collapses to a point - no axis to fit", name));
    }
    let spread = rads
        .iter()
        .map(|r| (r - mean_r).abs())
        .fold(0.0f64, f64::max);
    if spread > RING_BAND * mean_r {
        return Err(format!("wall of '{}' is not round enough to fit an axis from", name));
    }
    // The circle's centre in the plane coordinates, walked back along
    // the basis, and set down at the wall's mean height along the axis.
    let centre = [
        (mx + cx) * u[0] + (my + cy) * v[0] + tbar * axis[0],
        (mx + cx) * u[1] + (my + cy) * v[1] + tbar * axis[1],
        (mx + cx) * u[2] + (my + cy) * v[2] + tbar * axis[2],
    ];
    Ok((axis, centre))
}

/// Two unit vectors square to `axis`, forming a right-handed basis with
/// it. Deterministic: built from the smallest component of the axis.
fn perp_basis(axis: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let i = if axis[0].abs() <= axis[1].abs() && axis[0].abs() <= axis[2].abs() {
        0
    } else if axis[1].abs() <= axis[2].abs() {
        1
    } else {
        2
    };
    let mut w = [0.0; 3];
    w[i] = 1.0;
    let u = {
        let t = cross(axis, w);
        scale(t, 1.0 / norm(t))
    };
    let v = cross(axis, u);
    (u, v)
}

/// Cyclic Jacobi for a real symmetric 3x3: eigenvalues ascending, with
/// matching eigenvectors as columns folded into `evec`. A handful of
/// sweeps always; the wall covariance is tiny.
#[allow(clippy::needless_range_loop)] // fixed 3x3: k indexes both the matrix and the eigenvector columns
fn jacobi_sym3(a: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut a = a;
    let mut v = [[0.0f64; 3]; 3];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..32 {
        let off = norm([a[0][1], a[0][2], a[1][2]]);
        let diag = norm([a[0][0], a[1][1], a[2][2]]);
        if off <= 1e-15 * diag.max(1.0) {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            let apq = a[p][q];
            if apq.abs() <= 1e-300 {
                continue;
            }
            let phi = 0.5 * (2.0 * apq).atan2(a[p][p] - a[q][q]);
            let (c, s) = (phi.cos(), phi.sin());
            // A' = J^T A J with J's (p,q) block [[c, -s], [s, c]].
            for k in 0..3 {
                let (akp, akq) = (a[k][p], a[k][q]);
                a[k][p] = c * akp + s * akq;
                a[k][q] = -s * akp + c * akq;
            }
            for k in 0..3 {
                let (akp, akq) = (a[p][k], a[q][k]);
                a[p][k] = c * akp + s * akq;
                a[q][k] = -s * akp + c * akq;
            }
            for k in 0..3 {
                let (vkp, vkq) = (v[k][p], v[k][q]);
                v[k][p] = c * vkp + s * vkq;
                v[k][q] = -s * vkp + c * vkq;
            }
        }
    }
    let mut idx = [0, 1, 2];
    idx.sort_by(|i, j| a[*i][*i].partial_cmp(&a[*j][*j]).unwrap_or(std::cmp::Ordering::Equal));
    let eval = [a[idx[0]][idx[0]], a[idx[1]][idx[1]], a[idx[2]][idx[2]]];
    // A_final = V^T A V with V built by right-multiplication, so
    // A V = V A_final: the eigenvectors are the COLUMNS of v, and
    // evec[i] belongs to eval[i] = the diagonal idx[i].
    let evec = core::array::from_fn(|i| [v[0][idx[i]], v[1][idx[i]], v[2][idx[i]]]);
    (eval, evec)
}
