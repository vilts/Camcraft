//! Provenance: shared sketch builders for the sketch-chaining suites (issue 1861).
#![allow(dead_code)]

use camcraft_geometry::dxf::{LayeredPrimitive, Primitive};
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::sketch_chains::{ClosedChain, LoopRole};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// The pinned identity of `slot("cut", 0.0)`.
pub const SLOT_ID: &str = "d38b0c1165832c5bd78b0bdc79473e80";

pub fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

pub fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

fn on(layer: &str, primitive: Primitive) -> LayeredPrimitive {
    LayeredPrimitive {
        layer: layer.into(),
        primitive,
    }
}

pub fn line(layer: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> LayeredPrimitive {
    on(
        layer,
        Primitive::Line {
            start: p(x0, y0),
            end: p(x1, y1),
        },
    )
}

pub fn arc(
    layer: &str,
    cx: f64,
    cy: f64,
    radius: f64,
    start_angle: f64,
    sweep_angle: f64,
) -> LayeredPrimitive {
    on(
        layer,
        Primitive::Arc {
            center: p(cx, cy),
            radius,
            start_angle,
            sweep_angle,
        },
    )
}

pub fn circle(layer: &str, cx: f64, cy: f64, radius: f64) -> LayeredPrimitive {
    on(
        layer,
        Primitive::Circle {
            center: p(cx, cy),
            radius,
        },
    )
}

/// The same curve drawn the other way.
pub fn flip(item: &LayeredPrimitive) -> LayeredPrimitive {
    let primitive = match item.primitive {
        Primitive::Line { start, end } => Primitive::Line {
            start: end,
            end: start,
        },
        Primitive::Arc {
            center,
            radius,
            start_angle,
            sweep_angle,
        } => Primitive::Arc {
            center,
            radius,
            start_angle: start_angle + sweep_angle,
            sweep_angle: -sweep_angle,
        },
        Primitive::Circle { center, radius } => Primitive::Circle { center, radius },
    };
    on(&item.layer, primitive)
}

/// Four sides of an axis-aligned rectangle, counterclockwise.
pub fn rect(layer: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<LayeredPrimitive> {
    vec![
        line(layer, x0, y0, x1, y0),
        line(layer, x1, y0, x1, y1),
        line(layer, x1, y1, x0, y1),
        line(layer, x0, y1, x0, y0),
    ]
}

/// A 20 x 10 slot with radius-5 caps, lower-left arc centre at (dx, 5). The right cap is drawn
/// clockwise, the top side right to left.
pub fn slot(layer: &str, dx: f64) -> Vec<LayeredPrimitive> {
    vec![
        line(layer, dx, 0.0, dx + 20.0, 0.0),
        arc(layer, dx + 20.0, 5.0, 5.0, FRAC_PI_2, -PI),
        line(layer, dx + 20.0, 10.0, dx, 10.0),
        arc(layer, dx, 5.0, 5.0, FRAC_PI_2, PI),
    ]
}

/// A w x h rectangle with corner radius r: four sides and four quarter arcs, every line
/// coordinate rounded to 1e-6 the way a DXF writer prints them.
pub fn rounded_rect(layer: &str, w: f64, h: f64, r: f64) -> Vec<LayeredPrimitive> {
    let q = |v: f64| (v * 1e6).round() / 1e6;
    let s = 0.333_333_3; // an offset that does not round evenly
    let (x0, y0, x1, y1) = (q(s), q(s), q(s + w), q(s + h));
    vec![
        line(layer, x0 + r, y0, x1 - r, y0),
        arc(layer, x1 - r, y0 + r, r, -FRAC_PI_2, FRAC_PI_2),
        line(layer, x1, y0 + r, x1, y1 - r),
        arc(layer, x1 - r, y1 - r, r, 0.0, FRAC_PI_2),
        line(layer, x1 - r, y1, x0 + r, y1),
        arc(layer, x0 + r, y1 - r, r, FRAC_PI_2, FRAC_PI_2),
        line(layer, x0, y1 - r, x0, y0 + r),
        arc(layer, x0 + r, y0 + r, r, PI, FRAC_PI_2),
    ]
}

/// Chord polygon of a closed chain, 2-degree steps on arcs.
pub fn flatten(chain: &ClosedChain) -> Vec<Point2> {
    let mut points = Vec::new();
    for segment in &chain.segments {
        let (center, radius, from, sweep) = match segment.curve {
            Primitive::Line { start, .. } => {
                points.push(start);
                continue;
            }
            Primitive::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => (center, radius, start_angle, sweep_angle),
            Primitive::Circle { center, radius } => (center, radius, 0.0, TAU),
        };
        let steps = (sweep.abs() / (PI / 90.0)).ceil().max(1.0) as usize;
        for i in 0..steps {
            let a = from + sweep * i as f64 / steps as f64;
            points.push(p(center.x + radius * a.cos(), center.y + radius * a.sin()));
        }
    }
    points
}

/// Move a primitive by (dx, dy).
pub fn shift(item: &LayeredPrimitive, dx: f64, dy: f64) -> LayeredPrimitive {
    let m = |q: Point2| p(q.x + dx, q.y + dy);
    let primitive = match item.primitive {
        Primitive::Line { start, end } => Primitive::Line {
            start: m(start),
            end: m(end),
        },
        Primitive::Arc {
            center,
            radius,
            start_angle,
            sweep_angle,
        } => Primitive::Arc {
            center: m(center),
            radius,
            start_angle,
            sweep_angle,
        },
        Primitive::Circle { center, radius } => Primitive::Circle {
            center: m(center),
            radius,
        },
    };
    on(&item.layer, primitive)
}

/// A small deterministic generator (Lehmer / Park-Miller minimal standard, CACM 31(10), 1988).
pub struct Lehmer(pub u64);

impl Lehmer {
    pub fn next(&mut self, below: u64) -> u64 {
        self.0 = self.0 * 48_271 % 2_147_483_647;
        self.0 % below
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        self.next(1_000_000) as f64 / 1e6
    }
}

/// Outer or hole, and the depth; panics on an ambiguous nesting.
pub fn role(chain: &ClosedChain) -> (LoopRole, usize) {
    let nesting = chain.nesting.clone().unwrap();
    (nesting.role, nesting.depth)
}

/// Turn a primitive about the origin by `angle` (radians, counterclockwise).
pub fn turn(item: &LayeredPrimitive, angle: f64) -> LayeredPrimitive {
    let (sin, cos) = angle.sin_cos();
    let m = |q: Point2| p(q.x * cos - q.y * sin, q.x * sin + q.y * cos);
    let primitive = match item.primitive {
        Primitive::Line { start, end } => Primitive::Line {
            start: m(start),
            end: m(end),
        },
        Primitive::Arc {
            center,
            radius,
            start_angle,
            sweep_angle,
        } => Primitive::Arc {
            center: m(center),
            radius,
            start_angle: start_angle + angle,
            sweep_angle,
        },
        Primitive::Circle { center, radius } => Primitive::Circle {
            center: m(center),
            radius,
        },
    };
    on(&item.layer, primitive)
}
