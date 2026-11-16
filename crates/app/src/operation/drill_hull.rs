//! The swept holder footprint's distance from the spindle (issue #84,
//! split from `drill_reach` for #842): the convex hull of a swept
//! rectangle and its distance from the origin - textbook planar geometry
//! (monotone-chain hull, point-to-segment distance). Our own design
//! notes; no vendor material consulted.

use crate::geometry::Point3;

/// Distance from the origin to the segment `p`-`q`.
pub(super) fn seg_dist(p: (f64, f64), q: (f64, f64)) -> f64 {
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let ll = dx * dx + dy * dy;
    let t = if ll > 0.0 { (-(p.0 * dx + p.1 * dy) / ll).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 + t * dx).hypot(p.1 + t * dy)
}

/// Closest XY approach to the spindle of the holder's footprint (a
/// rectangle `hl` long, `2 hr` wide, starting `reach` behind the tip
/// along the axis's XY part) swept from `a` to `b`: the convex hull of
/// its eight corners. An axis with no XY part stands the holder upright
/// - a disk of radius `hr` about the tip's XY.
pub(super) fn holder_approach(a: Point3, b: Point3, axis: [f64; 3], reach: f64, hl: f64, hr: f64) -> f64 {
    let l = axis[0].hypot(axis[1]);
    if l.is_nan() || l <= 1e-9 {
        return (seg_dist((a.x, a.y), (b.x, b.y)) - hr).max(0.0);
    }
    let (ux, uy) = (axis[0] / l, axis[1] / l);
    let (nx, ny) = (-uy, ux);
    let mut pts = Vec::with_capacity(8);
    for p in [a, b] {
        for s in [reach * l, (reach + hl) * l] {
            for k in [hr, -hr] {
                pts.push((p.x + s * ux + k * nx, p.y + s * uy + k * ny));
            }
        }
    }
    hull_dist(&mut pts)
}

/// Distance from the origin to the convex hull of `pts` (0 inside).
pub(super) fn hull_dist(pts: &mut [(f64, f64)]) -> f64 {
    if pts.iter().any(|(x, y)| !(x.is_finite() && y.is_finite())) {
        return f64::NAN;
    }
    pts.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1)));
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut hull: Vec<(f64, f64)> = Vec::with_capacity(2 * pts.len());
    let rev: Vec<(f64, f64)> = pts.iter().rev().copied().collect();
    for order in [&*pts, &rev[..]] {
        let start = hull.len();
        for &p in order {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    let n = hull.len();
    if n == 0 {
        return f64::NAN;
    }
    if n >= 3 && (0..n).all(|i| cross(hull[i], hull[(i + 1) % n], (0.0, 0.0)) >= 0.0) {
        return 0.0;
    }
    (0..n).map(|i| seg_dist(hull[i], hull[(i + 1) % n])).fold(f64::INFINITY, f64::min)
}
