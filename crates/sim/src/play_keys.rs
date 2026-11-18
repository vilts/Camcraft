//! The playback keyframe store (#2181): snapshots of the cut stock at
//! a fixed move stride, kept within a count cap and, once a calculation
//! before playback asks for one, a byte budget. Provenance: plain
//! periodic snapshotting with keyframe thinning (halve the keyframes,
//! double the stride) for the memory bound; the order-free field
//! subtraction (any prefix of the program is cut exactly by its own
//! moves) makes each kept keyframe exact. Nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Display only.** The keyframes are the playback picture's
//!   checkpoints and nothing else reads them. The Deviate verdict and
//!   the checks that block the post are walked elsewhere, unchanged.
//! - **One insertion path.** Every keyframe enters through `offer`, the
//!   calculation's and the live playback's alike, so the count cap and
//!   the byte budget hold whoever cuts (the #2192 review: a live seek
//!   after a cancel once added keyframes past the budget).
//! - **Memory is bounded by a budget, never by the program.** With a
//!   budget, the kept keyframes' bytes are summed as they are offered;
//!   past the budget every other keyframe goes and the stride doubles,
//!   so the store never holds more than the budget (at worst only the
//!   uncut stock, which is key 0 and is never thinned).
//! - **Incomplete is said, never shown as done.** A cancelled or failed
//!   calculation is `Precalc::Incomplete` by name; the keyframes it made
//!   stay valid (each is an exact prefix) and the moves past them are
//!   cut live.

use crate::stock_sim::{SdfField, Stock};

/// The keyframe store's memory budget: 512 MiB of cut layers.
pub const PRECALC_BUDGET: usize = 512 << 20;

/// Keyframes at most in a calculation: its stride is at least the
/// program over this.
pub const MAX_KEYFRAMES: usize = 1024;

/// Where the calculation before playback stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Precalc {
    /// Not asked: playback cuts as the playhead moves.
    Off,
    /// Cutting: `done` of `total` moves have keyframes behind them.
    Running { done: usize, total: usize },
    /// Every move is calculated: a seek cuts at most `stride` moves.
    Ready { keys: usize, bytes: usize, stride: usize, secs: f64 },
    /// Stopped at `done` of `total` moves, and why: past `done` the
    /// playback cuts live.
    Incomplete { done: usize, total: usize, why: String },
}

/// What one kept keyframe costs, in bytes.
pub trait KeyCost {
    /// The bytes a clone of this value holds of its own.
    fn key_bytes(&self) -> usize;
}

impl KeyCost for Stock {
    /// A clone copies the cut layers only (the mesh layer is shared).
    fn key_bytes(&self) -> usize {
        self.field().map_or(0, SdfField::cut_layer_bytes)
    }
}

/// The keyframes: `(moves cut, the value after them)`, ascending; key 0
/// is the uncut value and always stays.
#[derive(Clone, Debug)]
pub struct Keyframes<T> {
    keys: Vec<(usize, T)>,
    stride: usize,
    /// Keyframes past key 0 at most.
    cap: usize,
    /// The byte budget, once a calculation set one.
    budget: Option<usize>,
    /// Bytes the kept keyframes hold.
    bytes: usize,
}

impl<T: KeyCost + Clone> Keyframes<T> {
    /// The uncut value alone, a stride, a count cap and no byte budget.
    pub fn new(uncut: T, stride: usize, cap: usize) -> Self {
        Keyframes { keys: vec![(0, uncut)], stride: stride.max(1), cap, budget: None, bytes: 0 }
    }

    /// Back to the uncut value with a new stride, count cap and byte
    /// budget (the calculation's start); the budget stays from here on.
    pub fn restart(&mut self, stride: usize, cap: usize, budget: usize) {
        self.keys.truncate(1);
        self.stride = stride.max(1);
        self.cap = cap;
        self.budget = Some(budget);
        self.bytes = 0;
    }

    /// The stride, in moves.
    #[must_use]
    pub const fn stride(&self) -> usize {
        self.stride
    }

    /// Keyframes held, key 0 included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Never true: key 0 always stays.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Bytes the kept keyframes hold (within the budget, when set).
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.bytes
    }

    /// The bytes the kept keyframes hold, summed again from them: the
    /// audit of `bytes` (a walk over the keys; not for a frame loop).
    #[must_use]
    pub fn held_bytes(&self) -> usize {
        self.keys.iter().map(|(_, s)| s.key_bytes()).fold(0, usize::saturating_add)
    }

    /// The byte budget, once a calculation set one.
    #[must_use]
    pub const fn budget(&self) -> Option<usize> {
        self.budget
    }

    /// The newest keyframe at or before `k`.
    #[must_use]
    pub fn floor(&self, k: usize) -> Option<&(usize, T)> {
        self.keys.iter().rev().find(|(n, _)| *n <= k)
    }

    /// The move index of `floor(k)` (0 when none is nearer).
    #[must_use]
    pub fn floor_at(&self, k: usize) -> usize {
        self.floor(k).map_or(0, |c| c.0)
    }

    /// A seek from `at` to `k` restores `floor(k)` before it cuts: `k`
    /// is behind, or a keyframe stands more than one stride ahead (the
    /// replay law: a seek cuts at most one stride past a keyframe).
    #[must_use]
    pub fn restores(&self, at: usize, k: usize) -> bool {
        k < at || (self.floor_at(k) > at && k.saturating_sub(at) > self.stride)
    }

    /// The first stride boundary past `at`, clamped to `total`.
    #[must_use]
    pub fn next_boundary(&self, at: usize, total: usize) -> usize {
        (at / self.stride).saturating_add(1).saturating_mul(self.stride).min(total)
    }

    /// Keep `value` as the keyframe at move `at`, when it is past the
    /// last one and the count cap has room; then thin to the budget.
    /// `true` when it was kept and survived the thinning.
    pub fn offer(&mut self, at: usize, value: &T) -> bool {
        let last = self.keys.last().map_or(0, |c| c.0);
        if at <= last || self.keys.len() > self.cap {
            return false;
        }
        self.bytes = self.bytes.saturating_add(value.key_bytes());
        self.keys.push((at, value.clone()));
        if let Some(budget) = self.budget {
            while self.bytes > budget && self.keys.len() > 1 {
                self.thin();
            }
        }
        self.keys.last().is_some_and(|c| c.0 == at)
    }

    /// Halve the keyframes: double the stride and keep only those on it.
    fn thin(&mut self) {
        self.stride = self.stride.saturating_mul(2).max(1);
        let stride = self.stride;
        self.keys.retain(|(n, _)| n % stride == 0);
        self.bytes = self.held_bytes();
    }
}
