//! Issue #2121: law (d)'s traverses at the clearance plane re-read against the stock the setup's earlier operations left standing (the re-machining rest grid) - our own reading of the shop rule "never traverse through uncut stock" on the dexel model; no vendor material consulted.
//!
//! Since #2121 the column law judges a lateral rapid AT (or above) the
//! clearance plane like one under it: a plane set inside the stock is no
//! promise of air. The column it reads is `min(this op's own cuts before
//! the rapid, the stock top)` - the raw block, wherever this operation
//! has not cut. A clearance plane under the raw stock top is still legal
//! where the program ahead faced the stock down first, so a traverse
//! conviction is re-read here against the prior grid, the way
//! [`super::acquit_by_prior`] re-reads a dive.
//!
//! Only removes a conviction, and only for an upright lateral rapid that
//! rides at or above the clearance plane (a rapid UNDER the plane keeps
//! every conviction, as before #2121: [`super::acquit_by_prior`]'s
//! "a lateral conviction is never re-read"). The rapid is read level at
//! its low end over the whole capsule its footprint sweeps:
//!
//! - every prior column whose centre lies within the tool radius plus
//!   half a cell diagonal of the rapid's plan trace must stand at or
//!   under the tip (less `z_tol`) - the padding means a capsule that
//!   covers no cell centre touches no cell at all, and off the grid is
//!   outside the stock (air);
//! - a column whose height is not a number is unknown: the raw stock;
//! - the PART must stand under the tip too, by the highest vertex of any
//!   facet whose plan comes within the tool radius of the trace
//!   ([`super::stock_top`]'s bound, grown and lifted by the model excess
//!   `e`). The grid is the stock's; a part reaching out of its stock is
//!   never air because the grid is silent about it - the solid law's
//!   conviction of the same rapid shares the warning and must not be
//!   acquitted with the column's.
//!
//! Fails closed: a non-finite number anywhere, a posed rapid, a mode
//! whose rapids the vertical column does not judge, or a capsule over
//! more than [`MAX_CELLS`] columns keeps the conviction.

use super::laws::dist2_seg;
use super::stock_top::top_within;
use super::types::{Mode, Warning};
use super::upright::is_upright;

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Triangle;
use crate::stock_sim::Columns;

/// The most prior columns one traverse is read over; past it the rapid
/// keeps its conviction rather than being read at a thinned lattice.
const MAX_CELLS: f64 = 16_000_000.0;

/// What a traverse is re-read against, beside the prior grid.
#[derive(Clone, Copy, Debug)]
pub struct Traverse<'a> {
    /// The op's own material (its part), in the path's frame.
    pub part: &'a [Triangle],
    /// The tool's cutting radius.
    pub tool_r: f64,
    /// The op's clearance plane: only rapids at or above it are re-read.
    pub clearance_z: f64,
    pub z_tol: f64,
    /// The op's model excess (#864/#904): the part may stand that much
    /// outside its facets.
    pub excess: f64,
}

/// `warns` without the convictions of upright lateral rapids at or above
/// the clearance plane that travel only over stock the program ahead
/// already cut under them (see the module doc).
pub fn acquit_traverses_by_prior(
    path: &Toolpath,
    mode: Mode,
    prior: &Columns<'_>,
    cx: &Traverse<'_>,
    warns: &mut Vec<Warning>,
) {
    let numbers = [cx.tool_r, cx.clearance_z, cx.z_tol, cx.excess, prior.cell()];
    if matches!(mode, Mode::Radial | Mode::Revolve | Mode::Rotary)
        || !numbers.iter().all(|v| v.is_finite())
        || cx.tool_r < 0.0
        || prior.cell() <= 0.0
    {
        return;
    }
    warns.retain(|w| {
        let Warning::RapidInStock { at } = *w else { return true };
        let Some(m) = path.moves.get(at) else { return true };
        if m.kind != MoveKind::Rapid || !m.axis.is_none_or(|a| is_upright(&a)) {
            return true;
        }
        if !(m.from.is_finite() && m.to.is_finite()) {
            return true;
        }
        let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let low = m.from.z.min(m.to.z);
        let lateral = dx * dx + dy * dy > cx.z_tol * cx.z_tol;
        if !lateral || low < cx.clearance_z - cx.z_tol {
            return true;
        }
        let e = cx.excess.max(0.0);
        let over_part = top_within(&[cx.part], (a, b), cx.tool_r + e).is_some_and(|top| low - e < top - cx.z_tol);
        over_part || !prior_clear(prior, (a, b), cx.tool_r, low, cx.z_tol)
    });
}

/// Whether every prior column under the capsule of radius `r` (padded by
/// half a cell diagonal) about the plan segment `a`-`b` stands at or
/// under `low` (less `z_tol`). An unreadable column, or too many of
/// them, is no proof.
fn prior_clear(dex: &Columns<'_>, (a, b): (Point2, Point2), r: f64, low: f64, z_tol: f64) -> bool {
    let cell = dex.cell();
    let o = dex.origin();
    let rr = r + cell * std::f64::consts::FRAC_1_SQRT_2;
    // The capsule's box in lattice indices, clamped to the grid: a NaN
    // bound fails the count check below.
    let lo = |v: f64, o: f64| ((v - rr - o) / cell).floor().max(0.0);
    let hi = |v: f64, o: f64, n: usize| ((v + rr - o) / cell).ceil().min(n as f64);
    let (ix0, ix1) = (lo(a.x.min(b.x), o.x), hi(a.x.max(b.x), o.x, dex.nx()));
    let (iy0, iy1) = (lo(a.y.min(b.y), o.y), hi(a.y.max(b.y), o.y, dex.ny()));
    let (nx, ny) = ((ix1 - ix0).max(0.0), (iy1 - iy0).max(0.0));
    let count = nx * ny;
    if !count.is_finite() || count > MAX_CELLS {
        return false;
    }
    let r2 = rr * rr;
    let (ix0, iy0) = (ix0 as usize, iy0 as usize);
    for iy in iy0..iy0.saturating_add(ny as usize) {
        for ix in ix0..ix0.saturating_add(nx as usize) {
            if dist2_seg(dex.center(ix, iy), a, b) > r2 {
                continue;
            }
            let top = dex.top_at(ix, iy);
            // A height that is not a number is unknown: the raw stock.
            if top.is_nan() || top > low + z_tol {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::Move;
    use crate::geometry::{Point3, cube};
    use crate::stock_sim::Dexel;

    /// One lateral rapid from x = `x0` to x = `x1` on y = 0 at height `z`.
    fn traverse(x0: f64, x1: f64, z: f64) -> Toolpath {
        Toolpath::new(vec![Move {
            rotary: None,
            kind: MoveKind::Rapid,
            from: Point3::new(x0, 0.0, z),
            to: Point3::new(x1, 0.0, z),
            feed: None,
            arc: None,
            axis: None,
        }])
    }

    /// A 40 x 40 block, top Z 10, faced down to Z `faced` where x < `upto`.
    fn faced(faced: f64, upto: f64) -> Dexel {
        let mut d = match Dexel::block(-20.0, -20.0, 20.0, 20.0, 10.0, 0.25) {
            Ok(d) => d,
            Err(e) => panic!("block: {e:?}"),
        };
        for iy in 0..d.ny {
            for ix in 0..d.nx {
                let c = d.center(ix, iy);
                if c.x < upto
                    && let Some(k) = d.col(c)
                {
                    d.lower(k, faced);
                }
            }
        }
        d
    }

    fn cx(part: &[Triangle], clearance_z: f64) -> Traverse<'_> {
        Traverse { part, tool_r: 3.0, clearance_z, z_tol: 1e-3, excess: 0.0 }
    }

    fn judged(path: &Toolpath, dex: &Dexel, cx: &Traverse<'_>) -> Vec<Warning> {
        let mut w = vec![Warning::RapidInStock { at: 0 }];
        acquit_traverses_by_prior(path, Mode::Area, &Columns::of_grid(dex), cx, &mut w);
        w
    }

    #[test]
    fn a_traverse_over_stock_the_program_ahead_faced_is_acquitted() {
        // Clearance Z 8 under the raw top Z 10, the whole block faced to Z 5.
        let w = judged(&traverse(-30.0, 30.0, 8.0), &faced(5.0, 100.0), &cx(&[], 8.0));
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn a_traverse_through_raw_stock_stands() {
        let w = judged(&traverse(-30.0, 30.0, 8.0), &faced(10.0, 100.0), &cx(&[], 8.0));
        assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    }

    #[test]
    fn a_traverse_whose_rim_meets_the_unfaced_part_of_the_block_stands() {
        // Faced only where x < 0: the rapid ends at x = -2.9, its 3 mm
        // rim 0.1 mm over the raw columns.
        let w = judged(&traverse(-30.0, -2.9, 8.0), &faced(5.0, 0.0), &cx(&[], 8.0));
        assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
        // Pulled back clear of the raw columns, it is air.
        let w = judged(&traverse(-30.0, -4.0, 8.0), &faced(5.0, 0.0), &cx(&[], 8.0));
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn a_rapid_under_the_clearance_plane_is_never_re_read() {
        // The same faced block, but the rapid rides Z 8 under a
        // clearance plane of Z 9: the pre-#2121 conviction, kept whole.
        let w = judged(&traverse(-30.0, 30.0, 8.0), &faced(5.0, 100.0), &cx(&[], 9.0));
        assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    }

    #[test]
    fn a_traverse_off_the_grid_is_outside_the_stock() {
        let mut p = traverse(-30.0, 30.0, 8.0);
        if let Some(m) = p.moves.get_mut(0) {
            m.from.y = 40.0;
            m.to.y = 40.0;
        }
        let w = judged(&p, &faced(10.0, 100.0), &cx(&[], 8.0));
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn a_part_standing_out_of_the_faced_stock_keeps_the_conviction() {
        // The grid says Z 5 everywhere; the part's cube stands to Z 9
        // across the trace - the grid's silence is not air.
        let part = cube(Point3::new(-2.0, -2.0, 0.0), Point3::new(2.0, 2.0, 9.0)).facets;
        let w = judged(&traverse(-30.0, 30.0, 8.0), &faced(5.0, 100.0), &cx(&part, 8.0));
        assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
        // A part under the tip does not hold it.
        let low = cube(Point3::new(-2.0, -2.0, 0.0), Point3::new(2.0, 2.0, 5.0)).facets;
        let w = judged(&traverse(-30.0, 30.0, 8.0), &faced(5.0, 100.0), &cx(&low, 8.0));
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn unknown_numbers_posed_rapids_and_other_modes_keep_the_conviction() {
        let dex = faced(5.0, 100.0);
        let path = traverse(-30.0, 30.0, 8.0);
        let w = judged(&path, &dex, &Traverse { z_tol: f64::NAN, ..cx(&[], 8.0) });
        assert_eq!(w.len(), 1);
        let w = judged(&path, &dex, &Traverse { tool_r: f64::INFINITY, ..cx(&[], 8.0) });
        assert_eq!(w.len(), 1);
        let mut posed = path.clone();
        if let Some(m) = posed.moves.get_mut(0) {
            m.axis = Some([0.5, 0.0, 0.866]);
        }
        assert_eq!(judged(&posed, &dex, &cx(&[], 8.0)).len(), 1);
        for mode in [Mode::Radial, Mode::Revolve, Mode::Rotary] {
            let mut w = vec![Warning::RapidInStock { at: 0 }];
            acquit_traverses_by_prior(&path, mode, &Columns::of_grid(&dex), &cx(&[], 8.0), &mut w);
            assert_eq!(w.len(), 1, "{mode:?}");
        }
    }
}
