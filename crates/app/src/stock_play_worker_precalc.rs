//! The playback worker's side of the calculation before playback
//! (#2181): thread bindings only. The mailbox (progress, cancel, a lost
//! state by name), the loop, its ends and the keyframe store are the
//! sim crate's (`play_precalc_mailbox`, `play_precalc`, `play_keys`);
//! here the worker binds its stop flag, its clock, its wakeup and its
//! frame publication (#2192 review). Provenance: the worker's own
//! latest-wins mailbox (`stock_play_worker`); nothing from vendor
//! binaries.
//!
//! A worker started with `spawn_precalc` first cuts the whole program
//! once, keeping keyframes; its progress is `precalc()` and the UI may
//! `cancel_precalc()`. Requests wait until it ends. A cancel or a stop
//! leaves `Precalc::Incomplete` by name, and the moves past the last
//! keyframe are cut live, within the same byte budget.

use super::{lock, publish, PlayWorker, Shared};
use crate::clock::Instant;
use crate::stock_play::{FieldPlay, PlaySeed, Precalc, MAX_KEYFRAMES, MIN_STRIDE};
use camcraft_sim::play_precalc::{drive, Calc, PrecalcEnd};
use camcraft_sim::play_precalc_mailbox::MailboxOwner;

impl PlayWorker {
    /// Where the calculation before playback stands (`Off` when the
    /// worker was spawned without one; a lost state reads incomplete).
    #[must_use]
    pub fn precalc(&self) -> Precalc {
        self.shared.precalc.read(self.index.total())
    }

    /// Stop the calculation where it stands; playback goes on, cutting
    /// live past the last keyframe.
    pub fn cancel_precalc(&self) {
        self.shared.precalc.cancel();
        self.shared.wake.notify_all();
    }

    /// `spawn`, but the worker first calculates the whole removal with
    /// keyframes held to `budget` bytes (the sim crate's law).
    pub fn spawn_precalc(seed: PlaySeed, budget: usize) -> Result<Self, String> {
        Self::start(seed, Some(budget))
    }
}

/// A playback that would not build: with a calculation asked, that is
/// an incomplete calculation by name; then the refusal is published.
pub(super) fn refuse(shared: &Shared, budget: Option<usize>, e: String) {
    if budget.is_some() {
        shared.precalc.set(Precalc::Incomplete { done: 0, total: 0, why: e.clone() });
    }
    publish(shared, Err(e));
}

/// Run the calculation on the worker: `false` when the worker must end
/// (stopped, or a cut refused and published).
pub(super) fn precalc(shared: &Shared, play: &mut FieldPlay, budget: usize) -> bool {
    let t0 = Instant::now();
    let owner = MailboxOwner {
        mailbox: &shared.precalc,
        stopped: || lock(&shared.req).is_none_or(|g| g.stop),
        elapsed: || t0.elapsed().as_secs_f64(),
    };
    let end = drive(&mut Calc { play: &mut *play, keys: MAX_KEYFRAMES, min_stride: MIN_STRIDE, budget }, &owner);
    match end {
        PrecalcEnd::Play => {
            let p = shared.precalc.read(play.total());
            if matches!(p, Precalc::Ready { .. }) {
                tracing::info!("playback precalculated: {p:?}");
            }
            // The ledger starts fresh from the next (whole) picture.
            play.set_batched(false);
            true
        }
        PrecalcEnd::Stopped => false,
        PrecalcEnd::Refused(e) => {
            publish(shared, Err(e));
            false
        }
    }
}
