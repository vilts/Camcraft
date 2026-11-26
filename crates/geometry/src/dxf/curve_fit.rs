//! A smooth DXF curve (SPLINE, ELLIPSE) laid as tangent-continuous circular arcs (biarcs) and
//! lines, with a checked two-sided bound on the distance between the curve and the output.
//!
//! Provenance: our own implementation. The biarc with equal tangent lengths is the classical
//! construction (K. M. Bolton, "Biarc curves", Computer-Aided Design 7(2), 1975; D. S. Meek and
//! D. J. Walton, "Approximation of discrete data by G1 arc splines", CAD 24(6), 1992). The
//! bound is our own: a Lipschitz bound on the curve's speed covers the curve between samples,
//! and the unit speed of the output covers the output between the closest points of samples.
//!
//! The bound. For each accepted piece, every curve point is within `tol` of the output and
//! every output point is within `tol` of the curve (a Hausdorff distance of at most `tol`).
//! With samples `t_k` at spacing `h`, distances `d_k` to the output and closest output
//! positions `s_k` (arc length, non-decreasing): a curve point is within `d_k + L h / 2`, and an
//! output point within `d_k + (s_(k+1) - s_k) / 2`, where `L` bounds the curve's speed.
use super::biarc::{Seg, biarc, closest};
use super::types::{DxfError, Primitive, error};
use crate::geo2d::Point2;

/// A parametric plane curve that the fitter reads.
pub(super) trait Curve {
    fn point(&self, t: f64) -> Point2;
    /// The unit tangent at `t` inside the smooth section `[lo, hi]`.
    fn tangent(&self, t: f64, lo: f64, hi: f64) -> Option<(f64, f64)>;
    /// An upper bound of the speed `|C'(t)|` on `[t0, t1]` (inside one knot span).
    fn speed(&self, t0: f64, t1: f64) -> f64;
}

/// The halvings of one initial piece before the curve is refused.
const MAX_DEPTH: u32 = 40;
/// The most samples of one piece's dense check; a piece that needs more is halved first.
/// The drawing's evaluation budget bounds the total.
const MAX_SAMPLES: usize = 1 << 22;

/// The shared limits of one drawing's curves.
pub(super) struct Budget {
    /// Curve evaluations left for the whole drawing.
    pub evaluations: usize,
    /// Output primitives left for the whole drawing.
    pub primitives: usize,
}

/// The fit settings, in drawing units.
pub(super) struct Fit<'a> {
    pub kind: &'a str,
    pub tol: f64,
    pub max_radius: f64,
}

#[derive(Clone, Copy)]
struct End {
    t: f64,
    p: Point2,
    tan: (f64, f64),
}

impl Fit<'_> {
    /// Fit the smooth section `[cuts[0], cuts.last()]`; `cuts` are the initial piece ends
    /// (knot values, quarter turns). `last` replaces the end point (a closed curve's start).
    pub fn section<C: Curve>(
        &self,
        curve: &C,
        cuts: &[f64],
        last: Option<Point2>,
        budget: &mut Budget,
        out: &mut Vec<Primitive>,
    ) -> Result<(), DxfError> {
        let (Some(&lo), Some(&hi)) = (cuts.first(), cuts.last()) else {
            return Ok(());
        };
        let end = |t: f64, budget: &mut Budget| -> Result<End, DxfError> {
            spend(budget, 2, self.kind)?;
            let mut p = curve.point(t);
            if t == hi
                && let Some(q) = last
            {
                p = q;
            }
            let tan = curve
                .tangent(t, lo, hi)
                .ok_or_else(|| error(self.kind, "curve has no direction at a point"))?;
            if !(p.x.is_finite() && p.y.is_finite()) {
                return Err(error(self.kind, "curve point is not finite"));
            }
            Ok(End { t, p, tan })
        };
        // The dense checks alone need about L (t1 - t0) / tol samples: refuse a curve that
        // cannot fit the budget before any work is spent on it.
        let need: f64 = cuts
            .windows(2)
            .map(|w| curve.speed(w[0], w[1]) * (w[1] - w[0]) / self.tol)
            .sum();
        if need.is_nan() || need > budget.evaluations as f64 {
            return Err(error(
                self.kind,
                "curve is too long for its tolerance within the import budget",
            ));
        }
        let mut a = end(lo, budget)?;
        for &t in &cuts[1..] {
            let b = end(t, budget)?;
            self.piece(curve, a, b, lo, hi, budget, out)?;
            a = b;
        }
        Ok(())
    }

    /// Fit one initial piece, halving it until each part passes its check. Parts go out in
    /// curve order (a stack of right halves).
    #[allow(clippy::too_many_arguments)]
    fn piece<C: Curve>(
        &self,
        curve: &C,
        a: End,
        b: End,
        lo: f64,
        hi: f64,
        budget: &mut Budget,
        out: &mut Vec<Primitive>,
    ) -> Result<(), DxfError> {
        let mut stack = vec![(a, b, 0_u32)];
        while let Some((a, b, depth)) = stack.pop() {
            if let Some(segs) = self.accept(curve, a, b, budget)? {
                if segs.len() > budget.primitives {
                    return Err(error(self.kind, "arc count exceeds limit"));
                }
                budget.primitives -= segs.len();
                out.extend(segs.into_iter().map(Seg::primitive));
                continue;
            }
            if depth >= MAX_DEPTH || b.t - a.t <= (hi - lo) * 1e-12 {
                return Err(error(
                    self.kind,
                    &format!(
                        "curve cannot be laid as arcs within {:e} drawing units (a cusp or a sharp turn)",
                        self.tol
                    ),
                ));
            }
            let t = 0.5 * (a.t + b.t);
            spend(budget, 2, self.kind)?;
            let p = curve.point(t);
            let tan = curve
                .tangent(t, lo, hi)
                .ok_or_else(|| error(self.kind, "curve has no direction at a point"))?;
            let m = End { t, p, tan };
            stack.push((m, b, depth + 1));
            stack.push((a, m, depth + 1));
        }
        Ok(())
    }

    /// The output of one piece if it passes the bound, else `None` (halve it).
    fn accept<C: Curve>(
        &self,
        curve: &C,
        a: End,
        b: End,
        budget: &mut Budget,
    ) -> Result<Option<Vec<Seg>>, DxfError> {
        let span = b.t - a.t;
        let speed = curve.speed(a.t, b.t);
        if !speed.is_finite() {
            return Ok(None);
        }
        // A piece whose whole length is under half the tolerance and whose ends meet is
        // within the tolerance of a point: it adds nothing.
        if a.p == b.p {
            return Ok((speed * span <= 0.5 * self.tol).then(Vec::new));
        }
        // Samples so that the speed term L h / 2 is at most tol / 2.
        let need = (speed * span / self.tol).ceil().max(8.0);
        if need > MAX_SAMPLES as f64 {
            return Ok(None);
        }
        let n = need as usize;
        let Some(segs) = biarc(a.p, a.tan, b.p, b.tan, self.max_radius) else {
            return Ok(None);
        };
        // A coarse look first, then the full check.
        for stride in [n.div_ceil(64).max(1), 1] {
            if !self.check(curve, &segs, a, b, n, stride, speed, budget)? {
                return Ok(None);
            }
        }
        Ok(Some(segs))
    }

    /// The bound of the module head over `n` intervals, read at every `stride`-th sample.
    #[allow(clippy::too_many_arguments)]
    fn check<C: Curve>(
        &self,
        curve: &C,
        segs: &[Seg],
        a: End,
        b: End,
        n: usize,
        stride: usize,
        speed: f64,
        budget: &mut Budget,
    ) -> Result<bool, DxfError> {
        let total: f64 = segs.iter().map(Seg::length).sum();
        let h = (b.t - a.t) / n as f64;
        let mut worst: f64 = 0.0;
        let mut gap: f64 = 0.0;
        let mut previous = 0.0;
        let mut k = 0;
        loop {
            let q = if k == 0 {
                a.p
            } else if k == n {
                b.p
            } else {
                spend(budget, 1, self.kind)?;
                curve.point(a.t + h * k as f64)
            };
            let (d, s) = closest(segs, q);
            if !(d.is_finite() && s.is_finite()) || s + 1e-12 * total < previous {
                return Ok(false);
            }
            worst = worst.max(d);
            gap = gap.max(s - previous);
            previous = s.max(previous);
            if worst > 0.5 * self.tol {
                return Ok(false);
            }
            if k == n {
                break;
            }
            k = (k + stride).min(n);
        }
        gap = gap.max(total - previous);
        if stride > 1 {
            return Ok(true);
        }
        let reach = speed * h * 0.5;
        Ok(worst + reach <= self.tol && worst + gap * 0.5 <= self.tol)
    }
}

fn spend(budget: &mut Budget, n: usize, kind: &str) -> Result<(), DxfError> {
    if budget.evaluations < n {
        return Err(error(
            kind,
            "curve needs more work than the import budget allows",
        ));
    }
    budget.evaluations -= n;
    Ok(())
}
