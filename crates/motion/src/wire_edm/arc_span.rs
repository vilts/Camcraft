//! One span of an analytic wire profile (#3937): a straight line or a
//! circular arc, walked from `a` to `b`. The arc keeps its centre,
//! radius and turning sense, so its parallel at the kerf offset is a
//! concentric arc, never a chord list.
//!
//! Provenance: textbook plane geometry - the parallel (offset) curve of
//! a line is a line, of a circle a concentric circle (the offset-curve
//! reading of Tiller and Hanson, "Offsets of two-dimensional profiles",
//! IEEE CG&A 1984). Output points use only `+ - * /` and `sqrt` (no
//! trigonometry), so the posted bytes agree on every platform. Our own
//! implementation.

use std::f64::consts::TAU;

use crate::geo2d::Point2;

/// How far (radians) a direction may stand outside an arc's sweep and
/// still read as on it: float noise at the arc's ends.
pub(super) const ANG_EPS: f64 = 1e-9;

pub(super) fn sub(p: Point2, q: Point2) -> Point2 {
    Point2::new(p.x - q.x, p.y - q.y)
}

pub(super) fn add(p: Point2, q: Point2) -> Point2 {
    Point2::new(p.x + q.x, p.y + q.y)
}

pub(super) fn mul(p: Point2, f: f64) -> Point2 {
    Point2::new(p.x * f, p.y * f)
}

pub(super) fn dot(p: Point2, q: Point2) -> f64 {
    p.x * q.x + p.y * q.y
}

pub(super) fn cross(p: Point2, q: Point2) -> f64 {
    p.x * q.y - p.y * q.x
}

pub(super) fn norm(p: Point2) -> f64 {
    p.x.hypot(p.y)
}

pub(super) fn dist(p: Point2, q: Point2) -> f64 {
    norm(sub(p, q))
}

/// `p` scaled to unit length; `None` for a zero or non-finite vector.
pub(super) fn unit(p: Point2) -> Option<Point2> {
    let l = norm(p);
    (l.is_finite() && l > 0.0).then(|| mul(p, 1.0 / l))
}

/// The left normal of a unit direction.
pub(super) fn left(t: Point2) -> Point2 {
    Point2::new(-t.y, t.x)
}

/// One span in walk direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Span {
    Line { a: Point2, b: Point2 },
    /// `sweep` is the turn from `a` to `b` in (0, 2pi]; 2pi is a full
    /// circle (`a == b`). `ccw` is the turning sense of the walk.
    Arc { c: Point2, r: f64, a: Point2, b: Point2, ccw: bool, sweep: f64 },
}

/// What a span's parallel at a signed offset is.
pub(super) enum Parallel {
    Span(Span),
    /// A concave arc whose radius equals the offset: its parallel is the
    /// centre point.
    Point,
    /// A concave arc tighter than the offset by this much (mm).
    Inverted(f64),
}

impl Span {
    #[must_use]
    pub fn start(&self) -> Point2 {
        match *self {
            Span::Line { a, .. } | Span::Arc { a, .. } => a,
        }
    }

    #[must_use]
    pub fn end(&self) -> Point2 {
        match *self {
            Span::Line { b, .. } | Span::Arc { b, .. } => b,
        }
    }

    pub(super) fn is_arc(&self) -> bool {
        matches!(self, Span::Arc { .. })
    }

    /// The unit tangent of the walk at `p` on the span.
    fn tangent_at(&self, p: Point2) -> Option<Point2> {
        match *self {
            Span::Line { a, b } => unit(sub(b, a)),
            Span::Arc { c, ccw, .. } => {
                let n = left(unit(sub(p, c))?);
                Some(if ccw { n } else { mul(n, -1.0) })
            }
        }
    }

    pub(super) fn dir_start(&self) -> Option<Point2> {
        self.tangent_at(self.start())
    }

    pub(super) fn dir_end(&self) -> Option<Point2> {
        self.tangent_at(self.end())
    }

    #[must_use]
    pub fn length(&self) -> f64 {
        match *self {
            Span::Line { a, b } => dist(a, b),
            Span::Arc { r, sweep, .. } => r * sweep,
        }
    }

    /// The turn (radians, in walk sense) from the arc's start to the
    /// direction of `q` from its centre, in [0, 2pi). 0 for a line.
    pub(super) fn turn_to(&self, q: Point2) -> f64 {
        let Span::Arc { c, a, ccw, .. } = *self else { return 0.0 };
        let (u, v) = (sub(a, c), sub(q, c));
        let t = cross(u, v).atan2(dot(u, v));
        let t = if ccw { t } else { -t };
        let t = t.rem_euclid(TAU);
        if t > TAU - ANG_EPS { 0.0 } else { t }
    }

    /// Whether the direction of `q` from the arc's centre lies within its
    /// sweep (always true for a full circle).
    pub(super) fn holds_dir(&self, q: Point2) -> bool {
        match *self {
            Span::Line { .. } => false,
            Span::Arc { sweep, .. } => self.turn_to(q) <= sweep + ANG_EPS,
        }
    }

    /// The point at `f` of the way along (0 = start, 1 = end). Used by the
    /// laws' sampling only; output points never come from here.
    pub(super) fn point_at(&self, f: f64) -> Point2 {
        match *self {
            Span::Line { a, b } => add(a, mul(sub(b, a), f)),
            Span::Arc { c, a, ccw, sweep, .. } => {
                let t = if ccw { sweep * f } else { -sweep * f };
                let (s, co) = t.sin_cos();
                let u = sub(a, c);
                Point2::new(c.x + u.x * co - u.y * s, c.y + u.x * s + u.y * co)
            }
        }
    }

    /// The parallel of the span at signed offset `s` (positive: to the
    /// left of the walk). A line keeps its length; an arc turning left
    /// (counter-clockwise) has the radius `r - s`, one turning right
    /// `r + s`, its ends moved along their radii.
    pub(super) fn parallel(&self, s: f64) -> Option<Parallel> {
        match *self {
            Span::Line { a, b } => {
                let n = mul(left(unit(sub(b, a))?), s);
                Some(Parallel::Span(Span::Line { a: add(a, n), b: add(b, n) }))
            }
            Span::Arc { c, r, a, b, ccw, sweep } => {
                let r2 = if ccw { r - s } else { r + s };
                if !r2.is_finite() || r.is_nan() || r <= 0.0 {
                    return None;
                }
                if r2.abs() <= 1e-9 {
                    return Some(Parallel::Point);
                }
                if r2 < 0.0 {
                    return Some(Parallel::Inverted(-r2));
                }
                let f = r2 / r;
                Some(Parallel::Span(Span::Arc {
                    c,
                    r: r2,
                    a: add(c, mul(sub(a, c), f)),
                    b: add(c, mul(sub(b, c), f)),
                    ccw,
                    sweep,
                }))
            }
        }
    }

    /// The span from `p` to `q`, both on it in walk order: a line's
    /// parameter, an arc's turn. A moved end may stand up to `ext` (mm)
    /// past the span's own end (two near-tangent parallels joined across a
    /// drawing's joint gap). `None` when `q` does not follow `p` - the
    /// trim consumed the span.
    pub(super) fn between(&self, p: Point2, q: Point2, p_moved: bool, q_moved: bool, ext: f64) -> Option<Span> {
        match *self {
            Span::Line { a, b } => {
                let d = sub(b, a);
                let l2 = dot(d, d);
                if l2.is_nan() || l2 <= 0.0 {
                    return None;
                }
                let tp = if p_moved { dot(sub(p, a), d) / l2 } else { 0.0 };
                let tq = if q_moved { dot(sub(q, a), d) / l2 } else { 1.0 };
                let e = 1e-12 + ext / l2.sqrt();
                (tp >= -e && tq <= 1.0 + e && tq - tp > 1e-12).then_some(Span::Line { a: p, b: q })
            }
            Span::Arc { c, r, ccw, sweep, .. } => {
                let e = ANG_EPS + ext / r;
                let mut tp = if p_moved { self.turn_to(p) } else { 0.0 };
                if p_moved && tp > TAU - e {
                    tp -= TAU;
                }
                let tq = if q_moved { self.turn_to(q) } else { sweep };
                let tq = if q_moved && tq <= ANG_EPS && sweep >= TAU - ANG_EPS { sweep } else { tq };
                let turn = tq - tp;
                (tp >= -e && tq <= sweep + e && turn > ANG_EPS && turn <= TAU + ANG_EPS)
                    .then_some(Span::Arc { c, r, a: p, b: q, ccw, sweep: turn })
            }
        }
    }

    /// The span cut at `p` (on it): the part before and the part after.
    /// A part of no length is `None`.
    pub(super) fn split(&self, p: Point2) -> (Option<Span>, Option<Span>) {
        let before = if dist(self.start(), p) > 1e-12 { self.between(self.start(), p, false, true, 0.0) } else { None };
        let after = if dist(p, self.end()) > 1e-12 || self.full() { self.between(p, self.end(), true, false, 0.0) } else { None };
        (before, after)
    }

    /// A full circle.
    pub(super) fn full(&self) -> bool {
        matches!(*self, Span::Arc { sweep, .. } if sweep >= TAU - ANG_EPS)
    }

    /// The arc cut into two equal halves when it turns more than half a
    /// circle: a full circle at its antipode, any other at the point
    /// opposite the bisector of its end radii (no trigonometry).
    pub(super) fn halves(&self) -> Vec<Span> {
        let Span::Arc { c, r, a, b, ccw, sweep } = *self else { return vec![*self] };
        if sweep <= std::f64::consts::PI + ANG_EPS {
            return vec![*self];
        }
        let m = if self.full() {
            sub(mul(c, 2.0), a)
        } else {
            match unit(add(sub(a, c), sub(b, c))) {
                Some(u) => sub(c, mul(u, r)),
                None => return vec![*self],
            }
        };
        let h = sweep * 0.5;
        vec![Span::Arc { c, r, a, b: m, ccw, sweep: h }, Span::Arc { c, r, a: m, b, ccw, sweep: sweep - h }]
    }
}
