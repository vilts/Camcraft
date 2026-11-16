//! Plain 2D distances for the wire laws: point to point, point to
//! segment, segment to segment, and a move's clearance to the product.
//! Provenance: textbook Euclidean geometry (closest points of segments).
//! Our own code.

use super::Product;
use crate::geo2d::Point2;

pub(super) fn pp(a: Point2, b: Point2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// The point of segment `a`-`b` nearest `p`.
fn foot(p: Point2, a: Point2, b: Point2) -> Point2 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    if l2.is_nan() || l2 <= 0.0 {
        return a;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0);
    Point2::new(a.x + t * dx, a.y + t * dy)
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Whether segments `a`-`b` and `c`-`d` cross or touch (proper or not).
fn crosses(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// The distance between segments `a`-`b` and `c`-`d`, with the point of
/// `c`-`d` nearest the first.
pub(super) fn seg_seg(a: Point2, b: Point2, c: Point2, d: Point2) -> (f64, Point2) {
    if crosses(a, b, c, d) {
        let t = cross(c, d, a) / (cross(c, d, a) - cross(c, d, b));
        return (0.0, Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)));
    }
    let mut best = (f64::INFINITY, c);
    for (p, q) in [(a, foot(a, c, d)), (b, foot(b, c, d))] {
        let dd = pp(p, q);
        if dd < best.0 {
            best = (dd, q);
        }
    }
    for q in [c, d] {
        let dd = pp(q, foot(q, a, b));
        if dd < best.0 {
            best = (dd, q);
        }
    }
    best
}

/// How far the wire's centre walking `a`-`b` stays from the product,
/// and the product's point nearest it: 0 when the move stands in the
/// product. An empty product is infinitely clear.
pub(super) fn clearance(p: &Product<'_>, a: Point2, b: Point2) -> (f64, Point2) {
    if p.contains(a) {
        return (0.0, a);
    }
    if p.contains(b) {
        return (0.0, b);
    }
    let mut best = (f64::INFINITY, a);
    for l in p.boundary().loops() {
        let pts = l.points();
        for i in 0..pts.len() {
            let (c, d) = (pts[i], pts[(i + 1) % pts.len()]);
            let got = seg_seg(a, b, c, d);
            if got.0 < best.0 {
                best = got;
            }
        }
    }
    best
}
