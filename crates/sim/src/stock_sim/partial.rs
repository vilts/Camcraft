//! The travelled part of a move (#1516): what the playback shows while
//! the tool is still on a move - the stock cut by every completed move
//! plus the sweep of the move's start up to the tool's drawn position.
//! Provenance: the M7.1 field engine's order-free subtraction (a
//! prefix of a move is a move, so its sweep is the same closed-form
//! cut), and the timeline's own position law (`sim::point_along`), so
//! the material ends exactly where the tool is drawn. Nothing from
//! vendor binaries.
//!
//! ## The laws here
//!
//! - **Display only.** The partial cut is made on a copy of the
//!   completed-move stock; the stock a completed move leaves, and every
//!   verdict, are untouched by it.
//! - **Nothing travelled cuts nothing.** A fraction at or below zero,
//!   not finite, or a move with no length or a non-finite coordinate
//!   gives no partial move - never a panic, never a guessed cut.
//! - **The whole fraction is the whole move.** At `f >= 1` the partial
//!   move IS the move, bit for bit.

use std::collections::HashSet;

use crate::cl::Move;
use crate::sim::{leg_frac, point_along, Script};

use super::backend::Stock;
use super::cut::SimOp;
use super::kernel::StockError;

/// Below this the tool has not visibly left the move's start: no
/// partial move (a sub-arc this short would have coincident ends).
const MIN_FRAC: f64 = 1e-9;

/// A playhead fraction made safe: `[0, 1]`, anything not finite is 0.
#[must_use]
pub fn clamp_frac(f: f64) -> f64 {
    if f.is_finite() { f.clamp(0.0, 1.0) } else { 0.0 }
}

/// The part of `m` the tool has travelled at fraction `f` of its
/// travel: the same start, the end at the tool's drawn position, an
/// arc keeping its centre and direction. `None` when nothing has been
/// travelled or the move cannot say where it goes (the module law).
#[must_use]
pub fn partial_move(m: &Move, f: f64) -> Option<Move> {
    let f = clamp_frac(f);
    if f < MIN_FRAC || !m.is_finite() || m.from == m.to {
        return None;
    }
    if f >= 1.0 {
        return Some(*m);
    }
    let to = point_along(m.from, m.to, m.arc, f);
    if !to.is_finite() || to == m.from {
        return None;
    }
    Some(Move { to, ..*m })
}

/// A partial picture: the stock and the bricks its partial cut changed.
pub type PartialCut = (Stock, HashSet<[i32; 3]>);

impl Stock {
    /// This stock plus the travelled part of `m` (fraction `f`), cut
    /// with `op`'s tool by the display law (`simulate_display`), on a
    /// copy: `Ok(None)` when nothing has been travelled. The bricks the
    /// partial cut changed come back with it - what a playback picture
    /// must re-send beyond the completed-move stock's own ledger.
    pub fn with_partial(&self, op: &SimOp, m: &Move, f: f64) -> Result<Option<PartialCut>, StockError> {
        let Some(sub) = partial_move(m, f) else {
            return Ok(None);
        };
        let mut shown = self.clone();
        // The copy's ledger is this stock's own pending set; drop it so
        // what is left after the cut is the partial's bricks alone.
        let _ = shown.take_dirty_bricks();
        let moves = [sub];
        shown.simulate_display(&[SimOp { moves: &moves, ..*op }])?;
        let bricks = shown.take_dirty_bricks().unwrap_or_default();
        Ok(Some((shown, bricks)))
    }
}

impl Script {
    /// How far along its travel the segment under the playhead at `t`
    /// is, as the tool is drawn (`Script::at`'s fraction): 0 before the
    /// start, past the end, or for a non-finite `t`; 1 while it dwells
    /// at its end. The segment is the one `stock_play::progress` names.
    #[must_use]
    pub fn travelled(&self, t: f64) -> f64 {
        let Some(last) = self.segs.len().checked_sub(1) else {
            return 0.0;
        };
        if !t.is_finite() || t <= 0.0 || t >= self.total {
            return 0.0;
        }
        let i = self.cum.partition_point(|&c| c <= t).min(last);
        let Some(s) = self.segs.get(i) else {
            return 0.0;
        };
        let start = if i == 0 { 0.0 } else { self.cum.get(i - 1).copied().unwrap_or(0.0) };
        let local = t - start;
        if s.secs.is_nan() || s.secs <= 0.0 || local >= s.secs {
            return 1.0;
        }
        clamp_frac(leg_frac(s.pl, clamp_frac(local / s.secs), s.secs))
    }
}

#[cfg(test)]
#[path = "partial_tests.rs"]
mod tests;
