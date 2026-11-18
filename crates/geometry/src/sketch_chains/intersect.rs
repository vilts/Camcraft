//! Provenance: contact points of two line/arc/circle pieces. Segment-segment by the parametric
//! cross-product test (O'Rourke, "Computational Geometry in C", 2nd ed., sec. 7.2); line-circle
//! and circle-circle by the closed forms in Schneider and Eberly, "Geometric Tools for Computer
//! Graphics" (2003), sec. 7.3 and 7.5. An end of one piece lying on the other also counts, which
//! covers collinear and co-circular overlap. Original implementation.

use super::curve::{Piece, Shape, cross, dist, dot, sub};
use crate::geo2d::Point2;

fn along(p: Point2, d: Point2, t: f64) -> Point2 {
    Point2::new(p.x + d.x * t, p.y + d.y * t)
}

/// Distance from `p` to the piece's curve.
fn distance(piece: &Piece, p: Point2, eps: f64) -> f64 {
    match piece.shape {
        Shape::Line => {
            let d = sub(piece.b, piece.a);
            let len2 = dot(d, d);
            if len2 == 0.0 {
                return dist(p, piece.a);
            }
            let t = (dot(sub(p, piece.a), d) / len2).clamp(0.0, 1.0);
            dist(p, along(piece.a, d, t))
        }
        Shape::Circle { center, radius } => (dist(p, center) - radius).abs(),
        Shape::Arc { center, radius, .. } => {
            let ends = dist(p, piece.a).min(dist(p, piece.b));
            if piece.spans(p, eps) {
                ends.min((dist(p, center) - radius).abs())
            } else {
                ends
            }
        }
    }
}

fn round(piece: &Piece) -> Option<(Point2, f64)> {
    match piece.shape {
        Shape::Line => None,
        Shape::Arc { center, radius, .. } | Shape::Circle { center, radius } => {
            Some((center, radius))
        }
    }
}

fn line_line(p: &Piece, q: &Piece, eps: f64, out: &mut Vec<Point2>) {
    let (r, s) = (sub(p.b, p.a), sub(q.b, q.a));
    let (lr, ls) = (dot(r, r).sqrt(), dot(s, s).sqrt());
    let denom = cross(r, s);
    // Parallel or point-like lines have no single crossing; their overlap shows at the ends.
    if lr == 0.0 || ls == 0.0 || denom.abs() <= 1e-12 * lr * ls {
        return;
    }
    let w = sub(q.a, p.a);
    let (t, u) = (cross(w, s) / denom, cross(w, r) / denom);
    if t >= -eps / lr && t <= 1.0 + eps / lr && u >= -eps / ls && u <= 1.0 + eps / ls {
        out.push(along(p.a, r, t.clamp(0.0, 1.0)));
    }
}

fn line_round(
    line: &Piece,
    arc: &Piece,
    center: Point2,
    radius: f64,
    eps: f64,
    out: &mut Vec<Point2>,
) {
    let d = sub(line.b, line.a);
    let len = dot(d, d).sqrt();
    if len == 0.0 {
        return;
    }
    let f = sub(line.a, center);
    let off = cross(f, d).abs() / len;
    if off > radius + eps {
        return;
    }
    let mid = -dot(f, d) / (len * len);
    let half = (radius * radius - off * off).max(0.0).sqrt() / len;
    for t in [mid - half, mid + half] {
        if t >= -eps / len && t <= 1.0 + eps / len {
            let at = along(line.a, d, t.clamp(0.0, 1.0));
            if arc.spans(at, eps) {
                out.push(at);
            }
        }
    }
}

fn round_round(
    p: &Piece,
    q: &Piece,
    (c1, r1): (Point2, f64),
    (c2, r2): (Point2, f64),
    eps: f64,
    out: &mut Vec<Point2>,
) {
    let gap = dist(c1, c2);
    if gap <= eps {
        // Concentric: equal radii overlap. Arcs show that at their ends; two full circles
        // have no ends, so name a point of the shared circle.
        let circles = matches!(
            (p.shape, q.shape),
            (Shape::Circle { .. }, Shape::Circle { .. })
        );
        if circles && (r1 - r2).abs() <= eps {
            out.push(p.a);
        }
        return;
    }
    if gap > r1 + r2 + eps || gap < (r1 - r2).abs() - eps {
        return;
    }
    let a = (gap * gap + r1 * r1 - r2 * r2) / (2.0 * gap);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let u = Point2::new((c2.x - c1.x) / gap, (c2.y - c1.y) / gap);
    let foot = along(c1, u, a);
    for side in [-1.0, 1.0] {
        let at = Point2::new(foot.x - side * h * u.y, foot.y + side * h * u.x);
        if p.spans(at, eps) && q.spans(at, eps) {
            out.push(at);
        }
    }
}

/// Append every point where `p` and `q` cross, touch or overlap within `eps`.
pub(super) fn contacts(p: &Piece, q: &Piece, eps: f64, out: &mut Vec<Point2>) {
    match (round(p), round(q)) {
        (None, None) => line_line(p, q, eps, out),
        (None, Some((center, radius))) => line_round(p, q, center, radius, eps, out),
        (Some((center, radius)), None) => line_round(q, p, center, radius, eps, out),
        (Some(a), Some(b)) => round_round(p, q, a, b, eps, out),
    }
    for (piece, other) in [(p, q), (q, p)] {
        if matches!(piece.shape, Shape::Circle { .. }) {
            continue;
        }
        for end in [piece.a, piece.b] {
            if distance(other, end, eps) <= eps {
                out.push(end);
            }
        }
    }
}
