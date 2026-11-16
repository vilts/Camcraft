//! Law (c3)'s overhang backstop over swept cells (#1316): stock hidden under a ledge, asked over the same reachable set the column law reads, where it used to be asked at 4 path stations x 5 turn fractions.
//!
//! Provenance: the cells are [`super::turn_cells`]' (convex hull of two
//! discs, the linear interpolation error bound `max|f''| w^2 / 8`), the
//! hidden-stock law is [`super::hidden`]'s; the plane budget is ours.
//! No vendor source consulted.
//!
//! The hidden-stock law slices the model once per distinct plane
//! (cached by its bits), so the backstop cannot afford the column law's
//! 0.001 mm cells. It builds its own, coarser cover of the same
//! reachable set:
//!
//! - the turn split so the sag is at most [`BACKSTOP_SAG`] (a tenth of
//!   the hidden law's own 0.5 mm XY spacing), capped at
//!   [`BACKSTOP_PLANES`] intervals - past the cap the cells only grow,
//!   which convicts more, never less;
//! - the tip's chords grouped into at most [`STATIONS`] runs, and no
//!   more than `BACKSTOP_PLANES / intervals` of them;
//! - one ask per (interval, run), over the union of the run's cells'
//!   footprints, the kerf excused only where the tip stands (the cells'
//!   own test). The ask slices ONE plane, the lowest bottom any column
//!   of its cells reaches, and reads every column it asks over the
//!   whole range of heights the band's bottom takes there
//!   ([`Ask::heights`], [`super::hidden_span`]).
//! - every cell holding one of the sampled turn fractions reads no
//!   higher than #1303's span floor there ([`super::sweep::band_floors`],
//!   the column law's own floors), so no ask stands above the column
//!   the sampled backstop read at that pose (#1313).
//!
//! Why the range and not the lowest plane alone (#1324 review): the
//! column law is monotone in the plane - a column under the band's
//! lowest bottom is under every higher one - but the hidden law is not.
//! `hidden_at(h)` is the shadow of what hangs over `h` less the section
//! at `h`; a cavity over a floor is hidden stock at its own planes and
//! part material at the floor's. An ask's lowest plane is the deep end
//! of a whole run of the tip's path, so read alone it passed a band
//! that crossed the cavity higher up. A lower plane does NOT only add
//! convictions; a wider range does, and the range a column is asked
//! over holds every bottom the band takes there: each grid column of
//! the sampled backstop (its plane lies between the cell's lowest and
//! highest bottom) and the lowest plane's own reading, kept beside it.
//!
//! So a move slices at most [`BACKSTOP_PLANES`] planes per live band -
//! the old grid's 4 x 5 = 20 - and its footprint is read whole between
//! them, in XY and in Z. The hidden region is sampled in XY by its own
//! law (`hidden_in_cover`: 0.5 mm).

use super::sweep::band_floors;
use super::turn_cells::{Cell, push_cells_with};
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// The backstop cells' sag budget, mm.
pub(super) const BACKSTOP_SAG: f64 = 0.05;
/// Most distinct planes one move asks per live band (the old grid's 20).
pub(super) const BACKSTOP_PLANES: usize = 20;
/// Most runs the tip's chords are grouped into (the old grid's stations).
pub(super) const STATIONS: usize = 4;
/// Tip positions probed along one chord ([`Cell::probes`]).
const PROBE_STATIONS: usize = 4;

/// One backstop ask: the box, the lowest plane and the cells whose
/// union is the footprint.
pub(super) struct Ask {
    pub lo: Point2,
    pub hi: Point2,
    pub plane: f64,
    pub cells: Vec<Cell>,
}

impl Ask {
    /// Points of the footprint ([`Cell::probes`]), built only when the
    /// plane has hidden stock near the box.
    pub(super) fn probes(&self) -> Vec<Point2> {
        let mut out = Vec::new();
        for c in &self.cells {
            c.probes(PROBE_STATIONS, &mut out);
        }
        out
    }

    /// Whether some position of the ask's cells stands the band over
    /// `q`, outside the flutes' kerf at that position.
    #[cfg(test)]
    pub(super) fn covers(&self, q: Point2, kerf2: f64) -> bool {
        self.heights(q, kerf2, &mut |_, _| true)
    }

    /// Every height the band's bottom takes over `q` in the ask's
    /// cells ([`Cell::heights_over`]): `hit` is asked one closed range
    /// per cell and stretch, the first `true` is returned. No range:
    /// no position of the ask stands the band over `q` outside the
    /// flutes' kerf.
    pub(super) fn heights(&self, q: Point2, kerf2: f64, hit: &mut dyn FnMut(f64, f64) -> bool) -> bool {
        self.cells.iter().any(|c| {
            q.x >= c.lo.x && q.x <= c.hi.x && q.y >= c.lo.y && q.y <= c.hi.y && c.heights_over(q, kerf2, hit)
        })
    }
}

/// The backstop's asks for a move with tip `pieces` turning from the
/// letters `from` to `to`, over every live band - at most
/// [`BACKSTOP_PLANES`] per band (see the module law).
pub(super) fn backstop_asks(
    pieces: &[(Point3, Point3)],
    from: [f64; 3],
    to: [f64; 3],
    bands: &[(f64, f64, f64)],
    r: f64,
    grow: f64,
) -> Vec<Ask> {
    let mut asks = Vec::new();
    if pieces.is_empty() {
        return asks;
    }
    for &band in bands {
        let mut cells = Vec::new();
        // #1303's span floors, as the column law's cells hold them
        // (#1313): a cell holding a sampled pose reads no higher than
        // its floor, so every ask's plane is at most the sampled
        // backstop's column bottom there.
        let floors = band_floors(from, to, band);
        let n = push_cells_with(&mut cells, pieces, from, to, band, r, grow, (BACKSTOP_SAG, BACKSTOP_PLANES), &floors);
        if n == 0 || cells.is_empty() {
            continue;
        }
        let runs = (BACKSTOP_PLANES / n).clamp(1, STATIONS).min(pieces.len());
        // Within an interval the cells come in piece order, one per piece.
        let mut slot: Vec<Option<Ask>> = (0..n * runs).map(|_| None).collect();
        let (mut last, mut k) = (usize::MAX, 0usize);
        for c in cells {
            if c.interval != last {
                (last, k) = (c.interval, 0);
            }
            let run = (k * runs / pieces.len()).min(runs - 1);
            k += 1;
            let Some(entry) = slot.get_mut(c.interval * runs + run) else {
                continue; // #[cold]: interval < n by construction
            };
            let ask = entry.get_or_insert_with(|| Ask {
                lo: c.lo,
                hi: c.hi,
                plane: c.floor(),
                cells: Vec::new(),
            });
            ask.lo = Point2::new(ask.lo.x.min(c.lo.x), ask.lo.y.min(c.lo.y));
            ask.hi = Point2::new(ask.hi.x.max(c.hi.x), ask.hi.y.max(c.hi.y));
            ask.plane = ask.plane.min(c.floor());
            ask.cells.push(c);
        }
        asks.extend(slot.into_iter().flatten());
    }
    asks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_move_asks_at_most_twenty_planes_per_band() {
        let bands = [(100.0, 4.0, 4.0), (20.0, 6.0, 10.0)];
        let one = [(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 0.0, -1.0))];
        let many: Vec<(Point3, Point3)> =
            (0..50).map(|i| (Point3::new(i as f64, 0.0, 0.0), Point3::new(i as f64 + 1.0, 0.0, -0.1 * i as f64))).collect();
        for pieces in [&one[..], &many[..]] {
            for to in [[0.0, 1.0, 0.0], [0.0, 10.0, 0.0], [0.0, 90.0, 0.0], [45.0, 80.0, 120.0], [f64::NAN, 1.0, 0.0]] {
                let asks = backstop_asks(pieces, [0.0; 3], to, &bands, 3.0, 0.0);
                assert!(asks.len() <= BACKSTOP_PLANES * bands.len(), "{to:?}: {} asks", asks.len());
                let mut planes: Vec<u64> = asks.iter().map(|a| a.plane.to_bits()).collect();
                planes.sort_unstable();
                planes.dedup();
                assert!(planes.len() <= BACKSTOP_PLANES * bands.len());
                // Every cell is in exactly one ask, at or above its plane.
                for a in &asks {
                    assert!(a.cells.iter().all(|c| c.floor() >= a.plane));
                }
            }
        }
    }

    #[test]
    fn an_ask_covers_the_holder_between_its_end_poses() {
        // The #1316 fixture's swing: a holder 100 mm up, B 0 -> 80. At
        // B 30 its disc is centred at X ~51; some ask must cover it.
        let pieces = [(Point3::new(0.0, 0.0, 0.0), Point3::new(0.0, 0.0, -1.0))];
        let asks = backstop_asks(&pieces, [0.0; 3], [0.0, 80.0, 0.0], &[(100.0, 4.0, 4.0)], 3.0, 0.0);
        let q = Point2::new(102.0 * 30f64.to_radians().sin(), 0.0);
        assert!(asks.iter().any(|a| a.covers(q, 9.0)), "{} asks", asks.len());
    }
}
