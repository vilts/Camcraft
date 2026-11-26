//! A selected DXF circle, tangent racetrack or convex tangent loop for a wire punch, and its
//! circumscribed preview polygon. Provenance: classical circle and tangent-line geometry (#2022,
//! #2983).
use crate::dxf::Primitive;
use crate::geo2d::{Loop, Point2};
use crate::sketch_chains::ClosedChain;
use std::f64::consts::PI;

mod tangent;
pub use tangent::{MAX_SEGMENTS, TangentProfile, TangentSegment};

/// The joint and tangency tolerance in mm.
const FIT: f64 = 1e-8;
/// The largest radial excess of the preview polygon over a true arc, in mm.
pub const PREVIEW_SAG: f64 = 0.000025;

/// The centre of a circle, or the two cap centres of a racetrack.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Spine {
    Circle(Point2),
    Stadium([Point2; 2]),
}

/// A circle or a tangent racetrack, read from the analytic DXF primitives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundProfile {
    pub spine: Spine,
    pub radius: f64,
}

fn distance(a: Point2, b: Point2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

impl RoundProfile {
    /// Read one CIRCLE, or prove that four primitives form a racetrack: two
    /// straight sides tangent to two semicircles of the same radius. A joint
    /// inside the drawing tolerance alone does not prove tangency.
    ///
    /// # Errors
    /// Refuses any other chain, an ambiguous nesting and a joint gap.
    pub fn from_chain(chain: &ClosedChain) -> Result<Self, String> {
        let failure = "A wire DXF punch with arcs needs one CIRCLE, or one closed racetrack of two tangent lines and two semicircles";
        if !chain.nesting.as_ref().is_ok_and(|n| n.depth == 0) || chain.max_gap > FIT {
            return Err(failure.into());
        }
        if let [segment] = chain.segments.as_slice()
            && let Primitive::Circle { center, radius } = segment.curve
        {
            if !(0.5..=20.).contains(&radius) {
                return Err("The wire DXF circle radius must be from 0.5 to 20 mm".into());
            }
            return Ok(Self {
                spine: Spine::Circle(center),
                radius,
            });
        }
        if chain.segments.len() != 4 {
            return Err(failure.into());
        }
        let mut arcs = Vec::new();
        let mut lines = Vec::new();
        for segment in &chain.segments {
            match segment.curve {
                Primitive::Line { .. } => lines.push([segment.start, segment.end]),
                // The chain is counter-clockwise, so a convex cap sweeps +π.
                Primitive::Arc {
                    center,
                    radius,
                    sweep_angle,
                    ..
                } if (sweep_angle - PI).abs() * radius <= FIT => {
                    arcs.push((center, radius, segment.start, segment.end));
                }
                _ => return Err(failure.into()),
            }
        }
        let [(a, r, p, q), (b, s, u, v)] = arcs.as_slice() else {
            return Err(failure.into());
        };
        let shape = Self {
            spine: Spine::Stadium([*a, *b]),
            radius: *r,
        };
        if !(0.5..=20.).contains(r) || (r - s).abs() > FIT || distance(*a, *b) < 6.0 {
            return Err("The wire DXF racetrack needs equal cap radii from 0.5 to 20 mm and straight sides at least 6 mm long".into());
        }
        let [ar, br, bl, al] = shape.tangencies([*a, *b]);
        let close = |a, b| distance(a, b) <= FIT;
        if !close(*p, al)
            || !close(*q, ar)
            || !close(*u, br)
            || !close(*v, bl)
            || !lines.iter().any(|&[l, h]| close(l, ar) && close(h, br))
            || !lines.iter().any(|&[l, h]| close(l, bl) && close(h, al))
        {
            return Err(failure.into());
        }
        Ok(shape)
    }

    /// The circle centre twice, or the two cap centres.
    pub fn centers(&self) -> [Point2; 2] {
        match self.spine {
            Spine::Circle(c) => [c, c],
            Spine::Stadium(pair) => pair,
        }
    }

    /// Move the centres with a rigid transform. The radius does not change.
    pub fn place_centers(&mut self, transform: impl Fn(Point2) -> Point2) {
        self.spine = match self.spine {
            Spine::Circle(c) => Spine::Circle(transform(c)),
            Spine::Stadium(pair) => Spine::Stadium(pair.map(transform)),
        };
    }

    /// Tangency points in counter-clockwise order: the first straight side,
    /// the cap at `b`, the return side, and the cap at `a`.
    pub fn tangencies(&self, [a, b]: [Point2; 2]) -> [Point2; 4] {
        let scale = self.radius / distance(a, b);
        let x = (b.y - a.y) * scale;
        let y = (a.x - b.x) * scale;
        [
            Point2::new(a.x + x, a.y + y),
            Point2::new(b.x + x, b.y + y),
            Point2::new(b.x - x, b.y - y),
            Point2::new(a.x - x, a.y - y),
        ]
    }

    /// A circumscribed preview polygon. The intersections of consecutive
    /// tangents enclose each true arc with a radial excess of at most
    /// [`PREVIEW_SAG`].
    ///
    /// # Errors
    /// Refuses a radius that needs more than 1024 sides for each half.
    pub fn preview(&self) -> Result<Loop, String> {
        let n = (PI / (2. * (self.radius / (self.radius + PREVIEW_SAG)).acos())).ceil();
        if !n.is_finite() || !(2.0..=1024.0).contains(&n) {
            return Err("The DXF arc exceeds the verified wire preview resolution".into());
        }
        let step = PI / n;
        let outer = self.radius / (step / 2.).cos();
        let [a, b] = self.centers();
        let [br, al] = match self.spine {
            Spine::Circle(c) => [
                Point2::new(c.x + self.radius, c.y),
                Point2::new(c.x - self.radius, c.y),
            ],
            Spine::Stadium(pair) => {
                let [_, br, _, al] = self.tangencies(pair);
                [br, al]
            }
        };
        let mut points = Vec::with_capacity(2 * n as usize);
        for (center, start) in [(b, br), (a, al)] {
            let angle = (start.y - center.y).atan2(start.x - center.x);
            for i in 0..n as usize {
                let (sin, cos) = (angle + (i as f64 + 0.5) * step).sin_cos();
                points.push(Point2::new(center.x + outer * cos, center.y + outer * sin));
            }
        }
        Loop::new(points).map_err(|e| e.to_string())
    }
}

/// The analytic curves of one selected wire punch chain (#2983).
#[derive(Clone, Debug, PartialEq)]
pub enum WireProfile {
    /// A circle or a racetrack: the control-checked shapes of #2022.
    Round(RoundProfile),
    /// Any other convex loop of tangent lines and counter-clockwise arcs.
    Tangent(TangentProfile),
}

impl WireProfile {
    /// Read a circle or racetrack first. A different chain must be a convex
    /// tangent loop. A single circle keeps its own refusal.
    ///
    /// # Errors
    /// Refuses an ambiguous nesting, a joint gap and every other chain.
    pub fn from_chain(chain: &ClosedChain) -> Result<Self, String> {
        match RoundProfile::from_chain(chain) {
            Ok(shape) => Ok(Self::Round(shape)),
            Err(why)
                if chain.segments.len() == 1
                    || !chain.nesting.as_ref().is_ok_and(|n| n.depth == 0)
                    || chain.max_gap > FIT =>
            {
                Err(why)
            }
            Err(_) => TangentProfile::from_chain(chain).map(Self::Tangent),
        }
    }

    /// Move the profile with a rigid transform.
    pub fn place(&mut self, transform: impl Fn(Point2) -> Point2) {
        match self {
            Self::Round(shape) => shape.place_centers(transform),
            Self::Tangent(shape) => shape.place(transform),
        }
    }

    /// The exact axis-aligned bounds of the nominal curves.
    pub fn bounds(&self) -> [Point2; 2] {
        match self {
            Self::Round(shape) => {
                let [a, b] = shape.centers();
                let r = shape.radius;
                [
                    Point2::new(a.x.min(b.x) - r, a.y.min(b.y) - r),
                    Point2::new(a.x.max(b.x) + r, a.y.max(b.y) + r),
                ]
            }
            Self::Tangent(shape) => shape.bounds(),
        }
    }

    /// A circumscribed preview polygon within [`PREVIEW_SAG`] of each arc.
    ///
    /// # Errors
    /// Refuses a profile that needs too many preview vertices.
    pub fn preview(&self) -> Result<Loop, String> {
        match self {
            Self::Round(shape) => shape.preview(),
            Self::Tangent(shape) => shape.preview(),
        }
    }
}

#[cfg(test)]
mod tests;
