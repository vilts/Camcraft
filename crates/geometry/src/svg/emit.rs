//! SVG geometry to sketch primitives: lines, flattened Beziers, and elliptical arcs that stay
//! analytic arcs where the mapped ellipse is a circle.
//! Provenance: the W3C SVG 1.1 implementation notes (endpoint to centre arc conversion, radius
//! correction), the Bernstein form of Bezier curves, and the classic second-difference bound on
//! the chord error of a polynomial curve (Wang's formula); original implementation.

use super::transform::Affine;
use super::{SvgError, invalid};
use crate::dxf::{LayeredPrimitive, Primitive};
use crate::geo2d::Point2;
use std::f64::consts::TAU;

/// The most segments one curve is cut into.
const MAX_CURVE_SEGMENTS: usize = 20_000;

fn dist(a: Point2, b: Point2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Where the primitives go: the output transform, the tolerances in output units, the layer
/// and the vertex budget.
pub(super) struct Sink {
    pub map: Affine,
    /// Largest distance of a flattened curve from its true path.
    pub chord: f64,
    /// Shortest flattened segment; closer vertices merge.
    pub min_len: f64,
    pub layer: String,
    pub max_vertices: usize,
    pub out: Vec<LayeredPrimitive>,
}

impl Sink {
    fn push(&mut self, primitive: Primitive) -> Result<(), SvgError> {
        if self.out.len() >= self.max_vertices {
            return Err(invalid(format!(
                "more than {} segments: the drawing is larger than the import budget",
                self.max_vertices
            )));
        }
        self.out.push(LayeredPrimitive { layer: self.layer.clone(), primitive });
        Ok(())
    }

    /// A straight line between two user-space points; a zero-length line is dropped.
    pub fn line(&mut self, a: Point2, b: Point2) -> Result<(), SvgError> {
        let (start, end) = (self.map.apply(a), self.map.apply(b));
        if dist(start, end) <= self.min_len * 1e-6 {
            return Ok(());
        }
        self.push(Primitive::Line { start, end })
    }

    /// Half of the chord bound: the sampled pieces keep within it of the true curve, and a
    /// merge of short pieces may move the path by at most the other half. Distance to a
    /// segment is convex, so the merged path stays within the whole bound.
    fn flatten_chord(&self) -> f64 {
        self.chord / 2.0
    }

    /// Merge short pieces only when the omitted vertices lie within half the chord bound of
    /// the replacement segment (a cusp node with a retracted handle). Otherwise refuse the
    /// curve: the merge would break the chord bound.
    fn polyline(&mut self, curve: &str, from: Point2, inner: impl Iterator<Item = Point2>, to: Point2) -> Result<(), SvgError> {
        let mut last = from;
        let mut omitted = Vec::new();
        for p in inner {
            if dist(last, p) >= self.min_len && dist(p, to) >= self.min_len {
                self.check_merge(curve, last, p, &omitted)?;
                self.push(Primitive::Line { start: last, end: p })?;
                last = p;
                omitted.clear();
            } else {
                omitted.push(p);
            }
        }
        self.check_merge(curve, last, to, &omitted)?;
        if dist(last, to) > 0.0 {
            self.push(Primitive::Line { start: last, end: to })?;
        }
        Ok(())
    }

    fn check_merge(&self, curve: &str, from: Point2, to: Point2, omitted: &[Point2]) -> Result<(), SvgError> {
        let slack = self.chord - self.flatten_chord();
        // A NaN distance is not within the slack, so it is refused too.
        let within = |p: Point2| Self::segment_distance(from, to, p) <= slack;
        if !omitted.iter().all(|&p| within(p)) {
            return Err(invalid(format!("{curve} cannot keep the chord bound when short segments merge")));
        }
        Ok(())
    }

    fn on_segment(from: Point2, to: Point2, p: Point2) -> bool {
        let cross = (p.x - from.x) * (to.y - from.y) - (p.y - from.y) * (to.x - from.x);
        cross == 0.0 && p.x >= from.x.min(to.x) && p.x <= from.x.max(to.x)
            && p.y >= from.y.min(to.y) && p.y <= from.y.max(to.y)
    }

    fn segment_distance(from: Point2, to: Point2, p: Point2) -> f64 {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((p.x - from.x) * dx + (p.y - from.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        dist(p, Point2::new(from.x + t * dx, from.y + t * dy))
    }

    fn segment_budget(curve: &str, required: f64, minimum: usize) -> Result<usize, SvgError> {
        if !required.is_finite() || required > MAX_CURVE_SEGMENTS as f64 {
            return Err(invalid(format!("{curve} requires more than {MAX_CURVE_SEGMENTS} segments: the curve exceeds the import budget")));
        }
        Ok((required as usize).max(minimum))
    }

    /// Segment count for a curve whose largest second difference of control points is `m`
    /// (output units) and whose degree is `degree`.
    fn segments(&self, curve: &str, degree: f64, m: f64) -> Result<usize, SvgError> {
        let n = (degree * (degree - 1.0) / 8.0 * m / self.flatten_chord()).sqrt().ceil();
        Self::segment_budget(curve, n, 1)
    }

    pub fn cubic(&mut self, p0: Point2, c1: Point2, c2: Point2, p3: Point2) -> Result<(), SvgError> {
        let [p0, p1, p2, p3] = [p0, c1, c2, p3].map(|p| self.map.apply(p));
        // The convex hull is the endpoint segment: avoid roundoff from sampling a straight curve.
        if [p1, p2].into_iter().all(|p| Self::on_segment(p0, p3, p)) {
            return self.polyline("cubic curve", p0, std::iter::empty(), p3);
        }
        let second = |a: Point2, b: Point2, c: Point2| (a.x - 2.0 * b.x + c.x).hypot(a.y - 2.0 * b.y + c.y);
        let n = self.segments("cubic curve", 3.0, second(p0, p1, p2).max(second(p1, p2, p3)))?;
        let at = |t: f64| {
            let u = 1.0 - t;
            let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            Point2::new(
                w0 * p0.x + w1 * p1.x + w2 * p2.x + w3 * p3.x,
                w0 * p0.y + w1 * p1.y + w2 * p2.y + w3 * p3.y,
            )
        };
        self.polyline("cubic curve", p0, (1..n).map(|i| at(i as f64 / n as f64)), p3)
    }

    pub fn quad(&mut self, p0: Point2, c: Point2, p2: Point2) -> Result<(), SvgError> {
        let [p0, p1, p2] = [p0, c, p2].map(|p| self.map.apply(p));
        if Self::on_segment(p0, p2, p1) {
            return self.polyline("quadratic curve", p0, std::iter::empty(), p2);
        }
        let n = self.segments("quadratic curve", 2.0, (p0.x - 2.0 * p1.x + p2.x).hypot(p0.y - 2.0 * p1.y + p2.y))?;
        let at = |t: f64| {
            let u = 1.0 - t;
            let (w0, w1, w2) = (u * u, 2.0 * u * t, t * t);
            Point2::new(w0 * p0.x + w1 * p1.x + w2 * p2.x, w0 * p0.y + w1 * p1.y + w2 * p2.y)
        };
        self.polyline("quadratic curve", p0, (1..n).map(|i| at(i as f64 / n as f64)), p2)
    }

    /// The ellipse `centre + R(phi) diag(rx, ry) (cos t, sin t)` for `t` from `t0` over `dt`
    /// (radians; `|dt| >= TAU` is the whole ellipse), all in user space.
    pub fn ellipse(&mut self, centre: Point2, rx: f64, ry: f64, phi: f64, t0: f64, dt: f64) -> Result<(), SvgError> {
        let (s, c) = phi.sin_cos();
        let m = self.map;
        // The linear part of map * R(phi) * diag(rx, ry).
        let (m00, m10) = ((m.a * c + m.c * s) * rx, (m.b * c + m.d * s) * rx);
        let (m01, m11) = ((-m.a * s + m.c * c) * ry, (-m.b * s + m.d * c) * ry);
        let o = m.apply(centre);
        let on = |t: f64| Point2::new(o.x + m00 * t.cos() + m01 * t.sin(), o.y + m10 * t.cos() + m11 * t.sin());
        let (p, r, q) = (m00 * m00 + m10 * m10, m01 * m01 + m11 * m11, m00 * m01 + m10 * m11);
        let big = p.max(r);
        let whole = dt.abs() >= TAU - 1e-12;
        let radius = ((p + r) / 2.0).sqrt();
        let spread = (((p - r) / 2.0).powi(2) + q * q).sqrt();
        let largest = ((p + r) / 2.0 + spread).sqrt();
        let smallest = (((p + r) / 2.0 - spread).max(0.0)).sqrt();
        let circle_error = (largest - radius).abs().max((smallest - radius).abs());
        // Singular values bound the distance to a circle. An arc also aligns its
        // start angle to the ellipse, which can add at most twice the same error again.
        let analytic_error = if whole { circle_error } else { 3.0 * circle_error };
        if (p - r).abs() <= 1e-9 * big && q.abs() <= 1e-9 * big && analytic_error <= self.chord {
            // A circle within the absolute chord bound: keep it analytic.
            if whole {
                return self.push(Primitive::Circle { center: o, radius });
            }
            let start = on(t0);
            let start_angle = (start.y - o.y).atan2(start.x - o.x);
            let sweep = dt * (m00 * m11 - m01 * m10).signum();
            let (start_angle, sweep_angle) = if sweep < 0.0 { (start_angle + sweep, -sweep) } else { (start_angle, sweep) };
            return self.push(Primitive::Arc { center: o, radius, start_angle, sweep_angle });
        }
        let dt = if whole { TAU.copysign(dt) } else { dt };
        let flat = self.flatten_chord();
        let step = if flat < largest { 2.0 * (1.0 - flat / largest).acos() } else { dt.abs() };
        let n = (dt.abs() / step).ceil();
        let n = Self::segment_budget("ellipse", n, 4)?;
        let (from, to) = (on(t0), if whole { on(t0) } else { on(t0 + dt) });
        self.polyline("ellipse", from, (1..n).map(|i| on(t0 + dt * i as f64 / n as f64)), to)
    }

    /// The SVG arc from `p0` to `p1` (endpoint form, user space). Equal ends draw nothing; a zero
    /// radius is refused as a degenerate arc.
    #[allow(clippy::too_many_arguments)]
    pub fn arc(&mut self, p0: Point2, rx: f64, ry: f64, phi_deg: f64, large: bool, sweep: bool, p1: Point2) -> Result<(), SvgError> {
        if p0 == p1 {
            return Ok(());
        }
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if rx == 0.0 || ry == 0.0 {
            return Err(invalid(format!(
                "degenerate arc from ({}, {}) to ({}, {}): a radius is zero",
                p0.x, p0.y, p1.x, p1.y
            )));
        }
        let phi = phi_deg.to_radians();
        let (s, c) = phi.sin_cos();
        let (hx, hy) = ((p0.x - p1.x) / 2.0, (p0.y - p1.y) / 2.0);
        let (x1, y1) = (c * hx + s * hy, -s * hx + c * hy);
        let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
        if lambda > 1.0 {
            rx *= lambda.sqrt();
            ry *= lambda.sqrt();
        }
        let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
        let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
        let k = if den > 0.0 { (num / den).sqrt() } else { 0.0 };
        let k = if large == sweep { -k } else { k };
        let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
        let centre = Point2::new(c * cx1 - s * cy1 + (p0.x + p1.x) / 2.0, s * cx1 + c * cy1 + (p0.y + p1.y) / 2.0);
        let angle = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        let (ux, uy) = ((x1 - cx1) / rx, (y1 - cy1) / ry);
        let (vx, vy) = ((-x1 - cx1) / rx, (-y1 - cy1) / ry);
        let t0 = angle(1.0, 0.0, ux, uy);
        let mut dt = angle(ux, uy, vx, vy);
        if !sweep && dt > 0.0 {
            dt -= TAU;
        } else if sweep && dt < 0.0 {
            dt += TAU;
        }
        if !(t0.is_finite() && dt.is_finite() && centre.x.is_finite() && centre.y.is_finite()) {
            return Err(invalid("degenerate arc: its centre cannot be found"));
        }
        // Never a whole turn: an arc between two distinct points is less than one.
        let dt = dt.clamp(-TAU + 1e-9, TAU - 1e-9);
        self.ellipse(centre, rx, ry, phi, t0, dt)
    }
}
