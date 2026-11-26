// GPU backend selection for the viewport. Implements ROADMAP M4.2's
// "backend pick + honest fallback chain" from our own design notes and the
// wgpu 30 API (InstanceDescriptor::backends, Surface::configuration,
// DownlevelCapabilities) — no vendor reference consulted.

use egui_wgpu::wgpu;

/// What the user asked the viewport to run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuPick {
    /// wgpu's default PRIMARY mask (Vulkan/Metal/DX12/WebGPU), as today.
    Auto,
    /// Try exactly this backend first; the rest of the chain follows.
    Prefer(wgpu::Backend),
}

/// Name a backend the way the flag spells it.
pub fn backend_name(b: wgpu::Backend) -> &'static str {
    match b {
        wgpu::Backend::Vulkan => "vulkan",
        wgpu::Backend::Metal => "metal",
        wgpu::Backend::Dx12 => "dx12",
        wgpu::Backend::Gl => "gl",
        _ => "other",
    }
}

fn mask(b: wgpu::Backend) -> wgpu::Backends {
    // Backends' bit positions are defined as `1 << Backend::X as u32`.
    wgpu::Backends::from_bits_retain(1 << b as u32)
}

/// Parse a backend name (`dx12|metal|vulkan|gl`, case-insensitive).
/// Unknown names are refused by name — never silently treated as auto.
pub fn parse_backend(s: &str) -> Result<wgpu::Backend, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "dx12" | "d3d12" => Ok(wgpu::Backend::Dx12),
        "metal" => Ok(wgpu::Backend::Metal),
        "vulkan" => Ok(wgpu::Backend::Vulkan),
        "gl" | "gles" | "opengl" => Ok(wgpu::Backend::Gl),
        other => Err(format!(
            "unknown GPU backend '{other}' (expected dx12, metal, vulkan or gl)"
        )),
    }
}

/// Resolve the pick from argv + `CAMCRAFT_GPU_BACKEND`. The explicit
/// `--gpu-backend <name>` flag wins over the environment; a bad value in
/// either source refuses by name before any window opens.
pub fn pick_from(args: &[String], env: Option<&str>) -> Result<GpuPick, String> {
    if let Some(v) = flag_value(args, "--gpu-backend") {
        let Some(v) = v.filter(|v| !v.trim().is_empty()) else {
            return Err("--gpu-backend needs a value (dx12, metal, vulkan or gl)".into());
        };
        return Ok(GpuPick::Prefer(parse_backend(&v)?));
    }
    if let Some(v) = env.map(str::trim).filter(|v| !v.is_empty()) {
        return Ok(GpuPick::Prefer(parse_backend(v)?));
    }
    Ok(GpuPick::Auto)
}

/// A flag's value in either spelling, `--flag value` or `--flag=value`:
/// `None` when the flag is absent, `Some(None)` when it is last on the line.
fn flag_value(args: &[String], flag: &str) -> Option<Option<String>> {
    args.iter().enumerate().find_map(|(i, a)| {
        if a == flag {
            Some(args.get(i + 1).cloned())
        } else {
            a.strip_prefix(flag).and_then(|rest| rest.strip_prefix('=')).map(|v| Some(v.to_owned()))
        }
    })
}

/// #74: the adapter the user names by a case-insensitive substring of its
/// reported name (`--gpu "quadro"` or `--gpu=quadro`, winning over
/// `CAMCRAFT_GPU`). A flag with no value refuses by name; an empty env
/// value means automatic. `--gpu-backend` narrows which adapters `--gpu`
/// can match: its mask is tried first, so `--gpu` picks among that
/// backend's adapters before the fallback chain widens the set.
pub fn adapter_from(args: &[String], env: Option<&str>) -> Result<Option<String>, String> {
    if let Some(v) = flag_value(args, "--gpu") {
        return match v.as_deref().map(str::trim) {
            Some(v) if !v.is_empty() && !v.starts_with("--") => Ok(Some(v.to_owned())),
            _ => Err("--gpu needs an adapter name (a part of it is enough, e.g. --gpu quadro)".into()),
        };
    }
    Ok(env.map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned))
}

/// The honest fallback chain: (label, backend mask) pairs tried in order.
/// Each pair gets its own instance + adapter request; the first that
/// yields a usable adapter wins and says so on stdout.
pub fn attempts(pick: &GpuPick) -> Vec<(&'static str, wgpu::Backends)> {
    let mut chain = Vec::new();
    if let GpuPick::Prefer(b) = pick {
        chain.push((backend_name(*b), mask(*b)));
    }
    chain.push(("auto", wgpu::Backends::PRIMARY));
    chain.push(("all", wgpu::Backends::all()));
    chain
}

/// What the viewport actually runs on: the adapter wgpu handed us, which
/// backend answered, and whether we landed on the second-tier GL path
/// (wgpu's own wording: GLES/WebGL2 is the conservative feature set).
pub struct GpuInfo {
    pub adapter: String,
    pub backend: &'static str,
    pub tier1: bool,
}

impl GpuInfo {
    /// One-line startup log naming the adapter — the roadmap's "tell the
    /// user what the viewport actually runs on".
    pub fn log_line(&self) -> String {
        let mut s = format!("gpu: adapter \"{}\" via {}", self.adapter, self.backend);
        if self.tier1 {
            s.push_str(" — gl path: conservative feature set");
        }
        s
    }
}

/// Report the adapter we ended up with.
pub fn describe(adapter: &wgpu::Adapter) -> GpuInfo {
    let a = adapter.get_info();
    let tier1 = matches!(a.backend, wgpu::Backend::Gl);
    GpuInfo { adapter: a.name, backend: backend_name(a.backend), tier1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_each_backend_case_insensitively() {
        for (s, b) in [
            ("dx12", wgpu::Backend::Dx12),
            ("DX12", wgpu::Backend::Dx12),
            ("d3d12", wgpu::Backend::Dx12),
            ("Metal", wgpu::Backend::Metal),
            ("vulkan", wgpu::Backend::Vulkan),
            ("GL", wgpu::Backend::Gl),
            (" opengl ", wgpu::Backend::Gl),
        ] {
            assert_eq!(parse_backend(s).ok(), Some(b), "{s}");
        }
    }

    #[test]
    fn unknown_name_refuses_by_name() {
        let err = parse_backend("directx").unwrap_err();
        assert!(err.contains("directx"), "{err}");
        assert!(err.contains("dx12"), "{err}");
    }

    #[test]
    fn flag_wins_over_env() {
        let args = argv(&["--gpu-backend", "dx12"]);
        assert_eq!(
            pick_from(&args, Some("vulkan")).unwrap(),
            GpuPick::Prefer(wgpu::Backend::Dx12)
        );
    }

    #[test]
    fn env_used_when_no_flag() {
        assert_eq!(
            pick_from(&[], Some(" vulkan ")).unwrap(),
            GpuPick::Prefer(wgpu::Backend::Vulkan)
        );
        assert_eq!(pick_from(&[], None).unwrap(), GpuPick::Auto);
        assert_eq!(pick_from(&[], Some("")).unwrap(), GpuPick::Auto);
    }

    #[test]
    fn bad_flag_refuses_before_any_window() {
        let err = pick_from(&argv(&["--gpu-backend"]), None).unwrap_err();
        assert!(err.contains("needs a value"), "{err}");
        assert!(pick_from(&argv(&["--gpu-backend", "webgpu"]), None).is_err());
    }

    #[test]
    fn chain_prefers_then_falls_back() {
        let c = attempts(&GpuPick::Prefer(wgpu::Backend::Dx12));
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].0, "dx12");
        assert_eq!(c[0].1, wgpu::Backends::DX12);
        assert_eq!(c[1].1, wgpu::Backends::PRIMARY);
        assert_eq!(c[2].1, wgpu::Backends::all());
        assert_eq!(attempts(&GpuPick::Auto).len(), 2);
    }

    #[test]
    fn adapter_choice_flag_then_env() {
        assert_eq!(adapter_from(&argv(&["--gpu", "Quadro"]), Some("intel")).unwrap().as_deref(), Some("Quadro"));
        assert_eq!(adapter_from(&[], Some(" intel ")).unwrap().as_deref(), Some("intel"));
        assert_eq!(adapter_from(&[], Some("  ")).unwrap(), None);
        assert_eq!(adapter_from(&argv(&["--gpu-backend", "dx12"]), None).unwrap(), None);
        assert!(adapter_from(&argv(&["--gpu"]), None).unwrap_err().contains("needs an adapter name"));
        assert!(adapter_from(&argv(&["--gpu", "--headless"]), None).is_err());
    }

    #[test]
    fn equals_form_flags_are_parsed() {
        assert_eq!(adapter_from(&argv(&["--gpu=quadro"]), Some("intel")).unwrap().as_deref(), Some("quadro"));
        assert!(adapter_from(&argv(&["--gpu="]), None).unwrap_err().contains("needs an adapter name"));
        assert_eq!(adapter_from(&argv(&["--gpu-backend=dx12"]), None).unwrap(), None);
        assert_eq!(pick_from(&argv(&["--gpu-backend=vulkan"]), Some("dx12")).unwrap(), GpuPick::Prefer(wgpu::Backend::Vulkan));
        assert!(pick_from(&argv(&["--gpu-backend="]), None).unwrap_err().contains("needs a value"));
        assert!(pick_from(&argv(&["--gpu-backend=webgpu"]), None).unwrap_err().contains("webgpu"));
    }

    #[test]
    fn adapter_probe_names_the_gpu() {
        let _gpu = crate::egui_app::gpu_test_lock::gpu_guard(); // #3589
        // The real instance here; machines without a GPU adapter skip -
        // an honest CI note, not a silent pass.
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let options = wgpu::RequestAdapterOptions::default();
        let adapter = match pollster::block_on(instance.request_adapter(&options)) {
            Ok(a) => a,
            Err(err) => {
                eprintln!("skip: no GPU adapter on this machine ({err})");
                return;
            }
        };
        let info = describe(&adapter);
        assert!(!info.adapter.is_empty());
        assert!(
            ["vulkan", "metal", "dx12", "gl", "webgpu", "other"].contains(&info.backend),
            "{}",
            info.backend
        );
        println!("{}", info.log_line());
    }

}
