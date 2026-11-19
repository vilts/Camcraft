// Windowed GPU bring-up (#74): enumerate every adapter, log each, and try
// them adapter-by-adapter (gpu_select::order) inside the backend fallback
// chain (gpu::attempts) until one gives a device, a configured surface, the
// caller's pipelines AND one presented test frame - all checked, none
// assumed. Provenance: our own code, specced from the wgpu 30 public API
// docs (Instance::enumerate_adapters, RequestAdapterOptions::power_preference,
// Adapter::request_device, Surface::get_capabilities/configure/
// get_current_texture, Device::push_error_scope/set_device_lost_callback/
// poll) - no vendor reference consulted.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use egui_wgpu::wgpu;
use winit::window::Window;

use super::gpu;
use super::gpu_select::{self, AdapterDesc};

/// How long the test frame may take before the adapter counts as failing.
const TEST_FRAME_BUDGET: Duration = Duration::from_secs(5);

/// What a successful bring-up hands the shell; `built` is what the
/// caller's `build` made on this adapter (the viewport pipelines).
pub struct Bring<T> {
    pub surface: wgpu::Surface<'static>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub built: T,
}

type Key = (String, u32, u32, &'static str);

/// Bring the window's GPU up, or refuse naming every adapter tried and
/// why each failed (plus every adapter skipped). `want` is the `--gpu` /
/// `CAMCRAFT_GPU` substring. `build` makes the caller's GPU objects on a
/// candidate device; if it fails, that adapter fails and the next is tried.
pub fn bring_up<T, F>(window: &Arc<Window>, pick: &gpu::GpuPick, want: Option<&str>, mut build: F) -> Result<Bring<T>, String>
where
    F: FnMut(&wgpu::Device, &wgpu::Queue, &wgpu::SurfaceConfiguration) -> Result<T, String>,
{
    let size = window.inner_size();
    let mut seen: HashSet<Key> = HashSet::new();
    let mut tried: HashSet<Key> = HashSet::new();
    let mut failures: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    fn note(skipped: &mut Vec<String>, n: String) {
        if !skipped.contains(&n) {
            skipped.push(n);
        }
    }
    for (label, backends) in gpu::attempts(pick) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        // SAFETY: the shell keeps `window` alive in its own field and drops
        // the surface first (its field-order note); the handle stays valid.
        let target = match unsafe { wgpu::SurfaceTargetUnsafe::from_display_and_window(window, window) } {
            Ok(t) => t,
            Err(e) => {
                failures.push(format!("window surface handle unavailable ({e})"));
                return Err(gpu_select::refusal(&failures, &skipped));
            }
        };
        // SAFETY: same argument - the window outlives the surface.
        let surface = match unsafe { instance.create_surface_unsafe(target) } {
            Ok(s) => s,
            Err(err) => {
                diag_warn!("gpu: {label}: surface creation failed ({err}) - falling back");
                failures.push(format!("{label}: surface creation failed ({err})"));
                continue;
            }
        };
        let adapters = pollster::block_on(instance.enumerate_adapters(backends));
        let descs: Vec<AdapterDesc> = adapters
            .iter()
            .map(|a| AdapterDesc::from_info(&a.get_info(), a.is_surface_supported(&surface)))
            .collect();
        for d in &descs {
            if seen.insert(d.key()) {
                diag_info!("{}", d.log_line());
            }
        }
        if descs.is_empty() {
            diag_warn!("gpu: {label}: no adapters enumerated - falling back");
            note(&mut skipped, format!("{label}: no adapters enumerated"));
        }
        let hp = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .ok()
        .map(|a| AdapterDesc::from_info(&a.get_info(), true));
        let (order, notes) = gpu_select::order(&descs, want, hp.as_ref());
        for n in notes {
            diag_info!("gpu: {label}: {n}");
            note(&mut skipped, n);
        }
        for i in order {
            let (Some(adapter), Some(desc)) = (adapters.get(i), descs.get(i)) else { continue };
            if !tried.insert(desc.key()) {
                continue; // already failed under an earlier mask
            }
            let who = format!("\"{}\" via {}", desc.name, gpu::backend_name(desc.backend));
            diag_info!("gpu: trying {}", desc.summary());
            match try_adapter(&surface, adapter, size.width, size.height, &who, &mut build) {
                Ok((device, queue, config, built)) => {
                    diag_info!(
                        "gpu: using {} ({:?} {:?} {:?})",
                        desc.summary(),
                        config.format,
                        config.present_mode,
                        config.alpha_mode
                    );
                    return Ok(Bring { surface, adapter: adapter.clone(), device, queue, config, built });
                }
                Err(why) => {
                    diag_warn!("gpu: {who} failed: {why} - trying the next adapter");
                    failures.push(format!("{who}: {why}"));
                }
            }
        }
    }
    Err(gpu_select::refusal(&failures, &skipped))
}

type Tried<T> = (wgpu::Device, wgpu::Queue, wgpu::SurfaceConfiguration, T);

/// One candidate: device, capability-driven config, configure, the
/// caller's build and one presented test frame, all inside Validation +
/// OutOfMemory + Internal error scopes (so nothing reaches wgpu's
/// panicking default handler) with an unwind guard, and a device-lost
/// watch (wgpu reports a lost device only through that callback).
fn try_adapter<T, F>(
    surface: &wgpu::Surface<'static>,
    adapter: &wgpu::Adapter,
    w: u32,
    h: u32,
    who: &str,
    build: &mut F,
) -> Result<Tried<T>, String>
where
    F: FnMut(&wgpu::Device, &wgpu::Queue, &wgpu::SurfaceConfiguration) -> Result<T, String>,
{
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .map_err(|e| format!("device request refused ({e})"))?;
    let lost = Arc::new(AtomicBool::new(false));
    {
        let (lost, who) = (Arc::clone(&lost), who.to_owned());
        device.set_device_lost_callback(move |reason, msg| {
            if reason != wgpu::DeviceLostReason::Destroyed {
                diag_warn!("gpu: {who}: device lost ({reason:?}): {msg}");
                lost.store(true, Ordering::SeqCst);
            }
        });
    }
    let config = gpu_select::surface_config(&surface.get_capabilities(adapter), w, h)?;
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<T, String> {
        surface.configure(&device, &config);
        let built = build(&device, &queue, &config)?;
        test_frame(surface, &device, &queue)?;
        Ok(built)
    }));
    // Scopes pop in reverse push order.
    let errs = [
        ("internal error", pollster::block_on(internal.pop())),
        ("out of memory", pollster::block_on(oom.pop())),
        ("validation error", pollster::block_on(validation.pop())),
    ];
    let built = match run {
        Ok(Ok(b)) => b,
        Ok(Err(why)) => return Err(why),
        Err(_) => return Err(format!("bring-up aborted ({:?}, {:?})", config.format, config.present_mode)),
    };
    for (what, e) in errs {
        if let Some(e) = e {
            return Err(format!("{what} during bring-up ({e})"));
        }
    }
    // A lost device surfaces only via the callback; poll so it can fire.
    let polled = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(TEST_FRAME_BUDGET) });
    if lost.load(Ordering::SeqCst) {
        return Err("the device was lost during bring-up".into());
    }
    if let Err(e) = polled {
        return Err(format!("the test frame did not complete ({e})"));
    }
    Ok((device, queue, config, built))
}

/// Acquire, clear and present one frame: an adapter that configures but
/// cannot present (the Optimus blank window) fails here, not later.
fn test_frame(surface: &wgpu::Surface<'static>, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<(), String> {
    let st = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(st) | wgpu::CurrentSurfaceTexture::Suboptimal(st) => st,
        // Not this adapter's fault: the window is hidden or mid-resize.
        wgpu::CurrentSurfaceTexture::Timeout
        | wgpu::CurrentSurfaceTexture::Occluded
        | wgpu::CurrentSurfaceTexture::Outdated => {
            diag_info!("gpu: test frame skipped (window not presentable yet)");
            return Ok(());
        }
        wgpu::CurrentSurfaceTexture::Lost => return Err("the test frame's surface was lost".into()),
        wgpu::CurrentSurfaceTexture::Validation => return Err("the test frame failed validation".into()),
    };
    let view = st.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("gpu-test-frame") });
    drop(enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("gpu-test-frame"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
        })],
        ..Default::default()
    }));
    queue.submit([enc.finish()]);
    queue.present(st);
    Ok(())
}
