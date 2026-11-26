//! A 2D chamfer's break is credited only where its cone tip stands on a
//! wall (#2435).
//!
//! Provenance: the chamfer mill's cut law (machine-tool handbook
//! geometry, [`crate::cl::EdgeBreak`]): the 2D chamfer puts the tip ON
//! the wall at the corner depth and the cone's generatrix on the face,
//! so beside the tip, off the edge on one side, there is air down to
//! the tip. A declared edge that is no such wall - the top edge of a
//! bevel the model already carries, a ledge the tip sits under - puts
//! the tip inside the part, and the cone's whole V would be forgiven as
//! the declared bevel. Our own test over the model's facets; no vendor
//! material consulted.

use crate::cl::EdgeBreak;
use crate::geometry::Triangle;

/// How far off the declared edge, in plan mm, the test looks for the
/// air beside the tip: past the float noise of a section's loop on its
/// wall, far inside any bevel a chamfer mill is asked for.
pub const WALL_PROBE: f64 = 0.05;

/// Plan spacing of the samples along a declared edge, mm.
const SAMPLE_STEP: f64 = 0.25;

/// Most samples taken along one record; a longer edge spaces them out.
const MAX_SAMPLES: f64 = 200_000.0;

/// The records of `breaks` whose tip stands on a wall of `facets`; the
/// rest are dropped, and their columns are then judged against the
/// model as it is - stricter, never looser. At every sample along the
/// edge (inside each segment, at most `SAMPLE_STEP` apart) the
/// model's top `WALL_PROBE` off the edge, on at least one side, must
/// stand no higher than the tip (`edge z - depth`) plus `tol`. Framed
/// records ([`EdgeBreak::volume`]) and single points are kept as they
/// are: neither is a 2D chamfer's ride.
#[must_use]
pub fn on_walls(facets: &[Triangle], tol: f64, breaks: Vec<EdgeBreak>) -> Vec<EdgeBreak> {
    let tol = if tol.is_finite() { tol.max(0.0) } else { 0.0 };
    breaks
        .into_iter()
        .filter(|b| b.volume.is_some() || b.edge.len() < 2 || walled(facets, tol, b))
        .collect()
}

fn walled(facets: &[Triangle], tol: f64, b: &EdgeBreak) -> bool {
    let Some(tops) = Tops::near(facets, b) else {
        return false;
    };
    // The model's top on the two sides of each sample must leave air
    // down to the tip on at least one side.
    samples(b, |x, y, z, nx, ny| tops.at(x + nx, y + ny).min(tops.at(x - nx, y - ny)) <= z - b.depth + tol)
        .unwrap_or(false)
}

/// #2596: whether the model itself already carries a bevel (a modelled
/// chamfer or round) at any declared edge of `breaks`: somewhere along a
/// record, the model's top `WALL_PROBE` off the edge stands lower than
/// the edge by more than `tol` on BOTH sides. At a sharp edge one side
/// is the part's top, up at the edge's height. A no-pick 2D chamfer on
/// such a model has its edges drawn by the model, and the sim judges
/// its whole ride against the model ([`on_walls`] does not apply).
#[must_use]
pub fn models_a_bevel(facets: &[Triangle], tol: f64, breaks: &[EdgeBreak]) -> bool {
    let tol = if tol.is_finite() { tol.max(0.0) } else { 0.0 };
    breaks.iter().filter(|b| b.volume.is_none() && b.edge.len() >= 2).any(|b| {
        let Some(tops) = Tops::near(facets, b) else {
            return false;
        };
        // `samples` stops at the first false: a sample that shows the
        // bevel answers false there, so the record is "not all sharp".
        samples(b, |x, y, z, nx, ny| tops.at(x + nx, y + ny).max(tops.at(x - nx, y - ny)) >= z - tol) == Some(false)
    })
}

/// Run `ok(x, y, edge z, probe nx, probe ny)` at every sample along the
/// record's edge (inside each segment, at most `SAMPLE_STEP` apart);
/// `Some(true)` when every sample holds, `None` when the edge's length
/// is not finite.
fn samples(b: &EdgeBreak, ok: impl Fn(f64, f64, f64, f64, f64) -> bool) -> Option<bool> {
    let total: f64 = b.edge.windows(2).map(|s| plan(s[1].x - s[0].x, s[1].y - s[0].y)).sum();
    if !total.is_finite() {
        return None;
    }
    let step = SAMPLE_STEP.max(total / MAX_SAMPLES);
    Some(b.edge.windows(2).all(|seg| {
        let (a, c) = (seg[0], seg[1]);
        let (dx, dy) = (c.x - a.x, c.y - a.y);
        let len = plan(dx, dy);
        if len <= 1e-9 {
            return true;
        }
        let (nx, ny) = (-dy / len * WALL_PROBE, dx / len * WALL_PROBE);
        let n = (len / step).ceil().clamp(1.0, MAX_SAMPLES) as usize;
        (0..n).all(|i| {
            // A vertex's segment normal can land on the adjacent wall at
            // an inside corner. Sample the interior of each span instead.
            let t = (i as f64 + 0.5) / n as f64;
            ok(a.x + t * dx, a.y + t * dy, a.z + t * (c.z - a.z), nx, ny)
        })
    }))
}

/// The model's top over plan points near one record: its facets that
/// reach the record's plan box, bucketed on a coarse grid.
struct Tops<'a> {
    facets: Vec<&'a Triangle>,
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    buckets: Vec<Vec<usize>>,
}

impl<'a> Tops<'a> {
    fn near(facets: &'a [Triangle], b: &EdgeBreak) -> Option<Self> {
        let m = WALL_PROBE * 2.0;
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in &b.edge {
            (x0, y0, x1, y1) = (x0.min(p.x - m), y0.min(p.y - m), x1.max(p.x + m), y1.max(p.y + m));
        }
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            return None;
        }
        let cell = ((x1 - x0).max(y1 - y0) / 256.0).max(0.5);
        let nx = (((x1 - x0) / cell).ceil() as usize).clamp(1, 512);
        let ny = (((y1 - y0) / cell).ceil() as usize).clamp(1, 512);
        let mut tops = Tops { facets: Vec::new(), x0, y0, cell, nx, ny, buckets: vec![Vec::new(); nx * ny] };
        for t in facets {
            if !(t.a.is_finite() && t.b.is_finite() && t.c.is_finite()) {
                continue;
            }
            let (fx0, fx1) = (t.a.x.min(t.b.x).min(t.c.x), t.a.x.max(t.b.x).max(t.c.x));
            let (fy0, fy1) = (t.a.y.min(t.b.y).min(t.c.y), t.a.y.max(t.b.y).max(t.c.y));
            if fx1 < x0 || fx0 > x1 || fy1 < y0 || fy0 > y1 {
                continue;
            }
            let k = tops.facets.len();
            tops.facets.push(t);
            let (i0, i1) = (tops.ix(fx0), tops.ix(fx1));
            let (j0, j1) = (tops.iy(fy0), tops.iy(fy1));
            for j in j0..=j1 {
                for i in i0..=i1 {
                    if let Some(v) = tops.buckets.get_mut(i + j * nx) {
                        v.push(k);
                    }
                }
            }
        }
        Some(tops)
    }

    fn ix(&self, x: f64) -> usize {
        (((x - self.x0) / self.cell).floor().max(0.0) as usize).min(self.nx - 1)
    }

    fn iy(&self, y: f64) -> usize {
        (((y - self.y0) / self.cell).floor().max(0.0) as usize).min(self.ny - 1)
    }

    /// The highest facet over plan point `(x, y)`; `-inf` over air.
    fn at(&self, x: f64, y: f64) -> f64 {
        let Some(list) = self.buckets.get(self.ix(x) + self.iy(y) * self.nx) else {
            return f64::NEG_INFINITY;
        };
        let mut top = f64::NEG_INFINITY;
        for t in list.iter().filter_map(|&k| self.facets.get(k)) {
            let (a, b, c) = (t.a, t.b, t.c);
            let det = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            if det.abs() <= f64::EPSILON {
                continue; // a vertical facet has no top
            }
            let u = ((x - a.x) * (c.y - a.y) - (y - a.y) * (c.x - a.x)) / det;
            let v = ((b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)) / det;
            if u < -1e-9 || v < -1e-9 || u + v > 1.0 + 1e-9 {
                continue;
            }
            top = top.max(a.z + u * (b.z - a.z) + v * (c.z - a.z));
        }
        top
    }
}

fn plan(dx: f64, dy: f64) -> f64 {
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point3, cube};

    fn ring(h: f64, z: f64) -> Vec<Point3> {
        let (x, y) = (20.0 - h, 10.0 - h);
        vec![
            Point3::new(-x, -y, z),
            Point3::new(x, -y, z),
            Point3::new(x, y, z),
            Point3::new(-x, y, z),
            Point3::new(-x, -y, z),
        ]
    }

    fn brick() -> Vec<Triangle> {
        cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0)).facets.to_vec()
    }

    fn rec(edge: Vec<Point3>) -> EdgeBreak {
        EdgeBreak { edge, depth: 1.0, slope: 1.0, volume: None }
    }

    #[test]
    fn a_tip_on_the_wall_keeps_its_credit() {
        let kept = on_walls(&brick(), 0.01, vec![rec(ring(0.0, 10.0))]);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn a_tip_inside_the_part_loses_its_credit() {
        // The edge 1 mm in from the wall: the tip at z 9 under the top.
        let kept = on_walls(&brick(), 0.01, vec![rec(ring(1.0, 10.0))]);
        assert!(kept.is_empty());
    }

    #[test]
    fn a_sharp_edge_carries_no_modelled_bevel() {
        assert!(!models_a_bevel(&brick(), 0.01, &[rec(ring(0.0, 10.0))]));
    }

    #[test]
    fn an_edge_with_the_model_below_it_on_both_sides_is_a_modelled_bevel() {
        // The sharp brick's top edge plus one record 0.5 mm out from the
        // wall, where the model stands lower on both sides.
        assert!(models_a_bevel(&brick(), 0.01, &[rec(ring(0.0, 10.0)), rec(ring(-0.5, 10.0))]));
    }
}
