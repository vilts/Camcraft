//! The CPU march off the UI thread (M13.1 leftover): where no device
//! traces, one worker thread runs the sim's `preview` and publishes
//! the finished picture through a latest-only slot, the pattern the
//! playback worker (`app::stock_play_worker`) set. Provenance: the
//! plain latest-wins mailbox (a mutex-guarded job plus a condvar, a
//! single-item output slot) over std threads; nothing from vendor
//! binaries.
//!
//! ## The laws here
//!
//! - **The UI never waits on a march.** `request` overwrites the job
//!   the worker has not started (a camera that keeps moving coalesces
//!   to its newest pose) and `take` empties the slot without blocking.
//! - **A picture names what it was marched for.** Every answer carries
//!   its `Key`; the eye binds only a picture for the key it shows and
//!   throws a stale one away.
//! - **Refuse, never crash.** A refused march is published as `Err` by
//!   name; a poisoned lock reads as a lost worker. Dropping the handle
//!   stops and joins the thread.

use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use camcraft_app::stock_sim::{preview, Preview, PreviewCamera};
use camcraft_app::stock_view::FieldView;

use super::eye::Key;

/// One march to run: the field, the eye, the trace extent, the key
/// the picture answers to.
pub struct CpuJob {
    pub fv: Arc<FieldView>,
    pub cam: PreviewCamera,
    pub w: u32,
    pub h: u32,
    pub key: Key,
}

/// A finished march: the key and field it was asked for and the
/// picture, or the refusal by name.
pub struct CpuPicture {
    pub key: Key,
    /// The field marched (`None` only for a lost worker's refusal).
    pub fv: Option<Arc<FieldView>>,
    pub frame: Result<Preview, String>,
}

#[derive(Default)]
struct Req {
    job: Option<CpuJob>,
    /// What the worker is marching right now.
    running: Option<(Key, Arc<FieldView>)>,
    stop: bool,
}

#[derive(Default)]
struct Shared {
    req: Mutex<Req>,
    wake: Condvar,
    out: Mutex<Option<CpuPicture>>,
    published: Condvar,
}

/// The CPU eye's worker handle (see the module law).
pub struct CpuEye {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

fn lock<T>(m: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    m.lock().ok()
}

impl CpuEye {
    /// Start the worker; `Err` only when no thread spawns.
    pub fn spawn() -> Result<Self, String> {
        let shared = Arc::new(Shared::default());
        let inner = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("camcraft-cpu-eye".into())
            .spawn(move || run(&inner))
            .map_err(|e| format!("cpu eye: the worker would not start: {e}"))?;
        Ok(CpuEye { shared, handle: Some(handle) })
    }

    /// Ask for a march; the newest request wins over one not started.
    pub fn request(&self, job: CpuJob) {
        if let Some(mut g) = lock(&self.shared.req) {
            g.job = Some(job);
            self.shared.wake.notify_one();
        }
    }

    /// Whether a march of `fv` for `key` is queued or running: the eye
    /// asks before it requests, so a pending frame is not asked twice.
    #[must_use]
    pub fn in_flight(&self, key: Key, fv: &Arc<FieldView>) -> bool {
        let same = |k: Key, f: &Arc<FieldView>| k == key && Arc::ptr_eq(f, fv);
        lock(&self.shared.req).is_some_and(|g| {
            g.job.as_ref().is_some_and(|j| same(j.key, &j.fv)) || g.running.as_ref().is_some_and(|(k, f)| same(*k, f))
        })
    }

    /// The newest finished picture, never blocking. A lost worker
    /// answers `Err` by name under a `Key` nobody shows.
    pub fn take(&self) -> Option<CpuPicture> {
        match lock(&self.shared.out) {
            Some(mut g) => g.take().or_else(|| self.lost()),
            None => Some(lost_picture("cpu eye: the worker lost its output slot")),
        }
    }

    fn lost(&self) -> Option<CpuPicture> {
        self.handle.as_ref().filter(|h| h.is_finished()).map(|_| lost_picture("cpu eye: the worker stopped"))
    }

    /// Wait up to `timeout` for a picture to sit in the slot; tests
    /// only - the UI never waits. True when one is there.
    pub fn wait_picture(&self, timeout: Duration) -> bool {
        let Some(g) = lock(&self.shared.out) else { return false };
        match self.shared.published.wait_timeout_while(g, timeout, |s| s.is_none()) {
            Ok((g, _)) => g.is_some(),
            Err(_) => false,
        }
    }
}

impl Drop for CpuEye {
    fn drop(&mut self) {
        if let Some(mut g) = lock(&self.shared.req) {
            g.stop = true;
        }
        self.shared.wake.notify_all();
        if let Some(h) = self.handle.take()
            && h.join().is_err()
        {
            diag_warn!("cpu eye: the worker thread ended abnormally");
        }
    }
}

/// A lost worker's answer: an error under a key nobody shows, of no
/// field at all.
fn lost_picture(why: &str) -> CpuPicture {
    CpuPicture { key: Key::NONE, fv: None, frame: Err(why.into()) }
}

/// The next job, or `None` to stop; waits while there is none.
fn next_job(shared: &Shared) -> Option<CpuJob> {
    let mut g = lock(&shared.req)?;
    loop {
        if g.stop {
            return None;
        }
        if let Some(job) = g.job.take() {
            g.running = Some((job.key, Arc::clone(&job.fv)));
            return Some(job);
        }
        g = shared.wake.wait(g).ok()?;
    }
}

fn run(shared: &Shared) {
    while let Some(job) = next_job(shared) {
        let frame = match job.fv.stock.field() {
            Some(stock) => {
                let target = job.fv.target.as_ref().and_then(|t| t.field());
                preview(stock, target, &job.cam, job.w, job.h, job.fv.tol)
                    .map_err(|e| format!("the CPU march refused the frame: {e}"))
            }
            None => Err("the field view holds no field to march".into()),
        };
        if let Some(mut g) = lock(&shared.out) {
            *g = Some(CpuPicture { key: job.key, fv: Some(job.fv), frame });
        }
        if let Some(mut g) = lock(&shared.req) {
            g.running = None;
        }
        shared.published.notify_all();
    }
}
