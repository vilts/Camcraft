//! A triangle corner at a pole of a face's surface, given the parameter of
//! its triangle (#3034). At a pole one parameter names no direction: every
//! u (or v) there is the same point, so the planner may give the apex of a
//! fan any value. A measure that interpolates between that value and the
//! other corners' crosses the surface far from the triangle; this module
//! moves the free parameter of such a corner to the mean of the others'.
//! Provenance: degenerate (collapsed) boundaries of tensor-product patches,
//! where a partial derivative vanishes, are textbook (Farin, "Curves and
//! Surfaces for CAGD"; Piegl & Tiller, "The NURBS Book", sec. 7.5); our own
//! code over solidcore's public evaluator. No HSMWorks material consulted.

use solidcore::eval::plan::uv;
use solidcore_tess::EvalSurface;

use crate::mesh_bound::{norm, sub};

/// A parameter is free at a corner when the surface's derivative along it
/// is at most this fraction of the derivative along the other one.
const FREE_RATIO: f64 = 1e-9;

/// A corner's parameter is moved only when the surface point stays within
/// this distance (mm): the move must not change the corner.
const SAME_POINT_MM: f64 = 1e-9;

fn at(s: &EvalSurface, q: [f64; 2]) -> Option<([f64; 3], f64, f64)> {
    let (p, du, dv) = s.derivs(q[0], q[1])?;
    let p = [p.x, p.y, p.z];
    let length = |d: solidcore::Vec3| (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
    p.iter().all(|c| c.is_finite()).then_some((p, length(du), length(dv)))
}

/// The corners `q` of one triangle (in the face's parameters, `period` as
/// the face's domain gives it) with each corner that sits on a pole given
/// the mean of the other two corners' free parameter, those two taken on
/// one side of the period. `None` when no corner sits on a pole: `q` holds.
pub(crate) fn settle(s: &EvalSurface, period: [Option<f64>; 2], q: [[f64; 2]; 3]) -> Option<[[f64; 2]; 3]> {
    let mut out = q;
    let mut moved = false;
    for k in 0..3 {
        let Some((p, du, dv)) = at(s, q[k]) else { continue };
        for a in 0..2 {
            let (along, across) = if a == 0 { (du, dv) } else { (dv, du) };
            if !(across > 0.0 && along <= FREE_RATIO * across) {
                continue;
            }
            let (i, j) = ((k + 1) % 3, (k + 2) % 3);
            let qi = out[i][a];
            let qj = uv::unwrap(out[j][a], qi, period[a]);
            let mut c = out[k];
            c[a] = 0.5 * (qi + qj);
            let Some((p2, _, _)) = at(s, c) else { continue };
            if norm(sub(p, p2)) <= SAME_POINT_MM {
                out[j][a] = qj;
                out[k] = c;
                moved = true;
            }
        }
    }
    moved.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sphere octant of radius 1 (u around z over 90 degrees, v from the
    /// equator to the pole at v = 1), rational quadratic both ways.
    fn octant() -> EvalSurface {
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let around = [([1.0, 0.0], 1.0), ([1.0, 1.0], h), ([0.0, 1.0], 1.0)];
        let up = [(1.0, 0.0, 1.0), (1.0, 1.0, h), (0.0, 1.0, 1.0)];
        let mut ctrl = Vec::new();
        for (r, z, wv) in up {
            for ([x, y], wu) in around {
                let w = wu * wv;
                ctrl.push([w * r * x, w * r * y, w * z, w]);
            }
        }
        let k = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        EvalSurface::nurbs([2, 2], [k.clone(), k], [3, 3], ctrl)
    }

    #[test]
    fn a_corner_at_the_pole_takes_the_mean_u_of_the_others() {
        let s = octant();
        let q = [[0.0, 1.0], [0.9, 0.98], [1.0, 0.98]];
        let out = settle(&s, [None, None], q).expect("the apex sits on the pole");
        assert!((out[0][0] - 0.95).abs() < 1e-12 && out[0][1] == 1.0, "{out:?}");
        assert_eq!((out[1], out[2]), (q[1], q[2]), "the other corners hold");
    }

    #[test]
    fn a_triangle_off_the_pole_is_left_as_it_is() {
        let s = octant();
        assert_eq!(settle(&s, [None, None], [[0.1, 0.5], [0.9, 0.5], [0.5, 0.9]]), None);
    }

    #[test]
    fn the_other_corners_are_taken_on_one_side_of_the_period() {
        // A closed u of period 1 (as the q02 ellipsoid): the ring corners
        // at u = 0.98 and 0.02 are 0.04 apart, so the apex takes u = 1.0
        // (or 0.0), never 0.5 across the surface.
        let s = octant();
        let out = settle(&s, [Some(1.0), None], [[0.5, 1.0], [0.98, 0.98], [0.02, 0.98]]).expect("pole");
        assert!((out[0][0] - 1.0).abs() < 1e-12, "{out:?}");
        assert!((out[2][0] - 1.02).abs() < 1e-12, "{out:?}");
    }
}
