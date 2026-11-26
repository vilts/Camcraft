//! Convex tangent DXF chains of lines and arcs for a wire punch (#2983).
//! Provenance: the turning-angle theorem of a closed C1 curve and the support function of a
//! circular arc (classical plane geometry).
use super::{FIT, PREVIEW_SAG, distance};
use crate::{
    dxf::Primitive,
    geo2d::{Loop, Point2},
    sketch_chains::ClosedChain,
};
use std::f64::consts::{PI, TAU};

/// The largest number of chain segments of a tangent profile.
pub const MAX_SEGMENTS: usize = 128;
/// The largest number of vertices of the circumscribed tangent preview.
const MAX_PREVIEW: usize = 2048;

/// One nominal directed line, or one counter-clockwise arc of at most a half turn.
/// The coordinates are in the frame of the profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TangentSegment {
    pub start: Point2,
    pub end: Point2,
    pub center: Option<Point2>,
    pub sweep: f64,
}

impl TangentSegment {
    fn direction(&self, at: Point2) -> [f64; 2] {
        let (x, y) = match self.center {
            Some(c) => (c.y - at.y, at.x - c.x),
            None => (self.end.x - self.start.x, self.end.y - self.start.y),
        };
        let len = x.hypot(y);
        [x / len, y / len]
    }

    /// The greatest projection of the exact segment on the unit `normal`.
    pub fn support(&self, normal: [f64; 2]) -> f64 {
        let project = |p: Point2| normal[0] * p.x + normal[1] * p.y;
        let ends = project(self.start).max(project(self.end));
        let Some(c) = self.center else { return ends };
        let start = (self.start.y - c.y).atan2(self.start.x - c.x);
        let angle = (normal[1].atan2(normal[0]) - start).rem_euclid(TAU);
        if angle <= self.sweep {
            ends.max(project(c) + distance(c, self.start))
        } else {
            ends
        }
    }
}

/// A convex closed loop of tangent lines and counter-clockwise arcs, in walk order.
#[derive(Clone, Debug, PartialEq)]
pub struct TangentProfile {
    segments: Vec<TangentSegment>,
}

impl TangentProfile {
    pub fn segments(&self) -> &[TangentSegment] {
        &self.segments
    }

    /// Prove that the chain is one convex loop of tangent lines and arcs.
    ///
    /// # Errors
    /// Refuses a fold, a corner, a clockwise or long arc, and an arc radius
    /// outside 0.5 to 20 mm.
    pub(super) fn from_chain(chain: &ClosedChain) -> Result<Self, String> {
        let invalid = "A wire DXF punch with arcs needs one CIRCLE, a racetrack, or one convex loop of tangent lines and counter-clockwise arcs from 0.5 to 20 mm radius";
        if chain.segments.len() > MAX_SEGMENTS {
            return Err(format!(
                "The wire DXF punch has more than {MAX_SEGMENTS} segments"
            ));
        }
        let mut segments = Vec::with_capacity(chain.segments.len());
        let mut turn = 0.0;
        let mut lines = 0;
        for item in &chain.segments {
            let (start, end) = (item.start, item.end);
            if ![start.x, start.y, end.x, end.y]
                .iter()
                .all(|v| v.is_finite())
                || distance(start, end) < 0.001
            {
                return Err(invalid.into());
            }
            let (center, sweep) = match item.curve {
                Primitive::Line { .. } => {
                    lines += 1;
                    (None, 0.)
                }
                Primitive::Arc {
                    center,
                    radius,
                    sweep_angle,
                    ..
                } => {
                    if !center.x.is_finite()
                        || !center.y.is_finite()
                        || !(0.5..=20.).contains(&radius)
                        || !(0.0..=PI + 1e-12).contains(&sweep_angle)
                        || (distance(start, center) - radius).abs() > FIT
                        || (distance(end, center) - radius).abs() > FIT
                    {
                        return Err(invalid.into());
                    }
                    let alpha = (start.y - center.y).atan2(start.x - center.x);
                    let (s, c) = (alpha + sweep_angle).sin_cos();
                    let on = Point2::new(center.x + radius * c, center.y + radius * s);
                    if distance(on, end) > FIT {
                        return Err(invalid.into());
                    }
                    turn += sweep_angle;
                    (Some(center), sweep_angle)
                }
                _ => return Err(invalid.into()),
            };
            segments.push(TangentSegment {
                start,
                end,
                center,
                sweep,
            });
        }
        // For one simple C1 closed walk, positive arc turns that total one
        // revolution make every tangent a supporting line of a convex body.
        if lines == 0 || (turn - TAU).abs() > 1e-9 {
            return Err(invalid.into());
        }
        for (current, next) in segments.iter().zip(segments.iter().cycle().skip(1)) {
            let a = current.direction(current.end);
            let b = next.direction(next.start);
            if distance(current.end, next.start) > FIT || (a[0] - b[0]).hypot(a[1] - b[1]) > 1e-9 {
                return Err(invalid.into());
            }
        }
        Ok(Self { segments })
    }

    /// Move the joints and centres with a rigid transform. Radii and sweeps do not change.
    pub(super) fn place(&mut self, transform: impl Fn(Point2) -> Point2) {
        for segment in &mut self.segments {
            segment.start = transform(segment.start);
            segment.end = transform(segment.end);
            segment.center = segment.center.map(&transform);
        }
    }

    /// The greatest projection of the exact profile on the unit `normal`.
    pub fn support(&self, normal: [f64; 2]) -> f64 {
        self.segments
            .iter()
            .map(|s| s.support(normal))
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// The exact axis-aligned bounds of the lines and arcs.
    pub fn bounds(&self) -> [Point2; 2] {
        [
            Point2::new(-self.support([-1., 0.]), -self.support([0., -1.])),
            Point2::new(self.support([1., 0.]), self.support([0., 1.])),
        ]
    }

    /// A circumscribed preview polygon: the intersections of tangents to the
    /// arcs. Each edge is checked against the analytic support of the profile.
    pub(super) fn preview(&self) -> Result<Loop, String> {
        // The small pad covers the joint arithmetic and the downstream
        // 0.000001 mm clipping grid, inside the radial bound.
        const PAD: f64 = 0.000002;
        let mut vertices = Vec::new();
        for segment in &self.segments {
            let Some(c) = segment.center else { continue };
            let radius = distance(c, segment.start);
            let half_step = ((radius + PAD) / (radius + PREVIEW_SAG - PAD)).acos();
            let count = (segment.sweep / (2.0 * half_step)).ceil().max(1.);
            if !count.is_finite() || count > (MAX_PREVIEW - vertices.len()) as f64 {
                return Err(format!(
                    "The DXF wire preview exceeds {MAX_PREVIEW} tangent vertices"
                ));
            }
            let step = segment.sweep / count;
            let start = (segment.start.y - c.y).atan2(segment.start.x - c.x);
            let outer = (radius + PAD) / (step * 0.5).cos();
            for i in 0..count as usize {
                let (s, cos) = (start + (i as f64 + 0.5) * step).sin_cos();
                vertices.push(Point2::new(c.x + outer * cos, c.y + outer * s));
            }
        }
        let ring = Loop::new(vertices).map_err(|e| e.to_string())?;
        for (&a, &b) in ring
            .points()
            .iter()
            .zip(ring.points().iter().cycle().skip(1))
        {
            let len = distance(a, b);
            let outward = [(b.y - a.y) / len, (a.x - b.x) / len];
            let allowance = a.x * outward[0] + a.y * outward[1] - self.support(outward);
            if !(PAD * 0.5..=PREVIEW_SAG).contains(&allowance) {
                return Err(
                    "The tangent preview could not enclose the complete DXF profile".into(),
                );
            }
        }
        Ok(ring)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_oblique_arc_has_its_true_extremum_between_the_joins() {
        // This 143 degree arc starts at the bottom and ends at the 3:4 radius.
        let arc = TangentSegment {
            start: Point2::new(24., -5.),
            end: Point2::new(27., 4.),
            center: Some(Point2::new(24., 0.)),
            sweep: PI / 2. + 4.0_f64.atan2(3.),
        };
        assert_eq!(arc.support([1., 0.]), 29.);
        assert_eq!(arc.support([0., 1.]), 4.);
        assert_eq!(arc.support([0., -1.]), 5.);
    }
}
