//! The ball atom, split to keep Dexel below the module size limit.
//! Provenance: the existing closed-form sphere sweep in dexel.rs, unchanged.

use super::dexel::{reach, Dexel};
use crate::geometry::Point3;

impl Dexel {
    /// The ball-nose atom: a sphere of radius `r` whose CENTER travels
    /// the straight, Z-linear chord `a -> b`. Above a cell center `q`,
    /// at instant `t`, the sphere's bottom sits at
    /// `z(t) - sqrt(r^2 - d(t)^2)` with `d` the horizontal distance
    /// from `q` to the center path; the cut is the minimum of that
    /// over the reach window. Endpoints do NOT suffice here (unlike
    /// `cut_chord`, where the bottom is constant along the chord): the
    /// bottom function is convex in `t`, so its minimum is at a window
    /// end or at its single interior stationary point - `f'(t) = k +
    /// (t*D^2 - w.D)/sqrt(r^2 - d^2)`, whose squaring gives the
    /// quadratic below, and the unsquared law keeps only roots where
    /// `(w.D - t*D^2)` shares `k`'s sign. The XY-nearest instant `t_c`
    /// is always evaluated too, so a root lost to rounding degrades
    /// toward the chord's mid-sagitta, never past an endpoint.
    pub fn cut_ball(&mut self, a: Point3, b: Point3, r: f64) {
        if !(a.is_finite() && b.is_finite()) || !(r.is_finite() && r >= 0.0) {
            return; // the cut_chord gate: validation lives upstream
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let k = b.z - a.z;
        let fx = |v: f64| ((v - self.x0) / self.cell).floor();
        let fy = |v: f64| ((v - self.y0) / self.cell).floor();
        let lo_x = fx(a.x.min(b.x) - r).max(0.0) as usize;
        let hi_x = fx(a.x.max(b.x) + r).clamp(0.0, self.nx as f64 - 1.0) as usize;
        let lo_y = fy(a.y.min(b.y) - r).max(0.0) as usize;
        let hi_y = fy(a.y.max(b.y) + r).clamp(0.0, self.ny as f64 - 1.0) as usize;
        if lo_x >= self.nx || lo_y >= self.ny {
            return;
        }
        let r2 = r * r;
        for iy in lo_y..=hi_y {
            for ix in lo_x..=hi_x {
                let q = self.center(ix, iy);
                let (wx, wy) = (q.x - a.x, q.y - a.y);
                let Some((t0, t1)) = reach(wx, wy, dx, dy, r) else {
                    continue;
                };
                let f = |t: f64| {
                    let ex = wx - dx * t;
                    let ey = wy - dy * t;
                    a.z + k * t - (r2 - (ex * ex + ey * ey)).max(0.0).sqrt()
                };
                let t_c = if dd > f64::EPSILON {
                    (wx * dx + wy * dy) / dd
                } else {
                    0.0
                };
                let mut cut = f(t0).min(f(t1)).min(f(t_c.clamp(t0, t1)));
                if dd > f64::EPSILON {
                    let wd = wx * dx + wy * dy;
                    let s = dd + k * k;
                    let qa = dd * s;
                    let qb = -2.0 * wd * s;
                    let qc = wd * wd + k * k * (wx * wx + wy * wy - r2);
                    let mut disc = qb * qb - 4.0 * qa * qc;
                    if disc < 0.0 && disc > -qb * qb * 1e-12 {
                        disc = 0.0; // a planar double root erased by rounding
                    }
                    if disc >= 0.0 && disc.is_finite() {
                        let sd = disc.sqrt();
                        for t in [(-qb - sd) / (2.0 * qa), (-qb + sd) / (2.0 * qa)] {
                            if t > t0 && t < t1 && (wd - t * dd) * k >= 0.0 {
                                cut = cut.min(f(t));
                            }
                        }
                    }
                }
                self.lower(ix + iy * self.nx, cut);
            }
        }
    }

}
