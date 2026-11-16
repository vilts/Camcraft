//! Hidden stock over a RANGE of heights (#1316): which stretches of one vertical are air under part material, read from the facets the vertical crosses (the crossing-count form of the point-in-solid test, computational-geometry textbook material, on the winding `slice::shadow_above` reads); our own design, no vendor source.
//!
//! [`super::hidden`] answers one plane per ask: `hidden_at(h)` is the
//! shadow of what hangs over `h`, inside the stock footprint, less the
//! section at `h`. That law is NOT monotone in `h`: the shadow only
//! shrinks as the plane rises, but the section comes and goes, so a
//! column that is hidden stock at one plane (a cavity, a groove, the
//! gap of a T-slot) is part material at a lower one (the floor under
//! it) and drops out there. A band whose bottom sweeps a range of
//! heights over a column therefore cannot be judged at one plane of
//! the range, the lowest included (the #1324 review's counter-probe: a
//! plunge while turning, read at its deep end, passed a cavity the
//! band went through on the way down).
//!
//! So the swept backstop asks the whole range, and this module answers
//! it EXACTLY in Z for the column it is asked at. Walk the vertical
//! through `q` down from above the part, counting the solid's depth:
//! +1 at a facet facing up (entering material from above), -1 at one
//! facing down. A stretch at depth 0 with a crossing above it is air
//! (or uncut stock) that part material hangs over - precisely the
//! `(q, h)` the plane law calls hidden: `shadow_above(h)` holds `q`
//! (a down-facing facet stands above) and the section at `h` does not.
//! Counting depth rather than parity reads touching or overlapping
//! bodies (M13.11) as their union. The rest of the plane law is a
//! bound on `h` and is applied as one: under the stock top, and below
//! the lowest earlier cut that cleared the column less `z_tol`
//! (`Rest::uncut`'s walk, which convicts `h` iff no clearing chord
//! stands at or below `h + z_tol`).
//!
//! No sampling in Z, so no hidden slab can slip between planes however
//! thin - down to [`THIN`], the slicer's own plane nudge, under which
//! two faces are the same face to every plane law. A vertical that
//! cannot be read (it grazes an edge three times over, or the walk
//! does not close: an open or inside-out shell) convicts, fail closed.

use super::rest::Rest;
use super::solid_index::SolidIndex;
use crate::geo2d::Point2;

/// A gap thinner than this is no gap, mm: the plane nudge of
/// `slice::material_above` / `shadow_above` (two coincident faces of
/// touching bodies, apart by float noise).
const THIN: f64 = 1e-6;

/// The offsets a grazing vertical is retried at
/// ([`SolidIndex`]'s own parity rule).
/// The model's facets under an XY grid, built on the first ask of a
/// path's verdict and held by the caller (`reach`); the inner `None`
/// is a mesh that cannot be read - failing closed.
pub(super) type Columns<'a> = std::cell::RefCell<Option<Option<SolidIndex<'a>>>>;

const ASIDE: [(f64, f64); 3] = [(0.0, 0.0), (1.37e-7, 0.71e-7), (-0.93e-7, 1.73e-7)];

/// The air-under-material stretches `(under, over)` of a vertical's
/// crossings (highest first; `up` at an up-facing facet): `None` when
/// the walk does not close.
fn gaps_of(cross: &[(f64, bool)]) -> Option<Vec<(f64, f64)>> {
    let mut out = Vec::new();
    let mut depth = 0i64;
    for (k, &(z, up)) in cross.iter().enumerate() {
        depth += if up { 1 } else { -1 };
        if depth < 0 {
            return None;
        }
        let under = cross.get(k + 1).map_or(f64::NEG_INFINITY, |c| c.0);
        if depth == 0 && z - under > THIN {
            out.push((under, z));
        }
    }
    (depth == 0).then_some(out)
}

/// Whether some `h` in the closed range `[a, b]` lies strictly inside
/// the open stretch `(under, over)`.
fn meets(under: f64, over: f64, a: f64, b: f64) -> bool {
    if a.is_nan() || b.is_nan() {
        return false; // the NonFinite law names it
    }
    let s = under.max(a);
    s <= b && s < over && (under < s || under < b.min(over))
}

/// One column's hidden stretches, read once and asked per range.
pub(super) struct Column {
    gaps: Vec<(f64, f64)>,
    /// No uncut hidden stock at or above this height: the stock top,
    /// or the lowest clearing cut less `z_tol`.
    limit: f64,
}

impl Column {
    /// Is the column hidden, uncut stock at some height of `[a, b]`?
    pub(super) fn hidden_in(&self, a: f64, b: f64) -> bool {
        self.gaps.iter().any(|&(under, over)| meets(under, over.min(self.limit), a, b))
    }
}

impl<'a> Rest<'_, 'a> {
    /// The hidden stretches of the column at `q` before move `upto`:
    /// `Ok(None)` when nothing is hidden there at any height (outside
    /// the stock, or no part material hangs over air), `Err(())` when
    /// the vertical cannot be read - the caller fails closed.
    pub(super) fn hidden_column(&self, columns: &Columns<'a>, q: Point2, upto: usize) -> Result<Option<Column>, ()> {
        let cx = self.cx();
        let Some(facets) = cx.model else {
            return Ok(None);
        };
        if !self.in_stock(q) {
            return Ok(None);
        }
        let gaps = {
            let mut slot = columns.try_borrow_mut().map_err(|_| ())?;
            let index = slot.get_or_insert_with(|| SolidIndex::build(facets)).as_mut().ok_or(())?;
            ASIDE
                .iter()
                .find_map(|&(dx, dy)| index.column(q.x + dx, q.y + dy).and_then(|c| gaps_of(&c)))
                .ok_or(())?
        };
        if gaps.is_empty() {
            return Ok(None);
        }
        // `Rest::uncut` at plane h: no clearing chord at or below
        // h + z_tol. The walk is deepest first, so the first clearing
        // chord is the lowest; every h under it less z_tol is uncut.
        let cleared = self
            .near(q, 0)
            .find(|c| c.3 < upto && self.clears(c, q))
            .map_or(f64::INFINITY, |c| c.2 - cx.z_tol);
        Ok(Some(Column { gaps, limit: cleared.min(cx.stock_top_z) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ledge_over_a_floor_leaves_one_gap_and_the_air_under_the_part() {
        // Ledge 10..12 over a floor 0..4: crossings 12 up, 10 down, 4 up, 0 down.
        let g = gaps_of(&[(12.0, true), (10.0, false), (4.0, true), (0.0, false)]);
        assert_eq!(g, Some(vec![(4.0, 10.0), (f64::NEG_INFINITY, 0.0)]));
    }

    #[test]
    fn touching_and_overlapping_bodies_read_as_their_union() {
        // Two bodies butted at z 5, in either tie order: no gap at the seam.
        for seam in [[(5.0, false), (5.0, true)], [(5.0, true), (5.0, false)]] {
            let cross = [(9.0, true), seam[0], seam[1], (0.0, false)];
            assert_eq!(gaps_of(&cross), Some(vec![(f64::NEG_INFINITY, 0.0)]));
        }
        // One body through another (3..7 inside 0..9): depth 2, no gap.
        let through = [(9.0, true), (7.0, true), (3.0, false), (0.0, false)];
        assert_eq!(gaps_of(&through), Some(vec![(f64::NEG_INFINITY, 0.0)]));
    }

    #[test]
    fn a_walk_that_does_not_close_is_refused() {
        assert_eq!(gaps_of(&[(9.0, true)]), None, "an open shell");
        assert_eq!(gaps_of(&[(9.0, false), (0.0, true)]), None, "an inside-out shell");
        assert_eq!(gaps_of(&[]), Some(vec![]), "nothing over the column");
    }

    #[test]
    fn a_range_meets_a_gap_only_strictly_inside_it() {
        // The gap (4, 10): a range touching only its floor or ceiling is part material.
        assert!(meets(4.0, 10.0, 5.0, 5.0), "one plane inside");
        assert!(meets(4.0, 10.0, -20.0, 20.0), "a range holding the whole gap - the lowest plane is under the floor");
        assert!(meets(4.0, 10.0, 0.0, 4.5) && meets(4.0, 10.0, 9.5, 30.0));
        assert!(!meets(4.0, 10.0, 0.0, 4.0) && !meets(4.0, 10.0, 10.0, 30.0));
        assert!(!meets(4.0, 10.0, 4.0, 4.0) && !meets(4.0, 10.0, 6.0, 5.0));
        assert!(!meets(4.0, 4.0, 0.0, 9.0) && !meets(4.0, 10.0, f64::NAN, 9.0));
    }

    #[test]
    fn the_stock_top_and_an_earlier_cut_cap_the_gap() {
        let col = Column { gaps: vec![(4.0, 10.0)], limit: 6.0 };
        assert!(col.hidden_in(5.0, 8.0));
        assert!(!col.hidden_in(6.0, 8.0), "cleared (or over the stock top) from 6 up");
    }
}
