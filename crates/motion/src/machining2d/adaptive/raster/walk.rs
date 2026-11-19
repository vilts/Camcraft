//! The planner stock's walks: a path cut step by step, read by the
//! front-half contact angle before each step cuts (`quality::measure`
//! law, peripheral-milling engagement arc). Ours.

use super::Raster;
use crate::geo2d::Point2;

impl Raster {
    /// Walk `pts` from its first point, reading the front-half contact
    /// angle at each step (radius `rim`) BEFORE that step cuts - the
    /// measuring stock's own law (`quality::measure`), so what the planner
    /// keeps is what is read; returns the worst angle and where.
    pub(in crate::machining2d::adaptive) fn walk(&mut self, pts: &[Point2], r: f64, rim: f64, step: f64) -> (f64, Point2) {
        self.walk_until(pts, (r, rim, step), f64::INFINITY)
    }

    /// [`Raster::walk`], abandoned at the first reading past `limit`
    /// (a trial that already failed need not finish; the stock it
    /// leaves is then partial and only fit to be dropped).
    pub(in crate::machining2d::adaptive) fn walk_until(&mut self, pts: &[Point2], (r, rim, step): (f64, f64, f64), limit: f64) -> (f64, Point2) {
        let mut worst = (0.0f64, pts.first().copied().unwrap_or_default());
        // Where the tool stands it has already cleared its own disc (the
        // walk before it, or the entry, brought it there).
        if let Some(&s) = pts.first() {
            self.cut(s, s, r);
        }
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            if len <= 1e-12 {
                continue;
            }
            let n = ((len / step).ceil() as usize).clamp(1, 1 << 16);
            let mut prev = a;
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let p = Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                let deg = self.engage(p, Point2::new((b.x - a.x) / len, (b.y - a.y) / len), rim);
                if deg > worst.0 {
                    worst = (deg, p);
                    if deg > limit {
                        return worst;
                    }
                }
                self.cut(prev, p, r);
                prev = p;
            }
        }
        worst
    }

    /// [`Raster::walk`], read per segment: the worst angle each segment
    /// `pts[i] -> pts[i + 1]` read (the microlift's air/cut marks).
    pub(in crate::machining2d::adaptive) fn walk_marks(&mut self, pts: &[Point2], (r, rim, step): (f64, f64, f64)) -> Vec<f64> {
        let mut marks = Vec::with_capacity(pts.len().saturating_sub(1));
        if let Some(&s) = pts.first() {
            self.cut(s, s, r);
        }
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            let mut worst = 0.0f64;
            if len > 1e-12 {
                let dir = Point2::new((b.x - a.x) / len, (b.y - a.y) / len);
                let n = ((len / step).ceil() as usize).clamp(1, 1 << 16);
                let mut prev = a;
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let p = Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                    worst = worst.max(self.engage(p, dir, rim));
                    self.cut(prev, p, r);
                    prev = p;
                }
            }
            marks.push(worst);
        }
        marks
    }
}
