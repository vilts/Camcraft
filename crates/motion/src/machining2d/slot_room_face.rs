//! Plane geometry of the slot fit law (#1504): point-to-segment and
//! segment-to-segment distance, the convex hull, and how far a slot
//! (the capsule of radius `r` about the segment `a`-`b`) leaves a picked
//! face's region - measured exactly, not sampled.
//!
//! Provenance: textbook plane geometry - the orientation (cross product)
//! test for segment intersection and Andrew's monotone chain hull
//! (A. M. Andrew, "Another efficient algorithm for convex hulls in two
//! dimensions", 1979); no vendor material consulted.

use crate::geo2d::{Point2, Region};

use super::super::face::GenError;

/// Distance from `p` to the segment `a`-`b`.
pub(super) fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let k = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - (a.x + k * dx)).hypot(p.y - (a.y + k * dy))
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Distance between the segments `p`-`q` and `a`-`b`: zero where they
/// cross, else the least of the four endpoint-to-segment distances.
pub(super) fn seg_seg_dist(p: Point2, q: Point2, a: Point2, b: Point2) -> f64 {
    let (d1, d2) = (cross(a, b, p), cross(a, b, q));
    let (d3, d4) = (cross(p, q, a), cross(p, q, b));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return 0.0;
    }
    seg_dist(p, a, b).min(seg_dist(q, a, b)).min(seg_dist(a, p, q)).min(seg_dist(b, p, q))
}

/// Andrew's monotone chain; counter-clockwise, no repeated point.
pub(super) fn hull(pts: &[Point2]) -> Vec<Point2> {
    let mut p: Vec<Point2> = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let mut h: Vec<Point2> = Vec::with_capacity(2 * p.len());
    for pass in 0..2 {
        let start = h.len();
        let it: Box<dyn Iterator<Item = &Point2>> = if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
        for &q in it {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

/// How far the slot `a`-`b` of radius `r` leaves `region`, exactly:
/// - the wall law: the deepest any boundary segment of the region (an
///   outer side, an island, a notch) reaches into the slot, `r` less its
///   distance to the slot's centre segment. A slot that crosses the
///   region's boundary anywhere is caught here, with no sampling gap.
/// - the end law: a slot whose boundary meets no wall lies wholly inside
///   or wholly outside the region; an end centre outside it (past the
///   face, or in a hole) leaves it by `r` plus its distance to the
///   boundary.
///
/// # Errors
/// A region with no boundary to measure against.
pub(super) fn face_overshoot(region: &Region, a: Point2, b: Point2, r: f64) -> Result<f64, GenError> {
    let segs: Vec<(Point2, Point2)> = region
        .loops()
        .iter()
        .flat_map(|l| {
            let p = l.points();
            (0..p.len()).map(move |i| (p[i], p[(i + 1) % p.len()]))
        })
        .collect();
    if segs.is_empty() {
        return Err(GenError::Invalid("the picked face has no boundary to measure the slot against".into()));
    }
    let mut out = 0.0_f64;
    // The wall law: material standing in the slot.
    for &(p, q) in &segs {
        out = out.max(r - seg_seg_dist(p, q, a, b));
    }
    // The end law: a slot wholly past the face or in a hole.
    for c in [a, b] {
        if !region.contains(c) {
            let d = segs.iter().map(|&(s, e)| seg_dist(c, s, e)).fold(f64::INFINITY, f64::min);
            out = out.max(r + d);
        }
    }
    Ok(out)
}
