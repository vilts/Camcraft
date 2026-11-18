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

/// The CPU eye's worker handle (see the module law). Where OS threads are
/// unavailable (`wasm32` or `ErrorKind::Unsupported`), it runs inline on
/// the next `take` / `wait_picture` instead of refusing the fallback.
pub struct CpuEye {
    mode: Mode,
}

#[derive(Default)]
struct Inline {
    pending: Mutex<Option<CpuJob>>,
    out: Mutex<Option<CpuPicture>>,
}

enum Mode {
    Threaded {
        shared: Arc<Shared>,
        handle: Option<JoinHandle<()>>,
    },
    Inline(Box<Inline>),
}

fn lock<T>(m: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    m.lock().ok()
}

impl CpuEye {
    /// Start the worker; falls back to inline marching when threads are
    /// unsupported (`wasm32`, or `ErrorKind::Unsupported`).
    pub fn spawn() -> Result<Self, String> {
        #[cfg(target_arch = "wasm32")]
        {
            return Ok(Self::inline());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let shared = Arc::new(Shared::default());
            let inner = Arc::clone(&shared);
            match std::thread::Builder::new()
                .name("camcraft-cpu-eye".into())
                .spawn(move || run(&inner))
            {
                Ok(handle) => Ok(CpuEye {
                    mode: Mode::Threaded { shared, handle: Some(handle) },
                }),
                Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {
                    diag_warn!("cpu eye: threads unsupported ({e}); marching inline");
                    Ok(Self::inline())
                }
                Err(e) => Err(format!("cpu eye: the worker would not start: {e}")),
            }
        }
    }

    /// A thread-free CPU eye that marches the newest job on `take` or
    /// `wait_picture` (#31).
    #[must_use]
    pub fn inline() -> Self {
        CpuEye {
            mode: Mode::Inline(Box::default()),
        }
    }

    /// Ask for a march; the newest request wins over one not started.
    pub fn request(&self, job: CpuJob) {
        match &self.mode {
            Mode::Threaded { shared, .. } => {
                if let Some(mut g) = lock(&shared.req) {
                    g.job = Some(job);
                    shared.wake.notify_one();
                }
            }
            Mode::Inline(inline) => {
                if let Some(mut g) = lock(&inline.pending) {
                    *g = Some(job);
                }
            }
        }
    }

    /// Whether a march of `fv` for `key` is queued or running: the eye
    /// asks before it requests, so a pending frame is not asked twice.
    #[must_use]
    pub fn in_flight(&self, key: Key, fv: &Arc<FieldView>) -> bool {
        let same = |k: Key, f: &Arc<FieldView>| k == key && Arc::ptr_eq(f, fv);
        match &self.mode {
            Mode::Threaded { shared, .. } => lock(&shared.req).is_some_and(|g| {
                g.job.as_ref().is_some_and(|j| same(j.key, &j.fv))
                    || g.running.as_ref().is_some_and(|(k, f)| same(*k, f))
            }),
            Mode::Inline(inline) => {
                lock(&inline.pending).is_some_and(|g| g.as_ref().is_some_and(|j| same(j.key, &j.fv)))
            }
        }
    }

    /// Whether a march or finished picture is waiting to be drawn.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        match &self.mode {
            Mode::Threaded { shared, .. } => {
                lock(&shared.req).is_some_and(|g| g.job.is_some() || g.running.is_some())
                    || lock(&shared.out).is_some_and(|g| g.is_some())
            }
            Mode::Inline(inline) => {
                lock(&inline.pending).is_some_and(|g| g.is_some())
                    || lock(&inline.out).is_some_and(|g| g.is_some())
            }
        }
    }

    /// The newest finished picture, never blocking on a worker thread.
    /// In inline mode, marches the pending job here. A lost worker
    /// answers `Err` by name under a `Key` nobody shows.
    pub fn take(&self) -> Option<CpuPicture> {
        match &self.mode {
            Mode::Threaded { shared, handle } => match lock(&shared.out) {
                Some(mut g) => g.take().or_else(|| lost(handle.as_ref())),
                None => Some(lost_picture("cpu eye: the worker lost its output slot")),
            },
            Mode::Inline(inline) => {
                if let Some(pic) = lock(&inline.out).and_then(|mut g| g.take()) {
                    return Some(pic);
                }
                let job = lock(&inline.pending).and_then(|mut g| g.take())?;
                Some(march_job(job))
            }
        }
    }

    /// Wait up to `timeout` for a picture to sit in the slot; tests
    /// only - the UI never waits. True when one is there.
    pub fn wait_picture(&self, timeout: Duration) -> bool {
        match &self.mode {
            Mode::Threaded { shared, .. } => {
                let Some(g) = lock(&shared.out) else { return false };
                match shared.published.wait_timeout_while(g, timeout, |s| s.is_none()) {
                    Ok((g, _)) => g.is_some(),
                    Err(_) => false,
                }
            }
            Mode::Inline(inline) => {
                if lock(&inline.out).is_some_and(|g| g.is_some()) {
                    return true;
                }
                let Some(job) = lock(&inline.pending).and_then(|mut g| g.take()) else {
                    return false;
                };
                let pic = march_job(job);
                let Some(mut g) = lock(&inline.out) else { return false };
                *g = Some(pic);
                true
            }
        }
    }
}

impl Drop for CpuEye {
    fn drop(&mut self) {
        if let Mode::Threaded { shared, handle } = &mut self.mode {
            if let Some(mut g) = lock(&shared.req) {
                g.stop = true;
            }
            shared.wake.notify_all();
            if let Some(h) = handle.take()
                && h.join().is_err()
            {
                diag_warn!("cpu eye: the worker thread ended abnormally");
            }
        }
    }
}

fn lost(handle: Option<&JoinHandle<()>>) -> Option<CpuPicture> {
    handle
        .filter(|h| h.is_finished())
        .map(|_| lost_picture("cpu eye: the worker stopped"))
}

/// A lost worker's answer: an error under a key nobody shows, of no
/// field at all.
fn lost_picture(why: &str) -> CpuPicture {
    CpuPicture { key: Key::NONE, fv: None, frame: Err(why.into()) }
}

/// March one job on the caller's thread; shared by the worker and inline mode.
fn march_job(job: CpuJob) -> CpuPicture {
    let frame = match job.fv.stock.field() {
        Some(stock) => {
            let target = job.fv.target.as_ref().and_then(|t| t.field());
            preview(stock, target, &job.cam, job.w, job.h, job.fv.tol)
                .map_err(|e| format!("the CPU march refused the frame: {e}"))
        }
        None => Err("the field view holds no field to march".into()),
    };
    CpuPicture { key: job.key, fv: Some(job.fv), frame }
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
        let pic = march_job(job);
        if let Some(mut g) = lock(&shared.out) {
            *g = Some(pic);
        }
        if let Some(mut g) = lock(&shared.req) {
            g.running = None;
        }
        shared.published.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camcraft_app::geometry::{cube, Point3};
    use camcraft_app::stock_sim::{Backend, Stock, DEV_TOL, FIELD_BAND};

    fn job() -> CpuJob {
        let f = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 5.0));
        let stock = Stock::from_facets(Backend::Field { band: FIELD_BAND }, &f.facets, 0.5).expect("stock");
        let fv = Arc::new(FieldView {
            stock,
            target: None,
            target_note: None,
            cell: 0.5,
            tol: DEV_TOL,
            packed: None,
        });
        let cam = PreviewCamera::look_at(
            Point3::new(-15.0, -15.0, 20.0),
            Point3::new(5.0, 5.0, 2.5),
            Point3::new(0.0, 0.0, 1.0),
            0.5,
        )
        .expect("cam");
        CpuJob { fv, cam, w: 16, h: 12, key: Key::NONE }
    }

    #[test]
    fn inline_cpu_eye_marches_without_spawning_a_thread() {
        let eye = CpuEye::inline();
        let j = job();
        let fv = Arc::clone(&j.fv);
        assert!(!eye.is_pending());
        eye.request(j);
        assert!(eye.is_pending());
        assert!(eye.in_flight(Key::NONE, &fv));
        let pic = eye.take().expect("inline march returns a picture on take");
        assert!(!eye.is_pending());
        assert!(pic.frame.is_ok(), "inline march succeeds: {:?}", pic.frame.err());
        let j2 = job();
        eye.request(j2);
        assert!(eye.wait_picture(Duration::from_millis(1)));
        assert!(eye.take().is_some());
    }
}
