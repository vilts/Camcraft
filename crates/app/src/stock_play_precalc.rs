//! `FieldPlay` as the sim crate's `Playback` (#2181): the accessors and
//! the cut, rewind and store bindings that `camcraft_sim::play_precalc`
//! drives. The keyframe store, its budget and thinning, the stride, the
//! begin/step orchestration and the loop are the sim crate's
//! (`play_keys`, `play_precalc`); only the bindings of this app type
//! stay here, since `FieldPlay` itself is app's until #2268 (#2192
//! review). Provenance: our own seam; nothing from vendor binaries.

use super::FieldPlay;
use crate::stock_sim::Stock;
use camcraft_sim::play_keys::Keyframes;
use camcraft_sim::play_precalc::{self, Playback};

impl FieldPlay {
    /// The checkpoint stride, in moves.
    #[must_use]
    pub const fn stride(&self) -> usize {
        self.keys.stride()
    }

    /// Moves cut since the build (the replay law's yardstick: a seek
    /// after the calculation adds at most one stride).
    #[must_use]
    pub const fn moves_cut(&self) -> usize {
        self.moves_cut
    }

    /// Cut-layer bytes the keyframes hold (within the budget once a
    /// calculation set one, live seeks included).
    #[must_use]
    pub const fn keyframe_bytes(&self) -> usize {
        self.keys.bytes()
    }

    /// The checkpoint store (read only).
    #[must_use]
    pub const fn keyframes(&self) -> &Keyframes<Stock> {
        &self.keys
    }

    /// `camcraft_sim::play_precalc::begin` on this playback.
    pub fn begin_precalc(&mut self, keys: usize, min_stride: usize, budget: usize) -> Result<(), String> {
        play_precalc::begin(self, keys, min_stride, budget)
    }

    /// `camcraft_sim::play_precalc::step` on this playback.
    pub fn precalc_step(&mut self) -> Result<bool, String> {
        play_precalc::step(self)
    }
}

impl Playback for FieldPlay {
    type Value = Stock;

    fn total(&self) -> usize {
        self.total
    }

    fn at(&self) -> usize {
        self.at
    }

    fn keys(&self) -> &Keyframes<Stock> {
        &self.keys
    }

    fn keys_and_value(&mut self) -> (&mut Keyframes<Stock>, &Stock) {
        (&mut self.keys, &self.cur)
    }

    /// The uncut stock at move 0, the partial move gone, the dirty
    /// ledger off (no picture is taken while the calculation runs).
    fn rewind(&mut self, uncut: Stock) {
        self.drop_partial();
        self.cur = uncut;
        self.at = 0;
        self.set_batched(true);
        self.restored = true;
    }

    fn cut_to(&mut self, to: usize) -> Result<(), String> {
        self.cut(self.at, to)?;
        self.at = to;
        Ok(())
    }
}
