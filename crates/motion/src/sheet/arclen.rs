//! A closed ring read by arc length (#539, shared by #1883): the point
//! at a distance along it, the vertices of a stretch, the nearest point
//! of a stretch and the walk between two distances. Wire EDM lays its
//! tabs with it; waterjet lays its tab gaps with it.
//!
//! Provenance: plain 2D arc-length geometry on a polyline, moved out of
//! `wire_edm::tabs` without change. Our own code.

use crate::geo2d::Point2;

use super::ring::dist;

/// A closed ring by arc length.
pub struct ArcRing<'a> {
    pts: &'a [Point2],
    /// The arc length at each vertex, with the closing length last.
    pub cum: Vec<f64>,
    /// The ring's whole length.
    pub len: f64,
}

impl<'a> ArcRing<'a> {
    #[must_use]
    pub fn new(pts: &'a [Point2]) -> Self {
        let n = pts.len();
        let mut cum = Vec::with_capacity(n + 1);
        let mut s = 0.0;
        cum.push(0.0);
        for i in 0..n {
            s += dist(pts[i], pts[(i + 1) % n]);
            cum.push(s);
        }
        ArcRing { pts, cum, len: s }
    }

    /// The point at arc length `s` (any real; wrapped). A ring with no
    /// length answers its first point (or the origin when empty).
    #[must_use]
    pub fn at(&self, s: f64) -> Point2 {
        let n = self.pts.len();
        if n == 0 || !(self.len.is_finite() && self.len > 0.0) {
            return self.pts.first().copied().unwrap_or(Point2::new(0.0, 0.0));
        }
        let s = s.rem_euclid(self.len);
        let i = self.cum.partition_point(|c| *c <= s).saturating_sub(1).min(n - 1);
        let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
        let l = self.cum[i + 1] - self.cum[i];
        let t = if l > 0.0 { ((s - self.cum[i]) / l).clamp(0.0, 1.0) } else { 0.0 };
        Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y))
    }

    /// The vertices strictly between `a` and `b` (`a <= b`, unwrapped),
    /// counter-clockwise, with their arc lengths.
    #[must_use]
    pub fn between(&self, a: f64, b: f64) -> Vec<(f64, Point2)> {
        let n = self.pts.len();
        let mut out = Vec::new();
        if n == 0 || !(self.len.is_finite() && self.len > 0.0) {
            return out;
        }
        let mut lap = (a / self.len).floor() * self.len;
        while lap < b {
            for i in 0..n {
                let s = lap + self.cum[i];
                if s > a + 1e-9 && s < b - 1e-9 {
                    out.push((s, self.pts[i]));
                }
            }
            lap += self.len;
        }
        out
    }

    /// The nearest point of the stretch `a..b` to `p`: its arc length.
    #[must_use]
    pub fn nearest_on(&self, a: f64, b: f64, p: Point2) -> f64 {
        let mut pts = vec![(a, self.at(a))];
        pts.extend(self.between(a, b));
        pts.push((b, self.at(b)));
        let mut best = (a, f64::INFINITY);
        for w in pts.windows(2) {
            let ((sa, pa), (sb, pb)) = (w[0], w[1]);
            let (dx, dy) = (pb.x - pa.x, pb.y - pa.y);
            let dd = dx * dx + dy * dy;
            let t = if dd > 0.0 { (((p.x - pa.x) * dx + (p.y - pa.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
            let d = dist(p, Point2::new(pa.x + t * dx, pa.y + t * dy));
            if d < best.1 {
                best = (sa + t * (sb - sa), d);
            }
        }
        best.0
    }

    /// Walk from `a` to `b` along the ring (either direction): the
    /// points after `a`, ending at `b`.
    #[must_use]
    pub fn walk(&self, a: f64, b: f64) -> Vec<Point2> {
        let mut v: Vec<Point2> = if a <= b {
            self.between(a, b).into_iter().map(|(_, p)| p).collect()
        } else {
            self.between(b, a).into_iter().rev().map(|(_, p)| p).collect()
        };
        v.push(self.at(b));
        v
    }
}
