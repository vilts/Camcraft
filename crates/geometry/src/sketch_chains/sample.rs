//! Sampling, lookup, extents and closest-point queries for sketch chains.
//!
//! Provenance: circle sagitta bound `s = r (1 - cos(theta / 2))` and exact closest-point
//! projections on segments/arcs/circles from elementary planar geometry; original code, no
//! vendor material consulted.

use std::f64::consts::{PI, TAU};

use crate::dxf::Primitive;
use crate::geo2d::Point2;

use super::curve::{Piece, dist};
use super::types::{ChainError, ChainId, ChainSegment, ClosedChain, Nesting, OpenChain, SketchChains};

/// Borrowed view of either a closed loop or an open chain in [`SketchChains`].
#[derive(Clone, Copy, Debug)]
pub enum SketchChainRef<'a> {
    Closed(&'a ClosedChain),
    Open(&'a OpenChain),
}

impl<'a> SketchChainRef<'a> {
    #[must_use]
    pub fn id(self) -> ChainId {
        match self {
            SketchChainRef::Closed(c) => c.id,
            SketchChainRef::Open(c) => c.id,
        }
    }

    #[must_use]
    pub fn layer(self) -> &'a str {
        match self {
            SketchChainRef::Closed(c) => &c.layer,
            SketchChainRef::Open(c) => &c.layer,
        }
    }

    #[must_use]
    pub fn is_closed(self) -> bool {
        matches!(self, SketchChainRef::Closed(_))
    }

    #[must_use]
    pub fn segments(self) -> &'a [ChainSegment] {
        match self {
            SketchChainRef::Closed(c) => &c.segments,
            SketchChainRef::Open(c) => &c.segments,
        }
    }

    #[must_use]
    pub fn nesting(self) -> Option<&'a Result<Nesting, ChainError>> {
        match self {
            SketchChainRef::Closed(c) => Some(&c.nesting),
            SketchChainRef::Open(_) => None,
        }
    }

    #[must_use]
    pub fn length(self) -> f64 {
        chain_length(self.segments())
    }

    #[must_use]
    pub fn bounds(self) -> Option<(Point2, Point2)> {
        chain_bounds(self.segments())
    }

    /// Sample this chain into 2D vertices within chord tolerance `tol` (mm).
    /// Closed loops do not repeat their starting vertex.
    #[must_use]
    pub fn sample(self, tol: f64) -> Vec<Point2> {
        sample_segments(self.segments(), self.is_closed(), tol)
    }
}

impl SketchChains {
    /// Look up any cuttable (closed or open) chain by its stable [`ChainId`].
    #[must_use]
    pub fn find(&self, id: ChainId) -> Option<SketchChainRef<'_>> {
        self.closed
            .iter()
            .find(|c| c.id == id)
            .map(SketchChainRef::Closed)
            .or_else(|| self.open.iter().find(|c| c.id == id).map(SketchChainRef::Open))
    }

    /// Every cuttable chain in canonical order: closed chains first, then open chains.
    pub fn all(&self) -> impl Iterator<Item = SketchChainRef<'_>> {
        self.closed
            .iter()
            .map(SketchChainRef::Closed)
            .chain(self.open.iter().map(SketchChainRef::Open))
    }

    /// Every cuttable chain belonging to `layer`, closed first then open.
    #[must_use]
    pub fn by_layer(&self, layer: &str) -> Vec<SketchChainRef<'_>> {
        self.all().filter(|c| c.layer() == layer).collect()
    }

    /// 1-based ordinal of `id` in `self.all()`, for stable human-readable labels.
    #[must_use]
    pub fn index_of(&self, id: ChainId) -> Option<usize> {
        self.all().position(|c| c.id() == id).map(|i| i + 1)
    }
}

/// Analytic arc length of one [`Primitive`] in millimetres.
#[must_use]
pub fn primitive_length(p: &Primitive) -> f64 {
    match *p {
        Primitive::Line { start, end } => dist(start, end),
        Primitive::Arc { radius, sweep_angle, .. } => radius * sweep_angle.abs(),
        Primitive::Circle { radius, .. } => TAU * radius,
    }
}

/// Analytic length of a chain's segments in millimetres.
#[must_use]
pub fn chain_length(segments: &[ChainSegment]) -> f64 {
    segments.iter().map(|s| primitive_length(&s.curve)).sum()
}

/// Axis-aligned 2D bounds of `segments`, accounting for arc and circle quadrants.
#[must_use]
pub fn chain_bounds(segments: &[ChainSegment]) -> Option<(Point2, Point2)> {
    let mut range: Option<(Point2, Point2)> = None;
    for seg in segments {
        let (s_lo, s_hi) = match Piece::from_primitive(0, &seg.curve) {
            Ok(piece) => piece.bounds(),
            Err(_) => (seg.start, seg.end),
        };
        range = Some(match range {
            None => (
                Point2::new(s_lo.x.min(s_hi.x), s_lo.y.min(s_hi.y)),
                Point2::new(s_lo.x.max(s_hi.x), s_lo.y.max(s_hi.y)),
            ),
            Some((lo, hi)) => (
                Point2::new(lo.x.min(s_lo.x).min(s_hi.x), lo.y.min(s_lo.y).min(s_hi.y)),
                Point2::new(hi.x.max(s_lo.x).max(s_hi.x), hi.y.max(s_lo.y).max(s_hi.y)),
            ),
        });
    }
    range
}

/// Maximum angle step so circular chords stay within sagitta `sag` (mm).
fn arc_steps(radius: f64, sweep_abs: f64, sag: f64, min_steps: usize) -> usize {
    if !(radius.is_finite() && radius > 0.0 && sweep_abs.is_finite() && sweep_abs > 0.0) {
        return min_steps;
    }
    let ratio = (sag / radius).min(0.5);
    let step = if ratio > 1e-12 {
        2.0 * (1.0 - ratio).acos()
    } else {
        PI / 4.0
    };
    let raw = (sweep_abs / step.max(1e-4)).ceil();
    (raw as usize).clamp(min_steps, 1024)
}

/// Sample `segments` into ordered 2D points.
///
/// Curved segments use sagitta `<= (tol * 0.05).clamp(1e-4, 5e-4)` so offset regions and
/// `fit_arcs_sided` recover exact `MoveKind::Arc` (`G2`/`G3`) spans within contour tolerance.
#[must_use]
pub fn sample_segments(segments: &[ChainSegment], closed: bool, tol: f64) -> Vec<Point2> {
    let base_tol = if tol.is_finite() && tol > 0.0 { tol } else { 0.01 };
    let sag = (base_tol * 0.05).clamp(1e-4, 5e-4);
    let mut pts: Vec<Point2> = Vec::new();
    let push_pt = |out: &mut Vec<Point2>, p: Point2| {
        if out.last().is_none_or(|q| dist(*q, p) > 1e-9) {
            out.push(p);
        }
    };
    for seg in segments {
        match seg.curve {
            Primitive::Line { start, end } => {
                push_pt(&mut pts, start);
                push_pt(&mut pts, end);
            }
            Primitive::Arc { center, radius, start_angle, sweep_angle } => {
                push_pt(&mut pts, seg.start);
                let steps = arc_steps(radius, sweep_angle.abs(), sag, 4);
                for k in 1..steps {
                    let a = start_angle + sweep_angle * (k as f64 / steps as f64);
                    push_pt(&mut pts, Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin()));
                }
                push_pt(&mut pts, seg.end);
            }
            Primitive::Circle { center, radius } => {
                let steps = arc_steps(radius, TAU, sag, 24);
                for k in 0..steps {
                    let a = TAU * (k as f64 / steps as f64);
                    push_pt(&mut pts, Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin()));
                }
            }
        }
    }
    if closed && pts.len() >= 2 && dist(pts[0], pts[pts.len() - 1]) <= 1e-7 {
        pts.pop();
    }
    pts
}

/// Closest point on `curve` in 2D to query point `p`.
#[must_use]
pub fn nearest_point_on_primitive(p: Point2, curve: &Primitive) -> Point2 {
    match *curve {
        Primitive::Line { start, end } => {
            let (dx, dy) = (end.x - start.x, end.y - start.y);
            let l2 = dx * dx + dy * dy;
            if l2 <= 1e-24 {
                start
            } else {
                let t = (((p.x - start.x) * dx + (p.y - start.y) * dy) / l2).clamp(0.0, 1.0);
                Point2::new(start.x + dx * t, start.y + dy * t)
            }
        }
        Primitive::Circle { center, radius } => {
            let (dx, dy) = (p.x - center.x, p.y - center.y);
            let m = dx.hypot(dy);
            if m <= 1e-12 {
                Point2::new(center.x + radius, center.y)
            } else {
                Point2::new(center.x + radius * dx / m, center.y + radius * dy / m)
            }
        }
        Primitive::Arc { center, radius, start_angle, sweep_angle } => {
            let angle = (p.y - center.y).atan2(p.x - center.x);
            let along = if sweep_angle < 0.0 {
                (start_angle - angle).rem_euclid(TAU)
            } else {
                (angle - start_angle).rem_euclid(TAU)
            };
            let span = sweep_angle.abs();
            if along <= span {
                Point2::new(center.x + radius * angle.cos(), center.y + radius * angle.sin())
            } else {
                let a0 = Point2::new(
                    center.x + radius * start_angle.cos(),
                    center.y + radius * start_angle.sin(),
                );
                let end_angle = start_angle + sweep_angle;
                let a1 = Point2::new(
                    center.x + radius * end_angle.cos(),
                    center.y + radius * end_angle.sin(),
                );
                if dist(p, a0) <= dist(p, a1) { a0 } else { a1 }
            }
        }
    }
}

/// Shortest 2D Euclidean distance from `p` to `curve`.
#[must_use]
pub fn dist_to_primitive(p: Point2, curve: &Primitive) -> f64 {
    dist(p, nearest_point_on_primitive(p, curve))
}

/// Parse a 32-digit lowercase hex `ChainId` string (the project file and MCP format).
#[must_use]
pub fn parse_chain_id_hex(s: &str) -> Option<ChainId> {
    if s.len() != 32 || !s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    u128::from_str_radix(s, 16).ok().map(ChainId)
}
