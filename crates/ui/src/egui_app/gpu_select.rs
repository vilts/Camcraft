// Adapter ordering and surface configuration for the viewport (#74).
// Provenance: our own design, specced from the wgpu 30 public API docs
// (Instance::enumerate_adapters, Adapter::get_info / is_surface_supported,
// RequestAdapterOptions::power_preference, Surface::get_capabilities and
// SurfaceCapabilities) - no vendor reference consulted.
//
// Pure functions over plain descriptors, so the hybrid-laptop ordering is
// testable without the hardware: shell init feeds it what wgpu reported.

use egui_wgpu::wgpu;

/// What wgpu told us about one enumerated adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterDesc {
    pub name: String,
    pub vendor: u32,
    pub device: u32,
    pub device_type: wgpu::DeviceType,
    pub backend: wgpu::Backend,
    pub driver: String,
    pub driver_info: String,
    /// `Adapter::is_surface_supported` for the window's surface.
    pub surface_ok: bool,
}

impl AdapterDesc {
    pub fn from_info(info: &wgpu::AdapterInfo, surface_ok: bool) -> Self {
        AdapterDesc {
            name: info.name.clone(),
            vendor: info.vendor,
            device: info.device,
            device_type: info.device_type,
            backend: info.backend,
            driver: info.driver.clone(),
            driver_info: info.driver_info.clone(),
            surface_ok,
        }
    }

    /// One startup log line per adapter seen.
    pub fn log_line(&self) -> String {
        format!("gpu: seen {}{}", self.summary(), if self.surface_ok { "" } else { " (cannot present to this window)" })
    }

    /// The same metadata names both enumerated and selected adapters.
    pub fn summary(&self) -> String {
        format!(
            "\"{}\" vendor 0x{:04x} device 0x{:04x} {} via {} driver {:?} ({})",
            self.name,
            self.vendor,
            self.device,
            type_name(self.device_type),
            super::gpu::backend_name(self.backend),
            self.driver,
            self.driver_info
        )
    }

    /// The same physical GPU (any backend): the PCI vendor + device ids
    /// when both sides report them (DXGI and Vulkan can name one GPU
    /// differently); the name only when an id is missing (zero).
    fn same_gpu(&self, other: &AdapterDesc) -> bool {
        let ids = |d: &AdapterDesc| d.vendor != 0 && d.device != 0;
        if ids(self) && ids(other) {
            self.vendor == other.vendor && self.device == other.device
        } else {
            self.name == other.name
        }
    }

    /// Identity across instances (the fallback masks re-enumerate).
    pub fn key(&self) -> (String, u32, u32, &'static str) {
        (self.name.clone(), self.vendor, self.device, super::gpu::backend_name(self.backend))
    }
}

pub fn type_name(t: wgpu::DeviceType) -> &'static str {
    match t {
        wgpu::DeviceType::DiscreteGpu => "discrete",
        wgpu::DeviceType::IntegratedGpu => "integrated",
        wgpu::DeviceType::VirtualGpu => "virtual",
        wgpu::DeviceType::Cpu => "cpu",
        wgpu::DeviceType::Other => "other",
    }
}

fn type_rank(t: wgpu::DeviceType) -> u8 {
    match t {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::Cpu => 4,
    }
}

/// The platform's native presentation API first: on Windows DX12 presents
/// through DXGI on hybrid laptops without a Vulkan cross-adapter blit.
fn backend_rank(b: wgpu::Backend) -> u8 {
    let native = if cfg!(target_os = "windows") {
        wgpu::Backend::Dx12
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        wgpu::Backend::Metal
    } else {
        wgpu::Backend::Vulkan
    };
    match b {
        b if b == native => 0,
        wgpu::Backend::Vulkan | wgpu::Backend::Dx12 | wgpu::Backend::Metal => 1,
        wgpu::Backend::Gl => 2,
        _ => 3,
    }
}

/// Does the user's `--gpu` / `CAMCRAFT_GPU` substring name this adapter?
pub fn wanted(desc: &AdapterDesc, want: &str) -> bool {
    let w = want.trim().to_ascii_lowercase();
    !w.is_empty() && desc.name.to_ascii_lowercase().contains(&w)
}

/// The order to try adapters in (indices into `adapters`), plus a note for
/// every adapter left out. Order: a user-named adapter, then the GPU wgpu's
/// HighPerformance request answered with (on its best backend), then every
/// other surface-compatible adapter - discrete, integrated, virtual,
/// other, cpu - native backend first, enumeration order last. Adapters
/// that cannot present to the window are skipped by name.
pub fn order(
    adapters: &[AdapterDesc],
    want: Option<&str>,
    high_perf: Option<&AdapterDesc>,
) -> (Vec<usize>, Vec<String>) {
    let mut notes = Vec::new();
    let mut idx: Vec<usize> = Vec::new();
    for (i, a) in adapters.iter().enumerate() {
        if a.surface_ok {
            idx.push(i);
        } else {
            notes.push(format!(
                "\"{}\" via {}: skipped - cannot present to this window",
                a.name,
                super::gpu::backend_name(a.backend)
            ));
        }
    }
    let want = want.map(str::trim).filter(|w| !w.is_empty());
    if let Some(w) = want
        && !idx.iter().any(|&i| wanted(&adapters[i], w))
    {
        notes.push(format!("no presentable adapter matches the --gpu/CAMCRAFT_GPU choice \"{w}\" - automatic order"));
    }
    idx.sort_by_key(|&i| {
        let a = &adapters[i];
        let user = !want.is_some_and(|w| wanted(a, w));
        let hp = !high_perf.is_some_and(|h| h.same_gpu(a));
        (user, hp, type_rank(a.device_type), backend_rank(a.backend), i)
    });
    (idx, notes)
}

/// Build the surface configuration from what the surface reports for this
/// adapter, never from assumptions: the surface's preferred format, Fifo
/// (vsync, always offered per the spec) unless absent, Opaque alpha when
/// offered else the first reported mode, two frames in flight. `None` when
/// the surface reports nothing usable for this adapter.
pub fn surface_config(
    caps: &wgpu::SurfaceCapabilities,
    width: u32,
    height: u32,
) -> Result<wgpu::SurfaceConfiguration, String> {
    let format = *caps.formats.first().ok_or("the surface reports no texture format for this adapter")?;
    let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
        wgpu::PresentMode::Fifo
    } else {
        *caps.present_modes.first().ok_or("the surface reports no present mode for this adapter")?
    };
    let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
        wgpu::CompositeAlphaMode::Opaque
    } else {
        *caps.alpha_modes.first().ok_or("the surface reports no alpha mode for this adapter")?
    };
    if !caps.usages.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
        return Err("the surface cannot be rendered to on this adapter".into());
    }
    Ok(wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width: width.max(1),
        height: height.max(1),
        desired_maximum_frame_latency: 2,
        present_mode,
        alpha_mode,
        view_formats: vec![],
    })
}

/// The refusal text when no adapter brought the window up: every adapter
/// tried and why it failed, then every adapter or backend mask skipped
/// (cannot present, enumerated nothing, unmet `--gpu` choice).
pub fn refusal(failures: &[String], skipped: &[String]) -> String {
    let mut s = String::from("no GPU adapter could render to this window");
    match (failures.is_empty(), skipped.is_empty()) {
        (true, true) => s.push_str("; no GPU adapter was found on any backend"),
        (true, false) => s.push_str("; no adapter could be tried"),
        (false, _) => {
            s.push_str("; tried: ");
            s.push_str(&failures.join("; "));
        }
    }
    if !skipped.is_empty() {
        s.push_str("; skipped: ");
        s.push_str(&skipped.join("; "));
    }
    s.push_str("; try --gpu-backend dx12, --gpu-backend vulkan or --gpu-backend gl");
    s
}

#[cfg(test)]
#[path = "gpu_select_tests.rs"]
mod tests;
