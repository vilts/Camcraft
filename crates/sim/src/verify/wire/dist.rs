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

/// The clearance of [`clearance`], signed (#3211): a move that stands in
/// the product reads minus how far its deepest point stands inside (the
/// distance to the product's boundary), with that point - so the kerf's
/// entry there is its reach plus that distance, not the reach alone.
pub(super) fn entry(p: &Product<'_>, a: Point2, b: Point2) -> (f64, Point2) {
    let (d, at) = clearance(p, a, b);
    let at_t = |t: f64| Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
    let length = pp(a, b);
    let tolerance = (length / 1024.0).max(f64::MIN_POSITIVE);
    let mut cuts = vec![0.0, 1.0];
    crossings(p.boundary(), a, b, &mut cuts);
    if let Product::Around {
        stock: Some(stock), ..
    } = p
    {
        crossings(stock, a, b, &mut cuts);
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    let mut best = (0.0, at);
    // Bound each inside interval independently. Stock containment can
    // make the inside distance discontinuous at a stock edge, so split
    // there too; the boundary distance within each interval is 1-Lipschitz.
    for pair in cuts.windows(2) {
        let (lo, hi) = (pair[0], pair[1]);
        if !p.contains(at_t(lo + (hi - lo) / 2.0)) {
            continue;
        }
        let dl = to_boundary(p, at_t(lo));
        let dh = to_boundary(p, at_t(hi));
        for (t, depth) in [(lo, dl), (hi, dh)] {
            let q = at_t(t);
            if depth > best.0 && p.contains(q) {
                best = (depth, q);
            }
        }
        let mut pending = vec![(lo, hi, dl, dh)];
        while let Some((lo, hi, dl, dh)) = pending.pop() {
            let mid = lo + (hi - lo) / 2.0;
            let q = at_t(mid);
            let dm = to_boundary(p, q);
            if dm > best.0 && p.contains(q) {
                best = (dm, q);
            }
            // The two endpoint cones bound every possible deeper entry,
            // including one whose coarse samples would both be outside.
            let upper = (dl + dh + length * (hi - lo)) / 2.0;
            if upper <= best.0 + tolerance || mid <= lo || mid >= hi {
                continue;
            }
            pending.push((lo, mid, dl, dm));
            pending.push((mid, hi, dm, dh));
        }
    }
    if best.0 > 0.0 {
        (-best.0, best.1)
    } else {
        (d, at)
    }
}

/// Split a move at every crossing of a polygon boundary. Between these
/// parameters, containment is constant. Collinear edges contribute their
/// endpoints, so a walk along a boundary also has explicit intervals.
fn crossings(region: &crate::geo2d::Region, a: Point2, b: Point2, cuts: &mut Vec<f64>) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    for l in region.loops() {
        let pts = l.points();
        for i in 0..pts.len() {
            let (c, d) = (pts[i], pts[(i + 1) % pts.len()]);
            let (ex, ey) = (d.x - c.x, d.y - c.y);
            let det = dx * ey - dy * ex;
            let (cx, cy) = (c.x - a.x, c.y - a.y);
            if det != 0.0 {
                let t = (cx * ey - cy * ex) / det;
                let u = (cx * dy - cy * dx) / det;
                if t.is_finite() && t > 0.0 && t < 1.0 && (0.0..=1.0).contains(&u) {
                    cuts.push(t);
                }
            } else if cx * dy - cy * dx == 0.0 {
                for q in [c, d] {
                    let t = if dx.abs() >= dy.abs() {
                        (q.x - a.x) / dx
                    } else {
                        (q.y - a.y) / dy
                    };
                    if t.is_finite() && t > 0.0 && t < 1.0 {
                        cuts.push(t);
                    }
                }
            }
        }
    }
}

/// How far `q` stands from the product's boundary.
fn to_boundary(p: &Product<'_>, q: Point2) -> f64 {
    let mut best = f64::INFINITY;
    for l in p.boundary().loops() {
        let pts = l.points();
        for i in 0..pts.len() {
            best = best.min(pp(q, foot(q, pts[i], pts[(i + 1) % pts.len()])));
        }
    }
    if best.is_finite() { best } else { 0.0 }
}
