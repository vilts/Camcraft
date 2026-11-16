//! Chord-end welding for the slicer: two chord ends within a micrometre
//! of each other are one point.
//!
//! Provenance: tolerance welding by spatial hashing (bucket a point by a
//! grid cell, compare against the neighbouring cells' points) is the
//! standard vertex-welding construction from the computational-geometry
//! literature on mesh repair; our own code (#241).
//!
//! Why a tolerance and not a rounded key: an imported mesh's shared
//! corners are often not bit-identical - the issue's part carried copies
//! of one corner up to 1.04 um apart - so the two facets sharing an edge
//! compute crossing points a fraction of a micrometre apart. Rounding
//! each to a 1 um key splits any pair that straddles a rounding boundary,
//! and a closed mesh then slices as "open" (dangling chord ends) and is
//! refused. A neighbourhood search welds such a pair whichever side of a
//! boundary each end landed on. The slicer tries the exact key first and
//! falls back to this only when the exact weld leaves ends dangling, so a
//! mesh that sliced before slices exactly as before.

use std::collections::HashMap;

use crate::geo2d::Point2;

/// Welding grid: 1 unit = 1 um, the geo2d bridge's own quantization.
const WELD: f64 = 1e6;
/// Two chord ends at most this far apart (Chebyshev, grid units) weld.
/// Twice the grid unit: wider than the corner scatter of real STL
/// exports, far below any feature the region layer (1 um) can represent.
const WELD_TOL: f64 = 2.0;
/// Bucket edge, grid units. At least `WELD_TOL`, so every point within
/// tolerance lies in the same or an adjacent bucket.
const CELL: f64 = 4.0;

/// The welded chord-end set of one slice.
pub(super) struct Welder {
    tolerant: bool,
    pts: Vec<Point2>,
    keys: HashMap<(i64, i64), u32>,
    buckets: HashMap<(i64, i64), Vec<u32>>,
}

impl Welder {
    /// `tolerant: false` is the exact weld (one rounded 1 um key per
    /// end, the slicer's first try); `true` is the tolerance weld below.
    pub(super) fn new(tolerant: bool) -> Self {
        Welder { tolerant, pts: Vec::new(), keys: HashMap::new(), buckets: HashMap::new() }
    }

    /// The id of `p`. Coordinates are bounded by the slicer's horizon
    /// check before they get here, so the key casts stay in range.
    pub(super) fn weld(&mut self, p: Point2) -> u32 {
        if !self.tolerant {
            let key = ((p.x * WELD).round() as i64, (p.y * WELD).round() as i64);
            if let Some(id) = self.keys.get(&key) {
                return *id;
            }
            let id = self.pts.len() as u32;
            self.pts.push(p);
            self.keys.insert(key, id);
            return id;
        }
        self.weld_near(p)
    }

    /// The nearest existing end within `WELD_TOL`, or a new one.
    fn weld_near(&mut self, p: Point2) -> u32 {
        let (gx, gy) = (p.x * WELD, p.y * WELD);
        let (cx, cy) = ((gx / CELL).floor() as i64, (gy / CELL).floor() as i64);
        let mut best: Option<(f64, u32)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(ids) = self.buckets.get(&(cx.saturating_add(dx), cy.saturating_add(dy))) else {
                    continue;
                };
                for &id in ids {
                    let Some(q) = self.pts.get(id as usize) else { continue };
                    let d = (q.x - p.x).abs().max((q.y - p.y).abs()) * WELD;
                    if d <= WELD_TOL && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, id));
                    }
                }
            }
        }
        if let Some((_, id)) = best {
            return id;
        }
        let id = self.pts.len() as u32;
        self.pts.push(p);
        self.buckets.entry((cx, cy)).or_default().push(id);
        id
    }

    /// Every distinct end, indexed by its id.
    pub(super) fn points(&self) -> &[Point2] {
        &self.pts
    }
}
