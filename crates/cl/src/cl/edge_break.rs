//! The edge break an operation declares (#303, #1891): a legacy
//! vertical cone or an optional framed 3D cap (see [`super::EdgeBreakVolume`]).
//!
//! Provenance: the chamfer mill's cut law - a cone of half-angle `h`
//! from its axis, apex `depth` under the edge, meets the edge's level
//! plane at radius `depth * tan(h)` (machine-tool handbook geometry,
//! the same law `machining2d::chamfer` and `finishing3d::deburr` ride).
//! The surface the cone leaves over a column `s` mm (in plan) from the
//! edge is `z_edge - depth + slope * s`, `slope = cot(h)`; swept along
//! the edge it is the lower envelope of those cones. Our own model; no
//! vendor material consulted.
//!
//! The record is the operation's statement of what it was ASKED to cut:
//! the edge it picked, the depth and slope its parameters state. The
//! generator makes it beside the moves, never reading it back off them.
//! The program sim judges the moves against it: a column cut down to
//! the declared bevel is the finished part, a column cut deeper, or a
//! bevel on an edge no operation declared, is still a gouge.

use crate::geometry::Point3;

/// One declared edge break, independent of the generated moves.
/// Without `volume`, depth and slope describe the legacy vertical
/// cone. A framed record instead states its maximum inward depth and
/// full cross-edge width, with one local frame per polyline segment.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeBreak {
    pub edge: Vec<Point3>,
    pub depth: f64,
    pub slope: f64,
    /// A framed 3D cap, or the legacy vertical cone when absent.
    /// Framed records use slope zero so older readers discard them.
    pub volume: Option<super::EdgeBreakVolume>,
}

impl EdgeBreak {
    /// The framed cap's full cross-edge width, or the legacy bevel's
    /// plan width at the edge's level (`depth / slope`).
    #[must_use]
    pub fn width(&self) -> f64 {
        self.volume.as_ref().map_or_else(|| self.depth / self.slope, |v| v.width)
    }

    /// Finite positive dimensions and points, and a valid legacy slope
    /// or bounded orthonormal framed profile: every reader's safety door.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.depth.is_finite()
            && self.depth > 0.0
            && !self.edge.is_empty()
            && self.edge.iter().all(|p| p.is_finite())
            && match &self.volume {
                Some(v) => self.slope == 0.0 && v.is_sane(&self.edge, self.depth),
                None => self.slope.is_finite() && self.slope > 0.0 && self.width().is_finite() && self.width() > 0.0,
            }
    }

    /// The lowest connected declared surface below the model top.
    /// An isolated volume below a boss cannot forgive cutting the boss.
    #[must_use]
    pub fn floor_below(&self, x: f64, y: f64, model_top: f64) -> Option<f64> {
        if !self.is_sane() || !x.is_finite() || !y.is_finite() || !model_top.is_finite() {
            return None;
        }
        match &self.volume {
            Some(v) => v.floor_below(&self.edge, self.depth, Point3::new(x, y, model_top)),
            None => self.floor_at(x, y).filter(|&(_, z)| model_top <= z).map(|(floor, _)| floor),
        }
    }

    /// The declared bevel over the column at plan point `(x, y)`, with
    /// the edge height at the nearest edge point: `None` when the column
    /// lies more than `width()` from the edge in plan (the bevel has met
    /// the edge's level there, nothing below it is declared) or the
    /// record is not sane. Over the polyline the lowest bevel stands -
    /// the cone swept along the edge.
    #[must_use]
    pub fn floor_at(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        if self.volume.is_some() || !self.is_sane() {
            return None;
        }
        // (nearest plan distance, edge z there), and the lowest bevel.
        let mut near: Option<(f64, f64)> = None;
        let mut floor = f64::INFINITY;
        let mut take = |d: f64, z: f64, f: f64| {
            if near.is_none_or(|(nd, _)| d < nd) {
                near = Some((d, z));
            }
            floor = floor.min(f);
        };
        if let [p] = self.edge.as_slice() {
            let d = plan(x - p.x, y - p.y);
            take(d, p.z, p.z - self.depth + self.slope * d);
        }
        for seg in self.edge.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len = plan(dx, dy);
            if len <= 1e-12 || len.is_nan() {
                // A vertical or collapsed segment: its lowest point is
                // the cone's apex over that plan point.
                let (d, z) = (plan(x - a.x, y - a.y), a.z.min(b.z));
                take(d, a.z.max(b.z), z - self.depth + self.slope * d);
                continue;
            }
            let (ex, ey) = (dx / len, dy / len);
            let (ux, uy) = (x - a.x, y - a.y);
            let along = ux * ex + uy * ey;
            let across = (ux * ey - uy * ex).abs();
            let m = (b.z - a.z) / len;
            let un = along.clamp(0.0, len);
            let dn = plan(along - un, across);
            // The swept cone's lowest point over the column: minimise
            // z(u) + slope * dist(u) along the segment. The edge climbs
            // `m` per plan mm; with |m| < slope the minimum sits where
            // the cone's flank is tangent to that climb, otherwise at
            // the segment's low end.
            let r = m / self.slope;
            let u = if r.abs() < 1.0 {
                along - across * r / (1.0 - r * r).sqrt()
            } else if m > 0.0 {
                0.0
            } else {
                len
            };
            let u = u.clamp(0.0, len);
            let f = a.z + m * u - self.depth + self.slope * plan(along - u, across);
            take(dn, a.z + m * un, f);
        }
        let (d, z) = near?;
        (d <= self.width() && floor.is_finite()).then_some((floor, z))
    }
}

fn plan(dx: f64, dy: f64) -> f64 {
    (dx * dx + dy * dy).sqrt()
}
