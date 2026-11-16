//! The containment kinds' hole-wall excuse in the solid law (#1205): a
//! round hole imported as an inscribed n-gon stands inside its true
//! circle by each facet's sagitta, so a round tool finishing that circle
//! enters the facets by up to that much - the tessellation, not a cut.
//! The excuse is MEASURED, not assumed: a facet's sagitta about the
//! hole's own axis is its nearest vertex's radius less its nearest
//! approach to the axis (point-to-segment / point-in-triangle distance
//! in the plane square to the axis), and only a facet whose sagitta is
//! at most [`WALL_SAG`] reads as a round wall. A flat (a D-bore, a hex
//! or an octagon, a hole imported too coarse to be round) passes far
//! inside its corners' circle and is part steel like any other.
//!
//! Before #1205 the excuse was the dip of the facet's edges into the
//! TOOL's own disc, a bound that grows as the tool shrinks against the
//! hole; it let a thread mill cut 0.32 mm into an octagon's flats with no
//! warning. The excuse kept is never larger than that bound.
//!
//! Provenance: chord sagitta and point-to-segment distance from
//! elementary geometry; the cap is our own design (decision record
//! `docs/decisions/1205-hole-wall-nearest-point.md`). No vendor source.

use super::probe::Atom;
use super::swept::seg_dist;
use crate::geometry::{Point3, Triangle};

/// The deepest sagitta a facet may have about its hole's axis and still
/// be read as the tessellation of a round wall, mm: five gouge
/// tolerances - a 32-gon to r 10 mm, a 64-gon to r 41 mm - and
/// a tenth of what the flats of an octagon on r 5 mm stand inside it.
pub const WALL_SAG: f64 = 0.05;

/// One hole the operation declares: a point on its axis and the axis
/// direction (any length; a zero or non-finite one excuses nothing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoleAxis {
    pub at: Point3,
    pub dir: [f64; 3],
}

impl HoleAxis {
    /// A hole whose axis is the Z line through `(x, y)`.
    #[must_use]
    pub fn vertical(x: f64, y: f64) -> Self {
        Self { at: Point3::new(x, y, 0.0), dir: [0.0, 0.0, 1.0] }
    }
}

/// The penetration excused on facet `t` (tool frame, the atom's axis
/// along +Z, the move `d`) for the hole axes `axes` (their XY in the
/// same frame, each parallel to the tool axis). Zero unless every
/// vertex stands outside the atom's reach along the move and the facet
/// is a round wall of one of the holes (sagitta at most [`WALL_SAG`]);
/// then the smaller of that sagitta and the facet edges' dip into the
/// atom's disc - never more than the pre-#1205 excuse.
pub(super) fn wall_excuse(t: &[[f64; 3]; 3], d: [f64; 3], atom: &Atom, allow: f64, grow: f64, axes: &[[f64; 2]]) -> f64 {
    if axes.is_empty() {
        return 0.0;
    }
    let rho = atom.rho_max() + grow;
    let out = t.iter().all(|p| seg_dist([p[0], p[1]], [0.0, 0.0], [d[0], d[1]]) >= rho - allow);
    if !(out && rho.is_finite()) {
        return 0.0;
    }
    let sag = axes.iter().filter_map(|&c| facet_sag(t, c)).filter(|&s| s <= WALL_SAG).fold(f64::NEG_INFINITY, f64::max);
    if !sag.is_finite() {
        return 0.0; // no axis reads the facet as its round wall
    }
    let mut dip = 0.0f64;
    for k in 0..3 {
        let (p, q) = (t[k], t[(k + 1) % 3]);
        let half = 0.5 * (q[0] - p[0]).hypot(q[1] - p[1]);
        if half < rho {
            dip = dip.max(rho - (rho * rho - half * half).sqrt());
        }
    }
    sag.min(dip).max(0.0)
}

/// The facet's sagitta about the axis through `c` (square to the XY
/// plane of `t`'s frame): its nearest vertex's radius less its nearest
/// point's. `None` when anything is not finite - such a facet is never
/// excused.
pub(super) fn facet_sag(t: &[[f64; 3]; 3], c: [f64; 2]) -> Option<f64> {
    if !(c.iter().all(|v| v.is_finite()) && t.iter().flatten().all(|v| v.is_finite())) {
        return None;
    }
    let r = |p: [f64; 3]| (p[0] - c[0]).hypot(p[1] - c[1]);
    let vertex = r(t[0]).min(r(t[1])).min(r(t[2]));
    let s = vertex - nearest(t, c);
    s.is_finite().then_some(s.max(0.0))
}

/// Whether the part's wall about the vertical axis through `c` is a
/// tessellated circle wherever a tool reaching `reach` from that axis
/// could touch it between `z_lo` and `z_hi` (#894 x #1205): every wall
/// facet (normal square to the axis) standing nearer than `reach` to the
/// axis in that span has its sagitta about the axis at most [`WALL_SAG`].
/// A flat (an octagon, a D-bore) there, or anything not finite, answers
/// `false`; a thread mill's crests may pass only a round wall.
#[must_use]
pub fn hole_is_round(facets: &[Triangle], c: [f64; 2], reach: f64, z_lo: f64, z_hi: f64) -> bool {
    if !([c[0], c[1], reach, z_lo, z_hi].iter().all(|v| v.is_finite())) {
        return false;
    }
    facets.iter().all(|f| {
        let t = [[f.a.x, f.a.y, f.a.z], [f.b.x, f.b.y, f.b.z], [f.c.x, f.c.y, f.c.z]];
        if !t.iter().flatten().all(|v| v.is_finite()) {
            return false;
        }
        let (lo, hi) = t.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| (l.min(p[2]), h.max(p[2])));
        if hi < z_lo || lo > z_hi || nearest(&t, c) >= reach {
            return true; // out of the teeth's reach
        }
        let (u, v) = ([t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]], [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]]);
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len <= 1e-12 || n[2].abs() > 0.1 * len {
            return true; // a floor, a lid or a sliver: no wall to excuse
        }
        facet_sag(&t, c).is_some_and(|s| s <= WALL_SAG)
    })
}

/// The nearest distance from `c` to the facet's projection on the XY
/// plane: 0 when `c` lies inside it, else the nearest edge's.
fn nearest(t: &[[f64; 3]; 3], c: [f64; 2]) -> f64 {
    let p = |k: usize| [t[k][0], t[k][1]];
    let o = |a: [f64; 2], b: [f64; 2]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let (e1, e2, e3) = (o(p(0), p(1)), o(p(1), p(2)), o(p(2), p(0)));
    let inside = (e1 > 0.0 && e2 > 0.0 && e3 > 0.0) || (e1 < 0.0 && e2 < 0.0 && e3 < 0.0);
    if inside {
        return 0.0;
    }
    (0..3).map(|k| seg_dist(c, p(k), p((k + 1) % 3))).fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vertical wall facet between the points at angles `a0`, `a1`
    /// (radians) on the circle of radius `r` about the origin.
    fn chord(r: f64, a0: f64, a1: f64) -> [[f64; 3]; 3] {
        let p = |a: f64, z: f64| [r * a.cos(), r * a.sin(), z];
        [p(a0, 0.0), p(a1, 0.0), p(a1, 5.0)]
    }

    #[test]
    fn a_chord_reads_its_own_sagitta_about_the_axis() {
        let half = std::f64::consts::PI / 8.0; // an octagon's side
        let s = facet_sag(&chord(5.0, -half, half), [0.0, 0.0]).expect("finite");
        assert!((s - 5.0 * (1.0 - half.cos())).abs() < 1e-12, "{s}");
        let fine = std::f64::consts::PI / 96.0;
        let s = facet_sag(&chord(5.0, -fine, fine), [0.0, 0.0]).expect("finite");
        assert!(s < 0.003, "a 96-gon on r 5 is round: {s}");
    }

    #[test]
    fn a_facet_over_the_axis_or_unreadable_is_never_round() {
        let floor = [[-1.0, -1.0, 0.0], [2.0, -1.0, 0.0], [-1.0, 2.0, 0.0]];
        assert!(facet_sag(&floor, [0.0, 0.0]).is_some_and(|s| s > WALL_SAG));
        let mut bad = chord(5.0, 0.0, 0.1);
        bad[1][0] = f64::NAN;
        assert_eq!(facet_sag(&bad, [0.0, 0.0]), None);
        assert_eq!(facet_sag(&chord(5.0, 0.0, 0.1), [f64::INFINITY, 0.0]), None);
    }

    #[test]
    fn a_hole_is_round_only_while_its_reachable_wall_is_a_fine_circle() {
        let ring = |r: f64, n: usize| -> Vec<Triangle> {
            let step = std::f64::consts::TAU / n as f64;
            (0..n)
                .map(|k| {
                    let w = chord(r, k as f64 * step, (k + 1) as f64 * step);
                    let p = |q: [f64; 3]| Point3::new(q[0], q[1], q[2]);
                    Triangle::new(p(w[0]), p(w[1]), p(w[2]))
                })
                .collect()
        };
        assert!(hole_is_round(&ring(5.0, 32), [0.0, 0.0], 5.0, 0.0, 5.0), "a 32-gon on r 5");
        assert!(!hole_is_round(&ring(5.0, 8), [0.0, 0.0], 5.0, 0.0, 5.0), "an octagon's flats");
        assert!(hole_is_round(&ring(5.0, 8), [0.0, 0.0], 4.5, 0.0, 5.0), "flats out of reach");
        assert!(hole_is_round(&ring(5.0, 8), [0.0, 0.0], 5.0, 6.0, 9.0), "flats out of the span");
        assert!(!hole_is_round(&ring(5.0, 32), [f64::NAN, 0.0], 5.0, 0.0, 5.0));
    }
}
