//! The evaluation meters of one STEP tessellation pass: one per face and
//! per edge curve (#1536, #1545), and the pass-wide budget they share
//! (#1599), so a file of many runaway faces stops after a few of them.
//!
//! Provenance: our own design (a work budget on a parametric
//! subdivision); nothing from any CAM vendor.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use super::step_nearest::Fault;
use super::ImportError;

/// Faces and edges of one pass that may run out of evaluations before the
/// whole pass stops (#1599).
///
/// Each runaway face spends a whole cap (`step_guard::FACE_EVAL_CAP`, 10-15
/// s) before it trips, so without a pass-wide limit a hostile file of N
/// runaway faces costs N caps divided by the core count. After this many
/// trips every meter of the pass answers its fixed point, every face in
/// flight ends within one sweep, and the pass is refused. A pass then
/// spends at most this many caps plus less than one cap for each worker
/// thread, whatever the face count. Three keeps every pass with one or two
/// trips exactly as before: #1568's plane beside a jump (two trips) is
/// still judged face by face.
pub const PASS_TRIPS: u64 = 3;

/// The budget all meters of one pass share: how many have tripped, and
/// whether that stopped the pass.
#[derive(Debug, Default)]
pub(super) struct Pass {
    trips: AtomicU64,
    stopped: AtomicBool,
}

impl Pass {
    /// One more meter ran out (or was refused); the trip that reaches
    /// [`PASS_TRIPS`] stops the pass.
    fn trip(&self) {
        if self.trips.fetch_add(1, Ordering::Relaxed).saturating_add(1) >= PASS_TRIPS {
            self.stopped.store(true, Ordering::Relaxed);
        }
    }
    /// The pass ran out of trips: no meter evaluates anything more.
    pub(super) fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }
    /// A new pass: no trips.
    pub(super) fn reset(&self) {
        self.trips.store(0, Ordering::Relaxed);
        self.stopped.store(false, Ordering::Relaxed);
    }
}

/// The refusal of a pass that the trip limit stopped says so (#1599): the
/// named face is one of several, and the rest of the model was not judged.
pub(super) fn noted(stopped: bool, refusal: ImportError) -> ImportError {
    match refusal {
        ImportError::Step(why) if stopped => ImportError::Step(format!(
            "{why} (the tessellation stopped after {PASS_TRIPS} faces or edges ran out of evaluations; more of the model may be broken)"
        )),
        other => other,
    }
}

/// One face's (or edge curve's) evaluation count for the current pass.
#[derive(Debug)]
pub(super) struct Meter {
    pub(super) used: AtomicU64,
    cap: u64,
    /// Some pass of this import divided the surface over a range outside
    /// its own parameter range (#1568: sc_logo's face #836 was asked for u
    /// from -2e51, a trimming bound gone wild), where its continuity is not
    /// proven - a rational surface extrapolated can meet a pole. Sticky
    /// across passes: truck derives the range from each pass's tolerance,
    /// so the coarser pass whose mesh a fallback would keep may be the one
    /// that went wild while the finer pass that tripped stayed in range.
    pub(super) outside: AtomicBool,
    /// A boundary search of this pass that could not be answered soundly
    /// (`step_nearest::Fault`, 0 for none): the face is refused by name.
    fault: AtomicU8,
    /// The pass budget this meter shares with the rest of its shell.
    pass: Arc<Pass>,
    /// This meter's trip is already counted in `pass` for this pass.
    counted: AtomicBool,
}

impl Meter {
    /// A meter with a pass of its own.
    #[cfg(test)]
    pub(super) fn new(cap: u64) -> Self {
        Self::in_pass(cap, Arc::default())
    }
    /// A meter that shares `pass` with the other meters of its shell.
    pub(super) fn in_pass(cap: u64, pass: Arc<Pass>) -> Self {
        Self {
            used: AtomicU64::new(0),
            cap,
            outside: AtomicBool::new(false),
            fault: AtomicU8::new(0),
            pass,
            counted: AtomicBool::new(false),
        }
    }
    /// Count this meter's refusal in the pass, once.
    fn count_trip(&self) {
        if !self.counted.swap(true, Ordering::Relaxed) {
            self.pass.trip();
        }
    }
    /// Count one evaluation; `false` once the cap is spent or the pass
    /// stopped (#1599: a stopped pass counts nothing more).
    pub(super) fn spend(&self) -> bool {
        if self.pass.stopped() {
            return false;
        }
        let ok = self.used.fetch_add(1, Ordering::Relaxed) < self.cap;
        if !ok {
            self.count_trip();
        }
        ok
    }
    /// Count `n` evaluations done outside `subs` (a boundary search's own
    /// work, #1602); `false` once the cap is spent or the pass stopped.
    /// Saturating, never wraps.
    pub(super) fn spend_many(&self, n: u64) -> bool {
        if self.pass.stopped() {
            return false;
        }
        let mut used = self.used.load(Ordering::Relaxed);
        loop {
            let next = used.saturating_add(n);
            match self.used.compare_exchange_weak(used, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) if next <= self.cap => return true,
                Ok(_) => {
                    self.count_trip();
                    return false;
                }
                Err(now) => used = now,
            }
        }
    }
    pub(super) fn tripped(&self) -> bool {
        self.used.load(Ordering::Relaxed) > self.cap
    }
    /// Refuse the face for a boundary search fault. The first one stands.
    /// A search on a face the stopped pass cut short sees only the fixed
    /// point (#1599); its fault is not the face's own and is not recorded.
    pub(super) fn flag(&self, fault: Fault) {
        if self.pass.stopped() && !self.refused() {
            return;
        }
        let _ = self.fault.compare_exchange(0, fault as u8, Ordering::Relaxed, Ordering::Relaxed);
        self.count_trip();
    }
    pub(super) fn fault(&self) -> Option<Fault> {
        Fault::from_code(self.fault.load(Ordering::Relaxed))
    }
    /// The pass is refused for this face: out of evaluations, or a fault.
    pub(super) fn refused(&self) -> bool {
        self.tripped() || self.fault().is_some()
    }
    /// Spend the whole budget at once: the face's division did not
    /// converge (#1602), so the pass is refused as for a runaway. A
    /// division cut short because the pass stopped is not a runaway of its
    /// own (#1599) and spends nothing.
    pub(super) fn exhaust(&self) {
        if self.pass.stopped() && !self.tripped() {
            return;
        }
        self.used.fetch_max(self.cap.saturating_add(1), Ordering::Relaxed);
        self.count_trip();
    }
    /// A new pass: the whole budget again. `outside` stays set. The shared
    /// [`Pass`] is reset by its owner (`step_guard::Guarded::reset`).
    pub(super) fn reset(&self) {
        self.used.store(0, Ordering::Relaxed);
        self.fault.store(0, Ordering::Relaxed);
        self.counted.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
#[path = "step_pass_tests.rs"]
mod tests;
