//! Windowless wgpu device for the headless run and the GPU test suites.
//! The adapter policy moved down to `camcraft-gpu` (#2620), so the compute
//! seam and the renderer take the same device; this module keeps the old
//! paths. Provenance: re-exports of our own `camcraft_gpu` adapter policy.
//!
//! For tests it also keeps the GPU of the process (#3605): one lock, so
//! that one test at a time uses the GPU, and one shared device, made once
//! and never dropped. Parallel test threads that made and dropped their
//! own devices ended the process with 0xC0000005 on a hybrid-GPU Windows
//! box (#3605, and the lib tests in #3589).

#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

#[cfg(not(target_arch = "wasm32"))]
use egui_wgpu::wgpu;

pub use camcraft_gpu::{device_async, AdapterKind, HeadlessGpu, REQUIRE_GPU_ENV};
#[cfg(not(target_arch = "wasm32"))]
pub use camcraft_gpu::{device, device_for_test};

#[cfg(not(target_arch = "wasm32"))]
static GPU_TEST_LOCK: Mutex<()> = Mutex::new(());
/// Guards alive now. The lock keeps it at 0 or 1.
#[cfg(not(target_arch = "wasm32"))]
static HOLDERS: AtomicUsize = AtomicUsize::new(0);
/// The highest count of [`HOLDERS`] this process saw.
#[cfg(not(target_arch = "wasm32"))]
static MAX_HOLDERS: AtomicUsize = AtomicUsize::new(0);

/// Holds the GPU of the process for one test. Declare it before the
/// first value that can make or use a device, so that it drops last.
#[cfg(not(target_arch = "wasm32"))]
pub struct GpuGuard {
    _lock: MutexGuard<'static, ()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for GpuGuard {
    fn drop(&mut self) {
        // Runs before `_lock` is released (fields drop after `drop`), and
        // on a panic's unwind too, so a failed test does not leave a count.
        HOLDERS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Wait for the GPU, then hold it until the guard drops. A test that
/// panicked with the lock held poisons it; the next test takes it anyway,
/// so one failure does not cascade into the other GPU tests.
#[cfg(not(target_arch = "wasm32"))]
pub fn gpu_guard() -> GpuGuard {
    let lock = GPU_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = HOLDERS.fetch_add(1, Ordering::SeqCst).saturating_add(1);
    MAX_HOLDERS.fetch_max(now, Ordering::SeqCst);
    GpuGuard { _lock: lock }
}

/// `(guards alive now, the most guards alive at one time)`, for the pins.
#[cfg(not(target_arch = "wasm32"))]
pub fn gpu_holders() -> (usize, usize) {
    (HOLDERS.load(Ordering::SeqCst), MAX_HOLDERS.load(Ordering::SeqCst))
}

/// The shared test device and the lock that a test holds while it uses it.
/// Field order is drop order: the `Arc`s first, then the guard.
#[cfg(not(target_arch = "wasm32"))]
pub struct TestGpu {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    _guard: GpuGuard,
}

#[cfg(not(target_arch = "wasm32"))]
type Shared = Result<Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>)>, String>;

/// For GPU tests: take the lock, then the one device of the process (made
/// on the first call, by [`device_for_test`] and its SKIP policy, and never
/// dropped). `Ok(None)` is a loud SKIP; `Err` is `CAMCRAFT_REQUIRE_GPU=1`
/// with no adapter. `suite` names the first caller in the log line.
/// It takes [`gpu_guard`] itself: never call it while a guard is held on
/// the same thread (the lock is not re-entrant).
#[cfg(not(target_arch = "wasm32"))]
pub fn test_gpu(suite: &str) -> Result<Option<TestGpu>, String> {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    let guard = gpu_guard();
    let shared = SHARED.get_or_init(|| {
        device_for_test(suite).map(|gpu| gpu.map(|g| (Arc::new(g.device), Arc::new(g.queue))))
    });
    match shared {
        Ok(Some((device, queue))) => {
            Ok(Some(TestGpu { device: Arc::clone(device), queue: Arc::clone(queue), _guard: guard }))
        }
        Ok(None) => {
            eprintln!("*** {suite}: SKIPPED (no adapter) - NOT a pass ***");
            Ok(None)
        }
        Err(err) => Err(err.clone()),
    }
}
