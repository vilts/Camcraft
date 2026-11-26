//! Windowless wgpu device for the headless run, the compute seam and the
//! GPU test suites: the software adapter first (deterministic on CI:
//! lavapipe / WARP), then the platform's hardware adapter (Metal has no
//! software fallback, so on macOS this is the only way to render at all -
//! #615). Moved down from camcraft-ui's `headless_gpu` (#2620), unchanged
//! but for the compute probe that each device now carries.
//! Provenance: our own adapter policy over wgpu's public
//! `RequestAdapterOptions` API.

use crate::probe::{probe, Compute};

/// Which adapter the device came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    /// wgpu's fallback (software rasterizer) adapter.
    Software,
    /// The platform default hardware adapter.
    Hardware,
}

impl AdapterKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Software => "software",
            Self::Hardware => "hardware",
        }
    }
}

/// A device with no surface, and where it came from.
pub struct HeadlessGpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub kind: AdapterKind,
    /// `"<adapter name> via <backend>"`, for logs.
    pub name: String,
    /// What the device can compute, or why it cannot (#2620): read from
    /// the adapter's downlevel flags and the device's own limits.
    pub compute: Result<Compute, String>,
}

/// The env var that turns a test's "no adapter" skip into a failure.
pub const REQUIRE_GPU_ENV: &str = "CAMCRAFT_REQUIRE_GPU";

async fn try_kind(instance: &wgpu::Instance, kind: AdapterKind) -> Result<HeadlessGpu, String> {
    let options = wgpu::RequestAdapterOptions {
        force_fallback_adapter: kind == AdapterKind::Software,
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        ..Default::default()
    };
    let adapter = instance
        .request_adapter(&options)
        .await
        .map_err(|err| format!("{} adapter: {err}", kind.label()))?;
    let info = adapter.get_info();
    let name = format!("\"{}\" via {:?}", info.name, info.backend);
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .map_err(|err| format!("{} device {name}: {err}", kind.label()))?;
    let compute = probe(info.backend, &adapter.get_downlevel_capabilities(), &device.limits());
    Ok(HeadlessGpu { device, queue, kind, name, compute })
}

/// Software adapter first, then hardware. The error names why each failed.
/// Native only: it blocks on the adapter, which a browser cannot (#31) -
/// `device_async` is the same choice, awaited.
#[cfg(not(target_arch = "wasm32"))]
pub fn device() -> Result<HeadlessGpu, String> {
    pollster::block_on(device_async())
}

/// `device`, awaiting the adapter and device requests instead of blocking.
pub async fn device_async() -> Result<HeadlessGpu, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut why = Vec::new();
    for kind in [AdapterKind::Software, AdapterKind::Hardware] {
        match try_kind(&instance, kind).await {
            Ok(gpu) => return Ok(gpu),
            Err(err) => why.push(err),
        }
    }
    Err(format!("no graphics adapter: {}", why.join("; ")))
}

/// For GPU tests: the device, or `Ok(None)` after printing a loud SKIP
/// line to stderr when the machine has no adapter at all. With
/// `CAMCRAFT_REQUIRE_GPU=1` a missing adapter is `Err` instead, so a lane
/// that must render can't pass by skipping.
#[cfg(not(target_arch = "wasm32"))]
pub fn device_for_test(suite: &str) -> Result<Option<HeadlessGpu>, String> {
    match device() {
        Ok(gpu) => {
            eprintln!("{suite}: rendering on the {} adapter {}", gpu.kind.label(), gpu.name);
            Ok(Some(gpu))
        }
        Err(err) => {
            if gpu_required() {
                Err(format!("{suite}: {REQUIRE_GPU_ENV}=1 but {err}"))
            } else {
                eprintln!("*** {suite}: SKIPPED (no adapter) - NOT a pass: {err} ***");
                Ok(None)
            }
        }
    }
}

/// `CAMCRAFT_REQUIRE_GPU=1` is set: a skip must fail instead.
pub fn gpu_required() -> bool {
    std::env::var(REQUIRE_GPU_ENV).is_ok_and(|v| v.trim() == "1")
}
