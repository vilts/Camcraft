//! Measuring helpers of the arc turn tests (#1759): points on the true
//! curve of a move, distances to a wall, and the tangent check of the
//! arcs an option adds. Plane geometry; no vendor material.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::geo2d::{Point2, Region};

pub fn seg_dist(q: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    if l2 == 0.0 {
        return (q.x - a.x).hypot(q.y - a.y);
    }
    let t = (((q.x - a.x) * dx + (q.y - a.y) * dy) / l2).clamp(0.0, 1.0);
    (q.x - a.x - t * dx).hypot(q.y - a.y - t * dy)
}

pub fn to_wall(region: &Region, q: Point2) -> f64 {
    let mut d = f64::INFINITY;
    for l in region.loops() {
        let p = l.points();
        for i in 0..p.len() {
            d = d.min(seg_dist(q, p[i], p[(i + 1) % p.len()]));
        }
    }
    d
}

/// Points ON the true curve of a planar move: lines every ~0.05 mm,
/// arcs by angle in the travel sense.
pub fn samples(m: &Move) -> Vec<Point2> {
    let a = Point2::new(m.from.x, m.from.y);
    let b = Point2::new(m.to.x, m.to.y);
    match m.arc {
        None => {
            let n = (((b.x - a.x).hypot(b.y - a.y) / 0.05).ceil() as usize).clamp(1, 4000);
            (0..=n)
                .map(|k| {
                    let t = k as f64 / n as f64;
                    Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
                })
                .collect()
        }
        Some(arc) => {
            let c = arc.center;
            let rho = (a.x - c.x).hypot(a.y - c.y);
            let s = (a.y - c.y).atan2(a.x - c.x);
            let e = (b.y - c.y).atan2(b.x - c.x);
            let tau = std::f64::consts::TAU;
            let sweep = if arc.ccw { (e - s).rem_euclid(tau) } else { (s - e).rem_euclid(tau) };
            (0..=256)
                .map(|k| {
                    let t = s + if arc.ccw { 1.0 } else { -1.0 } * sweep * k as f64 / 256.0;
                    Point2::new(c.x + rho * t.cos(), c.y + rho * t.sin())
                })
                .collect()
        }
    }
}

/// Every point of the cutting moves in the plane of `z`.
pub fn cut_points(t: &Toolpath, z: f64) -> Vec<Point2> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z)
        .flat_map(samples)
        .collect()
}

pub fn tangent(m: &Move, start: bool) -> (f64, f64) {
    let v = match m.arc {
        None => (m.to.x - m.from.x, m.to.y - m.from.y),
        Some(arc) => {
            let p = if start { m.from } else { m.to };
            let (dx, dy) = (p.x - arc.center.x, p.y - arc.center.y);
            if arc.ccw { (-dy, dx) } else { (dy, -dx) }
        }
    };
    let l = v.0.hypot(v.1);
    (v.0 / l, v.1 / l)
}

/// The radii of the arcs `blended` has and `sharp` has not (by center),
/// each checked tangent to both neighbours (the center rides the 1 um
/// lattice: micro-radians per mm of radius).
pub fn added_tangent_radii(blended: &Toolpath, sharp: &Toolpath, what: &str) -> Vec<f64> {
    let mut radii = Vec::new();
    for (i, m) in blended.moves.iter().enumerate() {
        let Some(arc) = m.arc else { continue };
        if sharp.moves.iter().any(|s| s.arc.is_some_and(|b| b.center == arc.center)) {
            continue;
        }
        let rho = (m.from.x - arc.center.x).hypot(m.from.y - arc.center.y);
        let budget = 2e-6 / rho + 1e-9;
        let (before, after) = (&blended.moves[i - 1], &blended.moves[i + 1]);
        for (u, w) in [(tangent(before, false), tangent(m, true)), (tangent(m, false), tangent(after, true))] {
            let cross = u.0 * w.1 - u.1 * w.0;
            let dot = u.0 * w.0 + u.1 * w.1;
            assert!(dot > 0.0 && cross.abs() <= budget, "{what}: kink {cross} at an arc turn end (move {i}: {before:?} / {m:?} / {after:?})");
        }
        radii.push(rho);
    }
    radii
}
