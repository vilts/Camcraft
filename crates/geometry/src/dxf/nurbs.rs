//! The curve of a DXF SPLINE: a checked, possibly rational B-spline in the drawing plane.
//! Provenance: our own implementation of de Boor's algorithm in homogeneous coordinates
//! (C. de Boor, "A Practical Guide to Splines"; L. Piegl and W. Tiller, "The NURBS Book",
//! chapters 2 to 4). The speed bound is the derivative curve's control polygon (non-rational)
//! or, for weights, our own bound from the basis derivative (The NURBS Book, eq. 2.7) and the
//! convex hull property of positive weights.
use super::curve_fit::Curve;
use super::types::{DxfError, error};
use crate::geo2d::Point2;

/// The highest degree read. A higher degree is refused by name.
pub(super) const MAX_DEGREE: usize = 11;
/// The largest ratio of the greatest to the least weight.
const MAX_WEIGHT_RATIO: f64 = 1e6;

pub(super) struct Nurbs {
    degree: usize,
    knots: Vec<f64>,
    points: Vec<Point2>,
    weights: Vec<f64>,
    /// All weights are equal: the curve is non-rational. Found once, not for each span.
    uniform: bool,
}

impl Nurbs {
    /// Check the definition. `weights` is empty for a non-rational spline.
    pub fn new(
        degree: usize,
        knots: Vec<f64>,
        points: Vec<Point2>,
        weights: Vec<f64>,
    ) -> Result<Nurbs, DxfError> {
        const KIND: &str = "SPLINE";
        if degree == 0 || degree > MAX_DEGREE {
            return Err(error(KIND, &format!("degree must be 1 to {MAX_DEGREE}")));
        }
        let n = points.len();
        if n < degree + 1 {
            return Err(error(KIND, "fewer control points than degree + 1"));
        }
        if knots.len() != n + degree + 1 {
            return Err(error(
                KIND,
                "knot count is not the control point count + degree + 1",
            ));
        }
        if knots.iter().any(|k| !k.is_finite()) || knots.windows(2).any(|w| w[1] < w[0]) {
            return Err(error(KIND, "knots must be finite and non-decreasing"));
        }
        if knots[degree] >= knots[n] {
            return Err(error(KIND, "knot vector has an empty domain"));
        }
        // A value that repeats more than degree + 1 times anywhere, or more than degree
        // times inside the domain, breaks the curve apart: refused, not joined by a guess.
        let mut run = 1;
        for i in 1..knots.len() {
            run = if knots[i] == knots[i - 1] { run + 1 } else { 1 };
            let inside = knots[i] > knots[degree] && knots[i] < knots[n];
            if run > degree + 1 || (inside && run > degree) {
                return Err(error(KIND, "knot multiplicity breaks the curve"));
            }
        }
        let weights = if weights.is_empty() {
            vec![1.0; n]
        } else if weights.len() != n {
            return Err(error(KIND, "weight count does not match control points"));
        } else {
            weights
        };
        if weights.iter().any(|w| !w.is_finite() || *w <= 0.0) {
            return Err(error(KIND, "weights must be finite and positive"));
        }
        let (lo, hi) = weights
            .iter()
            .fold((f64::INFINITY, 0.0_f64), |(lo, hi), w| {
                (lo.min(*w), hi.max(*w))
            });
        if hi / lo > MAX_WEIGHT_RATIO {
            return Err(error(KIND, "weights differ by more than a factor of 1e6"));
        }
        let uniform = lo == hi;
        Ok(Nurbs {
            degree,
            knots,
            points,
            weights,
            uniform,
        })
    }

    /// The parameter domain.
    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.points.len()])
    }

    /// The distinct knot values of the domain, ends included: the span boundaries.
    pub fn breaks(&self) -> Vec<f64> {
        let (lo, hi) = self.domain();
        let mut out: Vec<f64> = Vec::new();
        for &k in &self.knots[self.degree..=self.points.len()] {
            if k >= lo && k <= hi && out.last() != Some(&k) {
                out.push(k);
            }
        }
        out
    }

    /// The span index `k` with `knots[k] <= u < knots[k + 1]`; the last span holds the end.
    fn span(&self, u: f64) -> usize {
        let (p, n) = (self.degree, self.points.len());
        let mut lo = p;
        let mut hi = n; // knots[hi] > u or u is the domain end
        if u >= self.knots[n] {
            let mut k = n - 1;
            while k > p && self.knots[k] >= self.knots[n] {
                k -= 1;
            }
            return k;
        }
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.knots[mid] <= u {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }
}

impl Curve for Nurbs {
    fn point(&self, u: f64) -> Point2 {
        let p = self.degree;
        let k = self.span(u);
        // Homogeneous de Boor points (w x, w y, w) of span k.
        let mut d = [[0.0_f64; 3]; MAX_DEGREE + 1];
        for (j, slot) in d.iter_mut().enumerate().take(p + 1) {
            let i = j + k - p;
            let w = self.weights[i];
            *slot = [self.points[i].x * w, self.points[i].y * w, w];
        }
        for r in 1..=p {
            for j in (r..=p).rev() {
                let a = self.knots[j + k - p];
                let b = self.knots[j + 1 + k - r];
                let alpha = (u - a) / (b - a);
                let before = d[j - 1];
                for (c, value) in d[j].iter_mut().enumerate() {
                    *value = (1.0 - alpha) * before[c] + alpha * *value;
                }
            }
        }
        Point2::new(d[p][0] / d[p][2], d[p][1] / d[p][2])
    }

    fn tangent(&self, u: f64, lo: f64, hi: f64) -> Option<(f64, f64)> {
        // A difference quotient: central inside the section, one-sided at its ends, so two
        // pieces that meet at u read the same direction there. Its error only costs arcs:
        // the fit is checked against the curve itself.
        let mut h = (hi - lo) * 1e-7;
        for _ in 0..5 {
            let a = (u - h).max(lo);
            let b = (u + h).min(hi);
            if b > a {
                let (p, q) = (self.point(a), self.point(b));
                let (dx, dy) = (q.x - p.x, q.y - p.y);
                let len = dx.hypot(dy);
                if len > 0.0 && len.is_finite() {
                    return Some((dx / len, dy / len));
                }
            }
            h *= 100.0;
        }
        None
    }

    fn speed(&self, t0: f64, t1: f64) -> f64 {
        let p = self.degree;
        let k = self.span(0.5 * (t0 + t1));
        if self.uniform {
            // Non-rational: C' is the B-spline of degree p - 1 with the control points
            // p (P[i+1] - P[i]) / (knots[i+p+1] - knots[i+1]) (The NURBS Book, eq. 3.4), so
            // |C'| on span k is at most the largest of its p active ones (convex hull).
            return (k - p..k)
                .map(|i| {
                    let (a, b) = (self.points[i], self.points[i + 1]);
                    let width = self.knots[i + p + 1] - self.knots[i + 1];
                    p as f64 * (b.x - a.x).hypot(b.y - a.y) / width
                })
                .fold(0.0, f64::max);
        }
        // Rational: on span k, |C'(u)| <= 2p (wmax / wmin) D / (knots[k+1] - knots[k]), where D
        // is the diameter of the p + 1 active control points (C lies in their convex hull).
        let active = k - p..=k;
        let mut d: f64 = 0.0;
        let (mut wlo, mut whi) = (f64::INFINITY, 0.0_f64);
        for i in active.clone() {
            wlo = wlo.min(self.weights[i]);
            whi = whi.max(self.weights[i]);
            for j in active.clone() {
                let (a, b) = (self.points[i], self.points[j]);
                d = d.max((a.x - b.x).hypot(a.y - b.y));
            }
        }
        let width = self.knots[k + 1] - self.knots[k];
        2.0 * p as f64 * (whi / wlo) * d / width
    }
}
