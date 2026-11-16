//! A straight flat-tool sweep read along a horizontal ray.
//! Provenance: circle-line intersections and the rectangle between two discs.

use super::carry_history::Span;
use crate::geometry::Point3;

/// The ray parameter interval inside the XY capsule from `a` to `b`.
/// The ray direction is one of the signed coordinate axes.
pub(super) fn capsule(a: Point3, b: Point3, p: Point3, dir: [f64; 3], r: f64) -> Option<Span> {
    let circle = |c: Point3| {
        let (vx, vy) = (p.x - c.x, p.y - c.y);
        let cross = vx * dir[1] - vy * dir[0];
        let rest = r * r - cross * cross;
        if rest < 0.0 {
            return None;
        }
        let middle = -(vx * dir[0] + vy * dir[1]);
        let half = rest.sqrt();
        Some((middle - half, middle + half))
    };
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx.hypot(dy);
    let mut spans: Vec<Span> = [circle(a), circle(b)].into_iter().flatten().collect();
    if length > 0.0 {
        let (ux, uy) = (dx / length, dy / length);
        let (vx, vy) = (p.x - a.x, p.y - a.y);
        let constraints = [
            (vx * ux + vy * uy, dir[0] * ux + dir[1] * uy, 0.0, length),
            (vx * uy - vy * ux, dir[0] * uy - dir[1] * ux, -r, r),
        ];
        let (mut low, mut high) = (f64::NEG_INFINITY, f64::INFINITY);
        for (origin, slope, lo, hi) in constraints {
            if slope == 0.0 {
                if origin < lo || origin > hi {
                    low = f64::INFINITY;
                    high = f64::NEG_INFINITY;
                }
            } else {
                let (u, v) = ((lo - origin) / slope, (hi - origin) / slope);
                low = low.max(u.min(v));
                high = high.min(u.max(v));
            }
        }
        if low <= high {
            spans.push((low, high));
        }
    }
    let lo = spans.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let hi = spans.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
    (lo <= hi && lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}
