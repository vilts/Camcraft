//! The playhead in move terms (M7.2 C, #1516): which move the transport
//! stands on and how far along it the tool is drawn, and the partial
//! picture of that move. Provenance: the timeline's own segment law
//! (`sim::Script`, one segment per move behind at most one connector
//! rapid) and the field engine's order-free subtraction (a prefix of a
//! move is a move). Nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Only the move in flight is partial.** A connector rapid, a
//!   finished op and the end of the program show no partial cut; the
//!   fraction is 0 there, so the picture is the completed-move stock.
//! - **A rotary or turning op shows whole moves.** Their slices are
//!   chorded about an axis from the pose before them; a straight
//!   sub-move would be a different cut, so their move in flight stays
//!   uncut until it completes (the pre-#1516 picture).

use super::{FieldPlay, PlayIndex};
use crate::sim::Script;
use crate::stock_sim::{clamp_frac, SimOp};

/// Where the transport's playhead stands, in the timeline's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    /// The op under the playhead.
    pub op: String,
    /// Its segments completed (a connector rapid counts here).
    pub segs_done: usize,
    /// Its segments in all.
    pub span: usize,
}

/// The playhead at `t` as a [`Progress`]: `None` for an empty
/// timeline. Before the first segment completes nothing is done; at or
/// past the end every segment of the last op is. Only completed
/// segments count - the one under the playhead is `Script::travelled`'s.
#[must_use]
pub fn progress(sc: &Script, t: f64) -> Option<Progress> {
    let last = sc.segs.len().checked_sub(1)?;
    let t = if t.is_finite() { t } else { 0.0 };
    // The segment the playhead is inside: the first whose end is past t.
    let i = sc.cum.partition_point(|&c| c <= t).min(last);
    let seg = sc.segs.get(i)?;
    let op = sc.ops.get(seg.op)?.clone();
    let first = sc.segs.iter().position(|s| s.op == seg.op)?;
    let end = sc.segs.iter().rposition(|s| s.op == seg.op)?;
    let finished = if t >= sc.total { i + 1 } else { i };
    let span = end + 1 - first;
    Some(Progress { op, segs_done: finished.saturating_sub(first).min(span), span })
}

/// The fraction a picture at move `at` of `total` may show: none at the
/// end (nothing is in flight), else `frac` made safe.
#[must_use]
pub fn settle_frac(at: usize, total: usize, frac: f64) -> f64 {
    if at >= total { 0.0 } else { clamp_frac(frac) }
}

impl PlayIndex {
    /// The move the playhead `p` stands on and how far the tool is
    /// along it (`frac`, `Script::travelled`'s): `None` when the plan
    /// holds no op of that name. The fraction is 0 on a connector rapid,
    /// past an op's last move and at the end (the module law).
    #[must_use]
    pub fn position_at(&self, p: &Progress, frac: f64) -> Option<(usize, f64)> {
        let k = self.index_at(p)?;
        let (_, _, len) = self.ops.iter().find(|o| o.0 == p.op)?;
        let in_connector = p.span == len + 1 && p.segs_done == 0;
        let in_flight = !in_connector && p.segs_done < p.span && k < self.total;
        Some((k, if in_flight { clamp_frac(frac) } else { 0.0 }))
    }
}

impl FieldPlay {
    /// The travelled fraction of move `at` the picture shows.
    #[must_use]
    pub const fn frac(&self) -> f64 {
        self.frac
    }

    /// `seek(k)`, then show move `k` cut as far as fraction `frac` of
    /// its travel (#1516); `Ok(true)` when the picture changed. The
    /// stock after `k` whole moves is exactly `seek(k)`'s - the partial
    /// cut is on a copy - so `seek_to(k + 1, 0.0)` is the plain end of
    /// the move, bit for bit.
    pub fn seek_to(&mut self, k: usize, frac: f64) -> Result<bool, String> {
        let moved = self.seek(k)?;
        let f = settle_frac(self.at, self.total, frac);
        if !moved && f.to_bits() == self.frac.to_bits() {
            return Ok(false);
        }
        self.set_partial(f)?;
        Ok(true)
    }

    /// Cut the travelled part of move `at` onto a copy of `cur`.
    fn set_partial(&mut self, f: f64) -> Result<(), String> {
        self.drop_partial();
        self.frac = f;
        if f <= 0.0 {
            return Ok(());
        }
        let at = self.at;
        let Some(op) = self.ops.iter().find(|o| at >= o.start && at - o.start < o.moves.len()) else {
            return Ok(());
        };
        if op.rotary.is_some() || op.lathe.is_some() {
            return Ok(());
        }
        let Some(m) = op.moves.get(at - op.start) else {
            return Ok(());
        };
        let sim = SimOp { moves: &[], radius: op.radius, ball: op.ball, cone: op.cone, corner: op.corner };
        let cut = self.cur.with_partial(&sim, m, f).map_err(|e| format!("playback could not cut {}: {e}", op.name))?;
        if let Some((stock, bricks)) = cut {
            self.partial = Some(stock);
            self.partial_bricks = bricks;
        }
        Ok(())
    }

    /// Forget the partial picture (the move in flight changed).
    pub(super) fn drop_partial(&mut self) {
        self.partial = None;
        self.partial_bricks.clear();
        self.frac = 0.0;
    }
}
