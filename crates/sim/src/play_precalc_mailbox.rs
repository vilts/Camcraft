//! The calculation's mailbox (#2181, #2192 review): where it stands,
//! read by the UI, and the UI's cancel, shared between the playback
//! thread and its handle. Provenance: a plain mutex slot and an atomic
//! flag; nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **A lost state reads incomplete, by name.** A poisoned slot never
//!   reads as `Off` or `Ready`: it is `Incomplete` with the reason.
//! - **The owner binds the stop and the clock.** The mailbox knows no
//!   thread and no clock; `MailboxOwner` takes them as closures, so the
//!   same law runs under the app's worker and under a test.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::play_keys::Precalc;
use crate::play_precalc::PrecalcOwner;

/// The calculation's state and the cancel flag.
#[derive(Debug, Default)]
pub struct Mailbox {
    state: Mutex<Option<Precalc>>,
    cancel: AtomicBool,
}

impl Mailbox {
    /// Where the calculation stands: `Off` when none was asked; a lost
    /// state reads `Incomplete` of `total` by name.
    #[must_use]
    pub fn read(&self, total: usize) -> Precalc {
        match self.state.lock() {
            Ok(g) => g.clone().unwrap_or(Precalc::Off),
            Err(_) => Precalc::Incomplete { done: 0, total, why: "the calculation lost its state".into() },
        }
    }

    /// Set the state (a lost slot keeps reading incomplete).
    pub fn set(&self, p: Precalc) {
        if let Ok(mut g) = self.state.lock() {
            *g = Some(p);
        }
    }

    /// Ask the calculation to stop where it stands.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// The user cancelled.
    #[must_use]
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// The mailbox as the loop's owner, with the caller's stop and clock.
pub struct MailboxOwner<'a, S, C> {
    pub mailbox: &'a Mailbox,
    /// `true` when the owner went away.
    pub stopped: S,
    /// Seconds since the start.
    pub elapsed: C,
}

impl<S: Fn() -> bool, C: Fn() -> f64> PrecalcOwner for MailboxOwner<'_, S, C> {
    fn stopped(&self) -> bool {
        (self.stopped)()
    }

    fn cancelled(&self) -> bool {
        self.mailbox.cancelled()
    }

    fn elapsed(&self) -> f64 {
        (self.elapsed)()
    }

    fn report(&self, p: Precalc) {
        self.mailbox.set(p);
    }
}
