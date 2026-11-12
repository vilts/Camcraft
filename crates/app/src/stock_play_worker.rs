//! Traced playback off the UI thread (2026-11 perf fix): one worker
//! owns the `FieldPlay`, cuts toward the newest requested move, and
//! publishes the newest view through a latest-only slot. Provenance:
//! the plain latest-wins mailbox pattern (a mutex-guarded target plus
//! a condvar, a single-item output slot) over std threads; nothing
//! from vendor binaries.
//!
//! ## The laws here
//!
//! - **The UI never waits on a cut.** `request` overwrites the target
//!   (coalescing every request the worker has not reached yet) and
//!   `take` empties the slot without blocking; whatever is newest is
//!   what shows.
//! - **Long jumps stay live.** The worker cuts in chunks sized to a
//!   frame's worth of work, re-reading the target between chunks, so
//!   a scrub turned around mid-jump is followed at once and a jump to
//!   the end publishes its progress as it goes.
//! - **A jump is batched (M13.1 leftover).** A target farther than the
//!   field's own batch (`JUMP_MOVES`) is cut checkpoint to checkpoint
//!   with the dirty ledger off (`FieldPlay::set_batched`): no per-move
//!   bookkeeping for a picture that travels whole anyway. The target
//!   is still re-read at every checkpoint, progress is published at
//!   most once a second (`JUMP_PUBLISH`), and the landing is published
//!   once; the ledger returns when the next play step arrives.
//! - **Publication is throttled.** A view is snapshotted (cut layers
//!   cloned, cut lattice packed) at most every `MIN_PUBLISH`, or when
//!   the worker catches up - the per-frame clone is bounded by time.
//! - **Only the touched bricks travel (M13.1).** After the session's
//!   first picture, a frame carries the cut bricks changed since the
//!   last frame the UI took (unioned over any it never took) as a
//!   patch; a checkpoint restore, a broken chain, or a patch as big as
//!   the layer sends the whole layer instead.
//! - **Refuse, never crash.** A build or cut failure is published as
//!   `Err` and the worker ends; a poisoned lock reads as a lost worker
//!   by name. Dropping the handle stops and joins the thread: no
//!   playback outlives its document.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use crate::clock::Instant;
use std::time::Duration;

use crate::stock_play::{FieldPlay, PlayIndex, PlaySeed};
use crate::stock_view::{CutLayer, FieldView, Packed, PackedBase};
use crate::stock_sim::preview::{pack_cut, pack_cut_patch, LatticeMeta};

/// The publication floor: at most ~20 views a second while cutting.
pub const MIN_PUBLISH: Duration = Duration::from_millis(50);

/// The chunk's time target: a chunk this long between target checks.
const CHUNK_TARGET: Duration = Duration::from_millis(15);

/// Chunk bounds, in moves (the upper one is the field's own batch).
const CHUNK_MIN: usize = 1;
const CHUNK_MAX: usize = 1024;

/// A target this far past the cut is a jump, not a play step (the
/// module law): the field's own batch, so a play can never trip it.
pub const JUMP_MOVES: usize = CHUNK_MAX;

/// The publication floor while jumping: progress once a second.
pub const JUMP_PUBLISH: Duration = Duration::from_secs(1);

/// One published picture: the view and the moves it is cut by.
#[derive(Clone, Debug)]
pub struct PlayFrame {
    pub at: usize,
    pub view: Arc<FieldView>,
}

#[derive(Default)]
struct Req {
    target: usize,
    stop: bool,
}

#[derive(Default)]
struct Slot {
    latest: Option<Result<PlayFrame, String>>,
    /// Publications so far (a test's progress counter).
    count: u64,
}

#[derive(Default)]
struct Shared {
    req: Mutex<Req>,
    wake: Condvar,
    out: Mutex<Slot>,
    published: Condvar,
}

/// The playback worker's handle (see the module law).
pub struct PlayWorker {
    shared: Arc<Shared>,
    index: PlayIndex,
    handle: Option<JoinHandle<()>>,
    /// Moves cut in the newest frame `take` handed out (`usize::MAX`:
    /// none yet).
    shown: AtomicUsize,
}

fn lock<T>(m: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    m.lock().ok()
}

impl PlayWorker {
    /// Start a worker over `seed` (the fields are built on the worker,
    /// so the call returns at once). `Err` only when no thread spawns.
    pub fn spawn(seed: PlaySeed) -> Result<Self, String> {
        let index = seed.index();
        let shared = Arc::new(Shared::default());
        let inner = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("camcraft-playback".into())
            .spawn(move || run(&inner, seed))
            .map_err(|e| format!("playback worker would not start: {e}"))?;
        Ok(PlayWorker { shared, index, handle: Some(handle), shown: AtomicUsize::new(usize::MAX) })
    }

    /// The move table the playhead maps onto.
    #[must_use]
    pub fn index(&self) -> &PlayIndex {
        &self.index
    }

    /// Ask for the stock cut to `k` moves; the newest request wins.
    pub fn request(&self, k: usize) {
        if let Some(mut g) = lock(&self.shared.req)
            && g.target != k
        {
            g.target = k;
            self.shared.wake.notify_one();
        }
    }

    /// The newest publication since the last take, never blocking.
    /// A lost worker (poisoned slot, or a thread that ended without a
    /// word) answers `Err` by name.
    pub fn take(&self) -> Option<Result<PlayFrame, String>> {
        match lock(&self.shared.out) {
            Some(mut g) => {
                let r = g.latest.take().or_else(|| self.lost());
                if let Some(Ok(f)) = &r {
                    self.shown.store(f.at, Ordering::Relaxed);
                }
                r
            }
            None => Some(Err("the playback worker lost its output slot".into())),
        }
    }

    fn lost(&self) -> Option<Result<PlayFrame, String>> {
        self.handle
            .as_ref()
            .filter(|h| h.is_finished())
            .map(|_| Err("the playback worker stopped".into()))
    }

    /// Wait up to `timeout` for a publication newer than `seen` (the
    /// count `published` last answered); tests and benches only - the
    /// UI never waits.
    pub fn wait_published(&self, seen: u64, timeout: Duration) -> u64 {
        let Some(g) = lock(&self.shared.out) else { return seen };
        match self.shared.published.wait_timeout_while(g, timeout, |s| s.count <= seen) {
            Ok((g, _)) => g.count,
            Err(_) => seen,
        }
    }

    /// Moves cut in the newest frame `take` handed out.
    #[must_use]
    pub fn shown_at(&self) -> Option<usize> {
        Some(self.shown.load(Ordering::Relaxed)).filter(|&a| a != usize::MAX)
    }

    /// Whether the UI should keep polling for a requested or published frame.
    /// A paused playhead at its displayed target needs no redraw timer.
    #[must_use]
    pub fn needs_poll(&self) -> bool {
        let target = lock(&self.shared.req).map(|g| g.target.min(self.index.total()));
        if target.is_none() || self.shown_at() != target {
            return true;
        }
        lock(&self.shared.out).is_none_or(|g| g.latest.is_some())
    }

    /// Publications so far.
    #[must_use]
    pub fn published(&self) -> u64 {
        lock(&self.shared.out).map_or(0, |g| g.count)
    }
}

impl Drop for PlayWorker {
    fn drop(&mut self) {
        if let Some(mut g) = lock(&self.shared.req) {
            g.stop = true;
        }
        self.shared.wake.notify_all();
        if let Some(h) = self.handle.take()
            && h.join().is_err()
        {
            tracing::error!("playback worker: the thread ended abnormally");
        }
    }
}

fn publish(shared: &Shared, r: Result<PlayFrame, String>) {
    if let Some(mut g) = lock(&shared.out) {
        g.latest = Some(r);
        g.count = g.count.saturating_add(1);
    }
    shared.published.notify_all();
}

/// The next target, or `None` to stop; waits while there is nothing
/// to do (the cut stands at the target and that state was published).
fn next_target(shared: &Shared, at: usize, published: Option<usize>, total: usize) -> Option<usize> {
    let mut g = lock(&shared.req)?;
    loop {
        if g.stop {
            return None;
        }
        let k = g.target.min(total);
        if k != at || published != Some(at) {
            return Some(k);
        }
        g = shared.wake.wait(g).ok()?;
    }
}

/// Cells in a brick (the field's 8^3): the patch-size yardstick.
const BRICK_TEXELS: usize = 512;

/// What the worker last published: the patch base of the next frame.
#[derive(Default)]
struct Chain {
    seq: u64,
    last: Option<Last>,
}

enum Last {
    Full,
    Patch { from: u64, set: HashSet<[i32; 3]> },
}

fn texels(meta: &LatticeMeta) -> usize {
    meta.dims.iter().fold(1usize, |n, &d| n.saturating_mul(d as usize))
}

/// This frame's cut layer relation: `Some((from, bricks))` for a patch
/// over frame `from`, `None` for the whole layer (the module law).
fn delta(
    dirty: Option<HashSet<[i32; 3]>>,
    last: Option<Last>,
    untaken: bool,
    seq: u64,
) -> Option<(u64, HashSet<[i32; 3]>)> {
    match (dirty?, last?) {
        // The frame before still sits untaken and this one replaces
        // it: the patch must also carry that frame's bricks.
        (d, Last::Patch { from, mut set }) if untaken => {
            set.extend(d);
            Some((from, set))
        }
        (_, Last::Full) if untaken => None,
        (d, _) => Some((seq.saturating_sub(1), d)),
    }
}

fn snapshot(shared: &Shared, play: &mut FieldPlay, base: Option<&Arc<PackedBase>>, chain: &mut Chain) -> PlayFrame {
    let dirty = play.take_dirty();
    let mut view = play.view();
    if let (Some(base), Some(f)) = (base, view.stock.field()) {
        chain.seq = chain.seq.saturating_add(1);
        let seq = chain.seq;
        let untaken = lock(&shared.out).is_some_and(|g| matches!(g.latest, Some(Ok(_))));
        let d = delta(dirty, chain.last.take(), untaken, seq)
            .filter(|(_, set)| set.len().saturating_mul(BRICK_TEXELS) < texels(&base.meta));
        let (cut, last) = match d {
            Some((from, set)) => {
                let patch = Arc::new(pack_cut_patch(f, &base.meta, &set));
                (CutLayer::Patch { from, patch }, Last::Patch { from, set })
            }
            None => (CutLayer::Full(Arc::new(pack_cut(f, &base.meta))), Last::Full),
        };
        chain.last = Some(last);
        view.packed = Some(Packed { base: Arc::clone(base), seq, cut });
    }
    PlayFrame { at: play.at(), view: Arc::new(view) }
}

fn run(shared: &Shared, seed: PlaySeed) {
    let mut play = match FieldPlay::from_seed(seed) {
        Ok(p) => p,
        Err(e) => return publish(shared, Err(e)),
    };
    let base = {
        let v = play.view();
        PackedBase::of(&v.stock, v.target.as_deref()).map(|(b, _)| Arc::new(b))
    };
    let (mut published, mut last, mut chunk) = (None, None::<Instant>, 64usize);
    let mut chain = Chain::default();
    while let Some(k) = next_target(shared, play.at(), published, play.total()) {
        // A jump (the module law) cuts checkpoint to checkpoint with
        // the ledger off, and stays one until it lands (the last stride
        // is not a play step); a play step cuts a frame's worth of moves.
        let jump = k > play.at() && (play.batched() || k > play.at().saturating_add(JUMP_MOVES));
        play.set_batched(jump);
        // A backward target restores its checkpoint first (no cutting),
        // then walks forward in chunks like any other.
        let step = if k < play.at() {
            play.floor_checkpoint(k)
        } else if jump {
            k.min(play.next_boundary())
        } else {
            k.min(play.at().saturating_add(chunk))
        };
        let t0 = Instant::now();
        if let Err(e) = play.seek(step) {
            return publish(shared, Err(e));
        }
        // A stride's time says nothing about a chunk's: the chunk
        // adapts on play steps only.
        let dt = t0.elapsed();
        if !jump && dt < CHUNK_TARGET / 2 {
            chunk = chunk.saturating_mul(2).min(CHUNK_MAX);
        } else if !jump && dt > CHUNK_TARGET * 2 {
            chunk = (chunk / 2).max(CHUNK_MIN);
        }
        let floor = if jump { JUMP_PUBLISH } else { MIN_PUBLISH };
        let due = last.is_none_or(|t| t.elapsed() >= floor);
        let landed = play.at() == k;
        if landed {
            // The landing picture is the whole upload the next play
            // step's patches are relative to: the ledger restarts here.
            play.set_batched(false);
        }
        if published != Some(play.at()) && (landed || due) {
            publish(shared, Ok(snapshot(shared, &mut play, base.as_ref(), &mut chain)));
            published = Some(play.at());
            last = Some(Instant::now());
        }
    }
}
