//! The layer playback of the Simulate view (#3336; slice S3c of the GPU
//! stock sim, #2629): which playback the transport starts, and the GPU
//! cutter that cuts its layer on the window's own render device.
//! Provenance: our own design (decision 3336) over camcraft-gpu's
//! `cut_onto_or_cpu_on` and the layer mode of camcraft-app's playback
//! worker; nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **The render device, never a second one.** The kernel runs on the
//!   device that draws the window. A second device in the process can
//!   crash a driver (#3589), and the shared compute device prefers the
//!   software adapter (#2629, precondition 2).
//! - **A hardware adapter only.** A software adapter (WARP, lavapipe)
//!   runs the kernel on the CPU, where the CPU playback is the faster
//!   one: it gets no cutter, by name.
//! - **Only an eye that reads the packed frame.** The layer ops' cut is
//!   not in `FieldView::stock`. The layer playback starts only while the
//!   stock eye traces on the GPU, and an eye that cannot hold a layer
//!   frame on the GPU ends it: the CPU playback starts in its place.
//! - **The switch turns it off.** `CAMCRAFT_GPU_COMPUTE=off` gives no
//!   cutter. A kernel fault at run time is the fallback rule of
//!   `cut_onto_or_cpu_on`: the CPU law answers that batch.
//! - **Display only.** No verdict and no posted output reads this layer.

use std::sync::Arc;

use camcraft_app::stock_play::{PlaySeed, PRECALC_BUDGET};
use camcraft_app::stock_play_worker::PlayWorker;
use camcraft_app::stock_sim::preview::LatticeMeta;
use camcraft_app::stock_sim::{LayerCutter, TwinItem};
use camcraft_gpu::{CutGrid, CutInput, CutItem};

use crate::state::State;

/// What the Simulate view knows about the layer playback (view state,
/// never a command, never saved).
#[derive(Default)]
pub struct LayerPlayback {
    /// The cutter on the render device, set one time by the window shell.
    /// `None`: this process plays on the CPU.
    pub cutter: Option<Arc<LayerCutter>>,
    /// The stock eye traces on the GPU this frame (the view pass says).
    pub eye: bool,
    /// The running playback worker was started in the layer mode.
    pub on: bool,
    /// The eye could not hold a layer frame on the GPU: the layer
    /// playback stays off for this process.
    pub refused: bool,
}

impl LayerPlayback {
    /// The cutter for a playback that starts now, or `None` for the CPU
    /// playback (the module law).
    #[must_use]
    pub fn start_cutter(&self) -> Option<Arc<LayerCutter>> {
        self.cutter.clone().filter(|_| self.eye && !self.refused)
    }

    /// Whether the running layer playback must end: the eye holds its
    /// frame, but not on the GPU, so it would march `FieldView::stock`.
    #[must_use]
    pub const fn must_end(&self, eye_holds_frame: bool, eye_on_gpu: bool) -> bool {
        self.on && eye_holds_frame && !eye_on_gpu
    }
}

/// Start the playback worker of the transport: in the layer mode when
/// `start_cutter` gives a cutter, else the CPU playback. Both calculate
/// the removal before playback (#2181).
pub(crate) fn spawn(state: &mut State, seed: PlaySeed) -> Result<PlayWorker, String> {
    let cutter = state.layer_play.start_cutter();
    state.layer_play.on = cutter.is_some();
    match cutter {
        Some(cutter) => PlayWorker::spawn_layered_precalc(seed, PRECALC_BUDGET, cutter),
        None => PlayWorker::spawn_precalc(seed, PRECALC_BUDGET),
    }
}

/// The eye cannot show the layer playback's frames on the GPU (the
/// module law): that playback ends, the fallback is said one time, and
/// the next frame starts the CPU playback in its place.
pub(crate) fn end(state: &mut State) {
    state.field_play = None;
    state.field_view = None;
    state.layer_play.on = false;
    state.layer_play.refused = true;
    state.say_once("simulation cuts the stock on the CPU: the stock view cannot show the GPU cut layer");
}

/// One kernel item of camcraft-gpu from the twin's item, field by field.
#[must_use]
pub fn cut_item(it: &TwinItem) -> CutItem {
    let t = &it.tile;
    CutItem { a: t.a, b: t.b, r: t.r, flank: t.flank, margin: t.margin, ball: t.ball, base: it.base, lo: it.lo, hi: it.hi }
}

/// One kernel batch on `meta`'s lattice (the twin's own f32 cell and cap).
#[must_use]
pub fn cut_input(meta: &LatticeMeta, items: &[TwinItem]) -> CutInput {
    CutInput {
        grid: CutGrid { dims: meta.dims, cell: meta.cell as f32, cap: meta.cap as f32 },
        items: items.iter().map(cut_item).collect(),
    }
}

/// Why an adapter of this type gets no kernel, or `Ok` (the module law).
pub fn kernel_adapter(kind: egui_wgpu::wgpu::DeviceType) -> Result<(), String> {
    if kind == egui_wgpu::wgpu::DeviceType::Cpu {
        return Err("the window draws on a software adapter, where the CPU playback is faster".into());
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::sync::{Arc, OnceLock};

    use camcraft_app::stock_sim::preview::LatticeMeta;
    use camcraft_app::stock_sim::{LayerCutter, TwinItem};
    use camcraft_gpu::{compute_switch, cut_onto_or_cpu_on, probe, AdapterKind, CutKernel, HeadlessGpu, COMPUTE_ENV};
    use egui_wgpu::wgpu;

    use super::{cut_input, kernel_adapter};

    /// The layer cutter on `gpu`: the kernel is compiled on the first
    /// batch (on the playback worker, never on the UI thread), and each
    /// batch takes the fallback rule of `cut_onto_or_cpu_on`.
    #[must_use]
    pub fn gpu_cutter(gpu: HeadlessGpu) -> Arc<LayerCutter> {
        let kernel: OnceLock<Result<CutKernel, String>> = OnceLock::new();
        Arc::new(move |meta: &LatticeMeta, items: &[TwinItem], base: &[f32]| {
            let input = cut_input(meta, items);
            let made = kernel.get_or_init(|| CutKernel::new(&gpu.device));
            let on = made.as_ref().map(|k| (&gpu, k)).map_err(Clone::clone);
            cut_onto_or_cpu_on(on, &input, base).map(|out| out.values)
        })
    }

    /// `render_cutter` for the shell: `None`, and one log line with the
    /// reason, when this process plays on the CPU.
    #[must_use]
    pub fn shell_cutter(adapter: &wgpu::Adapter, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Arc<LayerCutter>> {
        render_cutter(adapter, device, queue)
            .inspect_err(|why| tracing::info!(target: "camcraft::playback", "stock playback cuts on the CPU: {why}"))
            .ok()
    }

    /// The layer cutter on the window's render device, or why this
    /// process plays on the CPU: the switch is off, the adapter is a
    /// software one, or the device has no compute.
    pub fn render_cutter(
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Arc<LayerCutter>, String> {
        compute_switch(std::env::var(COMPUTE_ENV).ok().as_deref())?;
        let info = adapter.get_info();
        kernel_adapter(info.device_type)?;
        let compute = probe(info.backend, &adapter.get_downlevel_capabilities(), &device.limits())?;
        Ok(gpu_cutter(HeadlessGpu {
            device: device.clone(),
            queue: queue.clone(),
            kind: AdapterKind::Hardware,
            name: format!("\"{}\" via {:?}", info.name, info.backend),
            compute: Ok(compute),
        }))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{gpu_cutter, render_cutter, shell_cutter};
