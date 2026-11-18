//! Provenance: oriented line/arc/circle pieces of a sketch chain. Arc area by Green's theorem
//! (any calculus text, e.g. Stewart, "Calculus", ch. 16.4); arc extents from the quadrant points.
//! Original implementation; no vendor material consulted.

use super::ChainError;
use crate::dxf::Primitive;
use crate::geo2d::Point2;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Sketch coordinates stay below this magnitude (mm, ten kilometres) so that the quotient
/// `coordinate / tolerance`, the identity quantization and the contact slack keep their meaning
/// in `f64`.
pub(super) const SKETCH_LIMIT: f64 = 1e7;

/// Identity quantum: 1e-6 mm per unit.
const ID_SCALE: f64 = 1e6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Shape {
    Line,
    /// As drawn: the arc runs from `start` by the signed `sweep`, whatever the traversal.
    Arc {
        center: Point2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
    Circle {
        center: Point2,
        radius: f64,
    },
}

/// One curve piece in traversal direction: it is walked from `a` to `b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Piece {
    pub shape: Shape,
    pub a: Point2,
    pub b: Point2,
    /// The traversal runs against the drawn direction.
    pub reversed: bool,
}

pub(super) fn sub(p: Point2, q: Point2) -> Point2 {
    Point2::new(p.x - q.x, p.y - q.y)
}

pub(super) fn cross(p: Point2, q: Point2) -> f64 {
    p.x * q.y - p.y * q.x
}

pub(super) fn dot(p: Point2, q: Point2) -> f64 {
    p.x * q.x + p.y * q.y
}

pub(super) fn dist(p: Point2, q: Point2) -> f64 {
    (p.x - q.x).hypot(p.y - q.y)
}

pub(super) fn same(p: Point2, q: Point2) -> bool {
    p.x == q.x && p.y == q.y
}

fn on_circle(center: Point2, radius: f64, angle: f64) -> Point2 {
    Point2::new(
        center.x + radius * angle.cos(),
        center.y + radius * angle.sin(),
    )
}

fn in_range(index: usize, values: &[f64]) -> Result<(), ChainError> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(ChainError::NonFinite { primitive: index });
    }
    if values.iter().any(|v| v.abs() >= SKETCH_LIMIT) {
        return Err(ChainError::OutOfRange { primitive: index });
    }
    Ok(())
}

fn invalid(primitive: usize, reason: &'static str) -> ChainError {
    ChainError::InvalidPrimitive { primitive, reason }
}

impl Piece {
    pub fn line(a: Point2, b: Point2) -> Piece {
        Piece {
            shape: Shape::Line,
            a,
            b,
            reversed: false,
        }
    }

    /// Validate one input primitive and return it in its drawn direction.
    pub fn from_primitive(index: usize, primitive: &Primitive) -> Result<Piece, ChainError> {
        match *primitive {
            Primitive::Line { start, end } => {
                in_range(index, &[start.x, start.y, end.x, end.y])?;
                Ok(Piece::line(start, end))
            }
            Primitive::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                in_range(
                    index,
                    &[center.x, center.y, radius, start_angle, sweep_angle],
                )?;
                in_range(index, &[center.x.abs() + radius, center.y.abs() + radius])?;
                if radius <= 0.0 {
                    return Err(invalid(index, "arc radius must be positive"));
                }
                if sweep_angle == 0.0 || sweep_angle.abs() > TAU + 1e-12 {
                    return Err(invalid(
                        index,
                        "arc sweep must be non-zero and at most a turn",
                    ));
                }
                let shape = Shape::Arc {
                    center,
                    radius,
                    start: start_angle,
                    sweep: sweep_angle,
                };
                let a = on_circle(center, radius, start_angle);
                let b = on_circle(center, radius, start_angle + sweep_angle);
                Ok(Piece {
                    shape,
                    a,
                    b,
                    reversed: false,
                })
            }
            Primitive::Circle { center, radius } => {
                in_range(index, &[center.x, center.y, radius])?;
                in_range(index, &[center.x.abs() + radius, center.y.abs() + radius])?;
                if radius <= 0.0 {
                    return Err(invalid(index, "circle radius must be positive"));
                }
                let a = Point2::new(center.x + radius, center.y);
                Ok(Piece {
                    shape: Shape::Circle { center, radius },
                    a,
                    b: a,
                    reversed: false,
                })
            }
        }
    }

    /// The same curve walked the other way. A circle has no ends and stays as it is.
    pub fn flipped(self) -> Piece {
        match self.shape {
            Shape::Circle { .. } => self,
            _ => Piece {
                shape: self.shape,
                a: self.b,
                b: self.a,
                reversed: !self.reversed,
            },
        }
    }

    /// The signed sweep in traversal direction (zero for a line).
    pub fn travel_sweep(&self) -> f64 {
        match self.shape {
            Shape::Line => 0.0,
            Shape::Arc { sweep, .. } => {
                if self.reversed {
                    -sweep
                } else {
                    sweep
                }
            }
            Shape::Circle { .. } => TAU,
        }
    }

    /// The piece as an analytic primitive in traversal direction.
    pub fn oriented(&self) -> Primitive {
        match self.shape {
            Shape::Line => Primitive::Line {
                start: self.a,
                end: self.b,
            },
            Shape::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let (start_angle, sweep_angle) = if self.reversed {
                    (start + sweep, -sweep)
                } else {
                    (start, sweep)
                };
                Primitive::Arc {
                    center,
                    radius,
                    start_angle,
                    sweep_angle,
                }
            }
            Shape::Circle { center, radius } => Primitive::Circle { center, radius },
        }
    }

    /// Green's theorem term `1/2 * integral(x dy - y dx)` along the piece; the terms of a
    /// closed walk sum to its signed area (counterclockwise positive).
    pub fn area_term(&self) -> f64 {
        match self.shape {
            Shape::Line => 0.5 * cross(self.a, self.b),
            Shape::Arc { center, radius, .. } => {
                0.5 * (radius * radius * self.travel_sweep() + center.x * (self.b.y - self.a.y)
                    - center.y * (self.b.x - self.a.x))
            }
            Shape::Circle { radius, .. } => PI * radius * radius,
        }
    }

    /// Axis-aligned bounds: the ends, plus every quadrant point an arc passes.
    pub fn bounds(&self) -> (Point2, Point2) {
        let mut lo = Point2::new(self.a.x.min(self.b.x), self.a.y.min(self.b.y));
        let mut hi = Point2::new(self.a.x.max(self.b.x), self.a.y.max(self.b.y));
        match self.shape {
            Shape::Line => {}
            Shape::Circle { center, radius } => {
                lo = Point2::new(center.x - radius, center.y - radius);
                hi = Point2::new(center.x + radius, center.y + radius);
            }
            Shape::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let (from, to) = if sweep < 0.0 {
                    (start + sweep, start)
                } else {
                    (start, start + sweep)
                };
                let mut k = (from / FRAC_PI_2).ceil();
                // At most five quadrant points lie in one turn.
                for _ in 0..5 {
                    let angle = k * FRAC_PI_2;
                    if angle > to {
                        break;
                    }
                    let (dx, dy) = match (k as i64).rem_euclid(4) {
                        0 => (radius, 0.0),
                        1 => (0.0, radius),
                        2 => (-radius, 0.0),
                        _ => (0.0, -radius),
                    };
                    lo = Point2::new(lo.x.min(center.x + dx), lo.y.min(center.y + dy));
                    hi = Point2::new(hi.x.max(center.x + dx), hi.y.max(center.y + dy));
                    k += 1.0;
                }
            }
        }
        (lo, hi)
    }

    /// Whether `p`, a point of the support circle, lies in the arc's angular span.
    pub fn spans(&self, p: Point2, eps: f64) -> bool {
        match self.shape {
            Shape::Line | Shape::Circle { .. } => true,
            Shape::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let angle = (p.y - center.y).atan2(p.x - center.x);
                let along = if sweep < 0.0 {
                    start - angle
                } else {
                    angle - start
                }
                .rem_euclid(TAU);
                let slack = (eps / radius).min(PI);
                along <= sweep.abs() + slack || along >= TAU - slack
            }
        }
    }
}

pub(super) type Key = (i64, i64);

/// Quantize a validated coordinate; `SKETCH_LIMIT * ID_SCALE` is far inside `i64`.
pub(super) fn quantum(v: f64) -> i64 {
    (v * ID_SCALE).round() as i64
}

pub(super) fn key(p: Point2) -> Key {
    (quantum(p.x), quantum(p.y))
}
