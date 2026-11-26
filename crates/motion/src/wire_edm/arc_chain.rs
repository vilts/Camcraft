//! The analytic wire profile (#3937): one closed chain of lines, arcs and
//! circles as the drawing states it, walked counter-clockwise, its joint
//! gaps measured and held under a stated tolerance.
//!
//! Provenance: plane geometry (Green's theorem for the enclosed area,
//! inscribed and circumscribed chords of a circle for the waste-side
//! ring). Our own implementation.

use std::f64::consts::TAU;

use camcraft_geometry::dxf::Primitive;
use camcraft_geometry::sketch_chains::ClosedChain;

use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::GenError;

use super::arc_span::{cross, dist, sub, Span};
use super::WireSide;

/// The widest joint gap (mm) the wire path accepts between two spans of
/// the drawing: 0.25 um. Arc ends computed from a centre and an angle
/// seldom meet a line's end bit for bit; a gap under this is charged to
/// the path's waste-side margin (`arc_cut`), a wider one is refused.
pub const JOINT_TOL: f64 = 2.5e-4;

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire contour: {why}"))
}

/// A closed line / arc chain, counter-clockwise.
#[derive(Clone, Debug, PartialEq)]
pub struct ArcChain {
    spans: Vec<Span>,
    max_gap: f64,
}

impl ArcChain {
    /// The chain from its spans in walk order. Refused: an empty chain, a
    /// non-finite or zero-length span, a joint gap past [`JOINT_TOL`], or
    /// no enclosed area. A clockwise chain is reversed.
    ///
    /// # Errors
    /// Each refusal by name.
    pub fn new(spans: Vec<Span>) -> Result<ArcChain, GenError> {
        if spans.is_empty() {
            return Err(GenError::Empty("wire contour: no profile to cut".into()));
        }
        for (i, s) in spans.iter().enumerate() {
            let ok = match *s {
                Span::Line { a, b } => fin(a) && fin(b) && dist(a, b) > 0.0,
                Span::Arc { c, r, a, b, sweep, .. } => {
                    fin(c) && fin(a) && fin(b) && r.is_finite() && r > 0.0 && sweep > 0.0 && sweep <= TAU + 1e-9
                }
            };
            if !ok {
                return Err(invalid(format!("span {} of the profile is degenerate (no length, or not a finite curve)", i + 1)));
            }
        }
        let n = spans.len();
        let mut max_gap: f64 = 0.0;
        for i in 0..n {
            let (e, s) = (spans[i].end(), spans[(i + 1) % n].start());
            let g = dist(e, s);
            if g.is_nan() || g > JOINT_TOL {
                return Err(GenError::invalid_at(
                    format!(
                        "wire contour: the profile has a joint gap of {g:.6} mm near X{:.3} Y{:.3} - the wire path accepts at most {JOINT_TOL} mm; close the drawing's joint",
                        e.x, e.y
                    ),
                    crate::geometry::Point3::new(e.x, e.y, 0.0),
                ));
            }
            max_gap = max_gap.max(g);
        }
        let mut chain = ArcChain { spans, max_gap };
        let area = chain.area();
        if !(area.is_finite() && area.abs() > 1e-6) {
            return Err(invalid("the profile has no area (a zero-length or collinear ring) - there is nothing to cut around".into()));
        }
        if area < 0.0 {
            chain.spans = chain.spans.iter().rev().map(reversed).collect();
        }
        Ok(chain)
    }

    /// The selected DXF chain, each point taken through `place` (a rigid
    /// placement: a turn about Z and a shift, never a mirror).
    ///
    /// # Errors
    /// As [`ArcChain::new`].
    pub fn from_closed(chain: &ClosedChain, place: impl Fn(Point2) -> Point2) -> Result<ArcChain, GenError> {
        let spans = chain
            .segments
            .iter()
            .map(|seg| match seg.curve {
                Primitive::Line { .. } => Span::Line { a: place(seg.start), b: place(seg.end) },
                Primitive::Arc { center, radius, sweep_angle, .. } => Span::Arc {
                    c: place(center),
                    r: radius,
                    a: place(seg.start),
                    b: place(seg.end),
                    ccw: sweep_angle > 0.0,
                    sweep: sweep_angle.abs(),
                },
                Primitive::Circle { center, radius } => {
                    let a = place(seg.start);
                    Span::Arc { c: place(center), r: radius, a, b: a, ccw: true, sweep: TAU }
                }
            })
            .collect();
        ArcChain::new(spans)
    }

    #[must_use]
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// The widest joint gap of the chain (mm), at most [`JOINT_TOL`].
    #[must_use]
    pub fn max_gap(&self) -> f64 {
        self.max_gap
    }

    /// Whether every span is a line.
    #[must_use]
    pub fn straight(&self) -> bool {
        self.spans.iter().all(|s| !s.is_arc())
    }

    /// The enclosed area (Green's theorem), counter-clockwise positive.
    #[must_use]
    pub fn area(&self) -> f64 {
        self.spans
            .iter()
            .map(|s| match *s {
                Span::Line { a, b } => 0.5 * cross(a, b),
                Span::Arc { c, r, a, b, ccw, sweep } => {
                    let signed = if ccw { sweep } else { -sweep };
                    0.5 * (cross(c, sub(b, a)) + r * r * signed)
                }
            })
            .sum()
    }

    #[must_use]
    pub fn perimeter(&self) -> f64 {
        self.spans.iter().map(Span::length).sum()
    }

    /// The chain as a polygon whose every edge lies on the WASTE side of
    /// the true curve, within `sag` (mm) of it: chords inscribed in an arc
    /// whose waste is toward its centre, circumscribed about one whose
    /// waste is away from it. A region read from it holds the whole
    /// product, so a check against it errs toward the product only by
    /// stating more of it.
    ///
    /// # Errors
    /// A `sag` that is not positive, or a ring that needs too many points.
    pub fn waste_ring(&self, side: WireSide, sag: f64) -> Result<Loop, GenError> {
        if !(sag.is_finite() && sag > 0.0) {
            return Err(invalid(format!("the waste-side ring needs a positive sag, not {sag}")));
        }
        // Counter-clockwise: the inside (a die's waste) is to the left.
        let waste_left = match side {
            WireSide::Die => true,
            WireSide::Punch => false,
            WireSide::Left | WireSide::Right => return Err(invalid("a closed profile states its die or punch side".into())),
        };
        let mut pts = Vec::new();
        for s in &self.spans {
            pts.push(s.start());
            let Span::Arc { c, r, a, ccw, sweep, .. } = *s else { continue };
            // The arc's centre is to the left of its walk when it turns left.
            let inscribed = ccw == waste_left;
            let step = if inscribed { 2.0 * (1.0 - sag / r).clamp(-1.0, 1.0).acos() } else { 2.0 * (r / (r + sag)).acos() };
            let n = (sweep / step).ceil();
            if !(n.is_finite() && n <= 65_536.0) {
                return Err(invalid(format!("an arc of radius {r:.4} mm needs more than 65536 points at {sag} mm - refused")));
            }
            let n = (n as usize).max(1);
            let d = sweep / n as f64 * if ccw { 1.0 } else { -1.0 };
            let a0 = (a.y - c.y).atan2(a.x - c.x);
            if inscribed {
                pts.extend((1..n).map(|j| on(c, r, a0 + d * j as f64)));
            } else {
                let rr = r / (d * 0.5).cos();
                pts.extend((1..=n).map(|j| on(c, rr, a0 + d * (j as f64 - 0.5))));
                pts.push(s.end());
            }
        }
        Loop::new(pts).map_err(GenError::from)
    }

    /// Whether `q` stands inside the chain (read on a ring within 1 um).
    pub(super) fn contains(&self, q: Point2) -> Result<bool, GenError> {
        let ring = self.waste_ring(WireSide::Die, 1e-3)?;
        Ok(Region::new(vec![ring])?.contains(q))
    }
}

fn fin(p: Point2) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn on(c: Point2, r: f64, t: f64) -> Point2 {
    let (s, co) = t.sin_cos();
    Point2::new(c.x + r * co, c.y + r * s)
}

fn reversed(s: &Span) -> Span {
    match *s {
        Span::Line { a, b } => Span::Line { a: b, b: a },
        Span::Arc { c, r, a, b, ccw, sweep } => Span::Arc { c, r, a: b, b: a, ccw: !ccw, sweep },
    }
}
