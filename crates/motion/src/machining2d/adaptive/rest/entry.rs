//! Where a rest Adaptive pass may plunge (issue 2031): only where the
//! whole cutter footprint, with a margin, is proved clear of the stock
//! the earlier operations left - by the stock as it was given, never by
//! what this operation's own passes have cut since (a column they
//! cleared at this depth says nothing about the levels above it).
//!
//! Provenance: the clearance of a disc against a region is the region
//! grown by the disc's radius (Minkowski sum; M. Held, "On the
//! Computational Geometry of Pocket Machining", LNCS 500, Springer
//! 1991). The proof is asked twice, of the polygon and of the
//! conservative raster; the margin is ours. A clear tool AXIS proves
//! nothing: issue 1490 measured rest plunges whose axis stood on
//! cleared floor while the rim touched the corner's rest material.

use crate::geo2d::{Point2, Region};
use crate::machining2d::GenError;

use super::super::front::{densify, dist};
use super::super::link::inside;
use super::super::raster::Raster;

/// The proved-clear plunge positions of one cut depth.
pub(super) struct Entries<'a> {
    center: &'a Region,
    /// Centers NOT proved clear: the stock's material grown by the tool
    /// radius and half the margin.
    blocked: Region,
    /// The stock as it was given (conservative), never cut.
    stock: Raster,
    /// The footprint the raster proof asks clear.
    footprint: f64,
    /// Vertices of the proved-clear center area's boundary (a full
    /// margin off the material).
    spots: Vec<Point2>,
    /// The entry guard (`stock::Guards::entry`).
    on: bool,
}

impl<'a> Entries<'a> {
    /// `material` is the bound on the stock's material at this depth,
    /// `grow` the conservative growth the raster was built with.
    pub(super) fn new(
        center: &'a Region,
        material: &Region,
        stock: Raster,
        (r, grow, margin, seg): (f64, f64, f64, f64),
        on: bool,
    ) -> Result<Self, GenError> {
        let blocked = material.offset(grow + r + 0.5 * margin)?;
        let free = center.difference(&material.offset(grow + r + margin)?)?;
        let spots = free.loops().iter().flat_map(|l| densify(l.points(), seg.max(1e-6), true)).collect();
        Ok(Entries { center, blocked, stock, footprint: r + 0.25 * margin, spots, on })
    }

    /// Is a plunge at `p` proved clear: a legal center, outside the
    /// grown material, with no material cell under the footprint?
    pub(super) fn proved(&self, p: Point2) -> bool {
        if !self.on {
            return true;
        }
        inside(self.center, p) && !self.blocked.contains(p) && self.stock.air(p, p, self.footprint)
    }

    /// Is any position proved at all?
    pub(super) fn any(&self) -> bool {
        !self.on || self.spots.iter().any(|&q| self.proved(q))
    }

    /// The `count` proved positions nearest any of `to`, nearest first.
    pub(super) fn near(&self, to: &[Point2], count: usize) -> Vec<Point2> {
        if !self.on {
            // (With the guard off every position counts as proved, so
            // the nearest is the asked one itself.)
            return to.iter().take(count).copied().collect();
        }
        let d = |q: Point2| to.iter().map(|&t| dist(q, t)).fold(f64::INFINITY, f64::min);
        let mut all: Vec<(f64, Point2)> = self.spots.iter().map(|&q| (d(q), q)).collect();
        all.sort_by(|a, b| a.0.total_cmp(&b.0));
        all.dedup_by(|a, b| dist(a.1, b.1) < 1e-9);
        all.into_iter().map(|(_, q)| q).filter(|&q| self.proved(q)).take(count).collect()
    }
}
