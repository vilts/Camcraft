//! Conservative G76 meridian coverage by swept inserts and axial drill cones;
//! convex-set geometry from machine-tool insert shapes, our own implementation.

type Rect = (f64, f64, f64, f64);

/// A prior feed segment with its cutting insert dimensions.
/// Endpoints are `(axial station, signed radius)` in the spindle frame.
pub struct AheadCut {
    pub p: (f64, f64),
    pub q: (f64, f64),
    pub nose: f64,
    pub band: f64,
}

/// An axial drill cut, from its point station to its entry station.
/// Coordinates are `(axial station, radius)` in the same frame as [`AheadCut`].
pub struct PilotCut {
    pub lo: f64,
    pub hi: f64,
    pub r: f64,
    /// Cotangent of the drill half-angle; `None` denotes a flat bottom.
    pub cot: Option<f64>,
}

impl PilotCut {
    fn removed(&self, s: f64, r: f64) -> bool {
        if s < self.lo || s > self.hi {
            return false;
        }
        let wide = match self.cot {
            Some(k) if k > 0.0 => ((s - self.lo) / k).min(self.r),
            _ => self.r,
        };
        r < wide
    }
}

fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let l2 = vx * vx + vy * vy;
    let t = if l2 > 0.0 {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.0 - a.0 - vx * t).hypot(p.1 - a.1 - vy * t)
}

fn corners((lo, hi, bottom, top): Rect) -> [(f64, f64); 4] {
    [(lo, bottom), (hi, bottom), (lo, top), (hi, top)]
}

impl AheadCut {
    fn covers(&self, rect: Rect, margin: f64) -> bool {
        if ![self.p.0, self.p.1, self.q.0, self.q.1, self.nose, self.band]
            .iter()
            .all(|v| v.is_finite())
        {
            return false;
        }
        if self.band > 0.0 {
            // The sweep is the Minkowski sum of the feed segment, the
            // axial blade band and the outward radial ray: a convex set.
            // Do not extend a diagonal cut's deepest tip across its full
            // axial span. Solve the permitted segment parameter instead.
            if self.p.1 != 0.0 && self.q.1 != 0.0 && self.p.1.signum() != self.q.1.signum() {
                return false;
            }
            return corners(rect).iter().all(|&(s, r)| {
                let ds = self.q.0 - self.p.0;
                let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
                let band = self.band - margin;
                if band < 0.0 {
                    return false;
                }
                if ds == 0.0 {
                    if (s - self.p.0).abs() > band {
                        return false;
                    }
                } else {
                    let (a, b) = ((s - band - self.p.0) / ds, (s + band - self.p.0) / ds);
                    t0 = t0.max(a.min(b));
                    t1 = t1.min(a.max(b));
                    if t0 > t1 {
                        return false;
                    }
                }
                let dr = self.q.1.abs() - self.p.1.abs();
                let tip = self.p.1.abs() + dr * if dr >= 0.0 { t0 } else { t1 };
                r >= tip + margin
            });
        }
        let reach = self.nose - margin;
        if !reach.is_finite() || reach <= 0.0 {
            return false;
        }
        // Each signed segment capsule is convex. All four corners must
        // belong to the SAME capsule; mixing signs could bridge a gap.
        [1.0, -1.0].iter().any(|sign| {
            corners(rect)
                .iter()
                .all(|&(s, r)| seg_dist((s, sign * r), self.p, self.q) < reach)
        })
    }
}

/// Prove full coverage of `(axial low, axial high, radial low, radial high)`.
/// All cut values must use the same spindle frame. Never infer coverage from samples. Each accepted cell
/// lies in one convex removed region because all its corners do. Cells
/// spanning several cuts are subdivided; exhausted budgets refuse.
pub fn rectangle_clear(rect: Rect, ahead: &[AheadCut], pilots: &[PilotCut], margin: f64) -> bool {
    if ![rect.0, rect.1, rect.2, rect.3, margin]
        .iter()
        .all(|x| x.is_finite())
        || margin < 0.0
    {
        return false;
    }
    let mut pending = vec![(rect, 0_u8)];
    let mut budget = 100_000_usize;
    while let Some((cell, depth)) = pending.pop() {
        if budget == 0 {
            return false;
        }
        budget -= 1;
        // A pilot is a capped cone: its meridian region is convex too.
        if pilots
            .iter()
            .any(|p| corners(cell).iter().all(|&(s, r)| p.removed(s, r)))
            || ahead.iter().any(|cut| cut.covers(cell, margin))
        {
            continue;
        }
        if depth == 24 {
            return false;
        }
        let (lo, hi, bottom, top) = cell;
        let (left, right) = if hi - lo >= top - bottom {
            let mid = lo + (hi - lo) * 0.5;
            if !(mid > lo && mid < hi) {
                return false;
            }
            ((lo, mid, bottom, top), (mid, hi, bottom, top))
        } else {
            let mid = bottom + (top - bottom) * 0.5;
            if !(mid > bottom && mid < top) {
                return false;
            }
            ((lo, hi, bottom, mid), (lo, hi, mid, top))
        };
        pending.push((left, depth + 1));
        pending.push((right, depth + 1));
    }
    true
}
