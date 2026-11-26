//! An exact XY bucket index over law (c)'s boundary samples (#3588) - the uniform-grid broad phase of computational geometry textbooks (de Berg et al., "Computational Geometry", range searching), our own code.
//!
//! Law (c) asks, for each band disc, which samples stand within its
//! radius. The old loop asked every sample of every plane: a ramp
//! whose moves each end at their own Z multiplied the planes by the
//! moves. This index answers the same question with a SUPERSET of the
//! samples the disc can hold, never a subset:
//!
//! - samples are kept once per exact bit pattern (the law's three
//!   readings - in stock, surface, part - are pure in `q`, and the
//!   verdict is a max over samples, so a repeat changes nothing);
//! - a finite query reads the cells its disc's box overlaps, the box
//!   widened by a relative 1e-9 against rounding;
//! - a sample or a query the grid cannot place (non-finite, or too far
//!   out for an `i64` cell) is always asked, exactly as the old loop
//!   asked it (a NaN distance passed the old `> r2` skip).

use std::collections::HashMap;

use crate::geo2d::Point2;

/// Cells further out than this many cell widths are not placed: the
/// float-to-int conversion stays far from saturation.
const CELL_LIMIT: f64 = 1.0e12;
/// A query box wider than this many cells scans everything instead.
const SPAN_LIMIT: i64 = 64;

pub(super) struct SampleGrid {
    /// The distinct samples, in first-seen order.
    samples: Vec<Point2>,
    cell: f64,
    buckets: HashMap<(i64, i64), Vec<usize>>,
    /// Samples the grid cannot place: asked by every query.
    always: Vec<usize>,
}

impl SampleGrid {
    /// Index `raw` with square cells of width `cell` (the widest disc
    /// radius the caller asks; a bad width indexes nothing and every
    /// query scans all samples - slow, never wrong).
    pub(super) fn new(raw: &[Point2], cell: f64) -> Self {
        let mut seen: HashMap<(u64, u64), ()> = HashMap::with_capacity(raw.len());
        let mut samples = Vec::new();
        for q in raw {
            if seen.insert((q.x.to_bits(), q.y.to_bits()), ()).is_none() {
                samples.push(*q);
            }
        }
        let cell = if cell.is_finite() && cell > 0.0 { cell } else { f64::NAN };
        let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        let mut always = Vec::new();
        for (k, q) in samples.iter().enumerate() {
            match (index(q.x, cell), index(q.y, cell)) {
                (Some(ix), Some(iy)) => buckets.entry((ix, iy)).or_default().push(k),
                _ => always.push(k),
            }
        }
        Self { samples, cell, buckets, always }
    }

    /// The distinct samples (the sweep (c3) reads them all).
    pub(super) fn samples(&self) -> &[Point2] {
        &self.samples
    }

    /// Every sample within `rad` of `c`, and possibly some further out
    /// (the caller's own distance test stays the law). Pushed into `out`
    /// (cleared first) as sample references.
    pub(super) fn near<'a>(&'a self, c: Point2, rad: f64, out: &mut Vec<&'a Point2>) {
        out.clear();
        // A sample the caller's `dx*dx + dy*dy <= rad*rad` keeps has
        // |dx|, |dy| <= rad up to a few ulps; the margin is far wider
        // than that rounding, and the float division and floor are
        // monotone, so no such sample falls outside the cells read.
        let m = rad + 1e-9 * (rad + c.x.abs() + c.y.abs() + 1.0);
        let lo = (index(c.x - m, self.cell), index(c.y - m, self.cell));
        let hi = (index(c.x + m, self.cell), index(c.y + m, self.cell));
        let (Some(x0), Some(y0), Some(x1), Some(y1)) = (lo.0, lo.1, hi.0, hi.1) else {
            out.extend(self.samples.iter());
            return;
        };
        if x1.saturating_sub(x0) > SPAN_LIMIT || y1.saturating_sub(y0) > SPAN_LIMIT {
            out.extend(self.samples.iter());
            return;
        }
        for ix in x0..=x1 {
            for iy in y0..=y1 {
                if let Some(b) = self.buckets.get(&(ix, iy)) {
                    out.extend(b.iter().filter_map(|&k| self.samples.get(k)));
                }
            }
        }
        out.extend(self.always.iter().filter_map(|&k| self.samples.get(k)));
    }
}

/// The cell of coordinate `v`, or `None` where the grid cannot place it.
fn index(v: f64, cell: f64) -> Option<i64> {
    let t = (v / cell).floor();
    // A NaN `t` fails the comparison and is not placed.
    if t.abs() <= CELL_LIMIT { Some(t as i64) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn repeats_are_kept_once_and_every_near_sample_is_found() {
        let raw = vec![p(0.0, 0.0), p(0.0, 0.0), p(5.0, 5.0), p(9.99, 0.0), p(100.0, 100.0)];
        let g = SampleGrid::new(&raw, 10.0);
        assert_eq!(g.samples().len(), 4);
        let mut out = Vec::new();
        g.near(p(0.0, 0.0), 10.0, &mut out);
        for q in [p(0.0, 0.0), p(5.0, 5.0), p(9.99, 0.0)] {
            assert!(out.iter().any(|s| **s == q), "{q:?} must be a candidate");
        }
        assert!(!out.iter().any(|s| **s == p(100.0, 100.0)), "a far cell is not read");
    }

    #[test]
    fn unplaceable_samples_and_queries_fall_back_to_all() {
        let raw = vec![p(f64::NAN, 0.0), p(1.0, 1.0), p(1.0e300, 0.0)];
        let g = SampleGrid::new(&raw, 1.0);
        let mut out = Vec::new();
        g.near(p(500.0, 500.0), 1.0, &mut out);
        assert_eq!(out.len(), 2, "the NaN and the far-out sample are always asked");
        g.near(p(f64::NAN, 0.0), 1.0, &mut out);
        assert_eq!(out.len(), 3, "a NaN query scans everything");
        g.near(p(0.0, 0.0), 1.0e6, &mut out);
        assert_eq!(out.len(), 3, "a wide query scans everything");
        let bad = SampleGrid::new(&raw, 0.0);
        bad.near(p(1.0, 1.0), 1.0, &mut out);
        assert_eq!(out.len(), 3, "a bad cell width scans everything");
    }
}
