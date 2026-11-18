//! The calculation before playback (#2181): cut the whole program once,
//! stride by stride, keeping keyframes (`play_keys`), with a stop, a
//! cancel and a progress report. Provenance: a plain cooperative work
//! loop over our own playback cut; nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Display only.** The loop drives the playback picture's cut and
//!   nothing else; the verdict and the post gate never read it.
//! - **Every end is said by name.** Each stride reports `Running`; the
//!   end reports `Ready`, or `Incomplete` with "cancelled", "stopped"
//!   or the cut's own refusal. No end is silent.
//! - **A cancel keeps playback; a stop or a refusal ends it.** After a
//!   cancel the keyframes made so far stay valid and the rest is cut
//!   live; a stop is the owner going away; a refusal is published.
//! - **The orchestration is here, the cut is the owner's.** `begin`
//!   and `step` set the stride, restart the store and offer each
//!   boundary; a `Playback` only cuts, rewinds and lends its store
//!   (the #2192 review: no store or stride logic in the document crate).

use crate::play_keys::{KeyCost, Keyframes, Precalc, MAX_KEYFRAMES};

/// A playback the calculation cuts: its moves, its playhead, its
/// keyframe store and its cut. It holds no calculation logic.
pub trait Playback {
    /// The value a keyframe holds (the cut stock).
    type Value: KeyCost + Clone;
    /// Moves in the whole program.
    fn total(&self) -> usize;
    /// Moves cut so far (the playhead).
    fn at(&self) -> usize;
    /// The keyframe store (read only).
    fn keys(&self) -> &Keyframes<Self::Value>;
    /// The store and the current value together (an offer's borrow).
    fn keys_and_value(&mut self) -> (&mut Keyframes<Self::Value>, &Self::Value);
    /// Stand at move 0 with `uncut`; the picture ledger goes off.
    fn rewind(&mut self, uncut: Self::Value);
    /// Cut from the playhead to move `to` and stand there.
    fn cut_to(&mut self, to: usize) -> Result<(), String>;
}

/// Start the calculation: back to the uncut value, keyframes every
/// `total / keys` moves (at least `min_stride`) within `budget` bytes
/// from here on.
pub fn begin<P: Playback + ?Sized>(play: &mut P, keys: usize, min_stride: usize, budget: usize) -> Result<(), String> {
    let uncut = play.keys().floor(0).ok_or("playback lost its uncut stock")?.1.clone();
    let stride = play.total().div_ceil(keys.max(1)).max(min_stride.max(1));
    play.keys_and_value().0.restart(stride, keys.max(MAX_KEYFRAMES), budget);
    play.rewind(uncut);
    Ok(())
}

/// Cut one stride and offer its keyframe to the store (its budget
/// thins it); `Ok(true)` when the whole program is calculated.
pub fn step<P: Playback + ?Sized>(play: &mut P) -> Result<bool, String> {
    let (at, total) = (play.at(), play.total());
    if at >= total {
        return Ok(true);
    }
    let to = play.keys().next_boundary(at, total);
    play.cut_to(to)?;
    let (keys, value) = play.keys_and_value();
    keys.offer(to, value);
    Ok(play.at() >= total)
}

/// A playback with the calculation's settings: the `PrecalcCut` that
/// `drive` runs.
pub struct Calc<'a, P: ?Sized> {
    pub play: &'a mut P,
    /// Keyframes asked for (the stride is the program over this).
    pub keys: usize,
    /// The smallest stride, in moves.
    pub min_stride: usize,
    /// The store's byte budget.
    pub budget: usize,
}

impl<P: Playback + ?Sized> PrecalcCut for Calc<'_, P> {
    fn total(&self) -> usize {
        self.play.total()
    }

    fn done(&self) -> usize {
        self.play.at()
    }

    fn begin(&mut self) -> Result<(), String> {
        begin(self.play, self.keys, self.min_stride, self.budget)
    }

    fn step(&mut self) -> Result<bool, String> {
        step(self.play)
    }

    fn ready(&self, secs: f64) -> Precalc {
        let k = self.play.keys();
        Precalc::Ready { keys: k.len(), bytes: k.bytes(), stride: k.stride(), secs }
    }
}

/// The playback cut, as the calculation drives it.
pub trait PrecalcCut {
    /// Moves in the whole program.
    fn total(&self) -> usize;
    /// Moves the calculation has cut so far.
    fn done(&self) -> usize;
    /// Back to the uncut stock, keyframes and budget set.
    fn begin(&mut self) -> Result<(), String>;
    /// Cut one stride and offer its keyframe; `Ok(true)` at the end.
    fn step(&mut self) -> Result<bool, String>;
    /// The finished store's reading, `secs` after the start.
    fn ready(&self, secs: f64) -> Precalc;
}

/// How the calculation ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrecalcEnd {
    /// Ready, or cancelled: playback goes on.
    Play,
    /// The owner stopped: playback ends, nothing to publish.
    Stopped,
    /// The cut refused: playback ends and the refusal is published.
    Refused(String),
}

/// The owner's side of the loop: what it asks and where it hears.
pub trait PrecalcOwner {
    /// The owner went away (the worker is dropped).
    fn stopped(&self) -> bool;
    /// The user cancelled the calculation.
    fn cancelled(&self) -> bool;
    /// Seconds since the start (the owner holds the clock).
    fn elapsed(&self) -> f64;
    /// The calculation's state, after each change.
    fn report(&self, p: Precalc);
}

/// Run the calculation to an end (the module law).
pub fn drive(cut: &mut impl PrecalcCut, owner: &impl PrecalcOwner) -> PrecalcEnd {
    let total = cut.total();
    if let Err(e) = cut.begin() {
        owner.report(Precalc::Incomplete { done: 0, total, why: e.clone() });
        return PrecalcEnd::Refused(e);
    }
    loop {
        if owner.stopped() {
            owner.report(Precalc::Incomplete { done: cut.done(), total, why: "stopped".into() });
            return PrecalcEnd::Stopped;
        }
        if owner.cancelled() {
            owner.report(Precalc::Incomplete { done: cut.done(), total, why: "cancelled".into() });
            return PrecalcEnd::Play;
        }
        match cut.step() {
            Ok(true) => {
                owner.report(cut.ready(owner.elapsed()));
                return PrecalcEnd::Play;
            }
            Ok(false) => owner.report(Precalc::Running { done: cut.done(), total }),
            Err(e) => {
                owner.report(Precalc::Incomplete { done: cut.done(), total, why: e.clone() });
                return PrecalcEnd::Refused(e);
            }
        }
    }
}
