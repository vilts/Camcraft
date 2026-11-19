// Unit tests for gpu_select (#74): the hybrid-laptop adapter order and the
// capability-driven surface config, over synthetic descriptors.
// Provenance: our own tests, from the wgpu 30 public API docs.

use super::*;
use wgpu::{Backend, DeviceType};

fn a(name: &str, vendor: u32, ty: DeviceType, backend: Backend, ok: bool) -> AdapterDesc {
    AdapterDesc { name: name.into(), vendor, device: 1, device_type: ty, backend, driver: String::new(), driver_info: String::new(), surface_ok: ok }
}

/// Intel UHD + Quadro P3200 as an Optimus laptop enumerates them, Vulkan
/// and DX12 each, plus the WARP software rasterizer.
fn hybrid() -> Vec<AdapterDesc> {
    vec![
        a("Intel(R) UHD Graphics 630", 0x8086, DeviceType::IntegratedGpu, Backend::Vulkan, true),
        a("Quadro P3200", 0x10de, DeviceType::DiscreteGpu, Backend::Vulkan, true),
        a("Intel(R) UHD Graphics 630", 0x8086, DeviceType::IntegratedGpu, Backend::Dx12, true),
        a("Quadro P3200", 0x10de, DeviceType::DiscreteGpu, Backend::Dx12, true),
        a("Microsoft Basic Render Driver", 0x1414, DeviceType::Cpu, Backend::Dx12, true),
    ]
}

fn names(list: &[AdapterDesc], idx: &[usize]) -> Vec<String> {
    idx.iter().map(|&i| format!("{}/{}", list[i].name, crate::egui_app::gpu::backend_name(list[i].backend))).collect()
}

#[test]
fn startup_adapter_log_names_the_gpu_backend_and_driver() {
    let mut info = wgpu::AdapterInfo::new(DeviceType::IntegratedGpu, Backend::Vulkan);
    info.name = "AMD Radeon test adapter".into();
    info.vendor = 0x1002;
    info.device = 0x164e;
    info.driver = "AMD test driver".into();
    info.driver_info = "Adrenalin test version".into();
    let line = AdapterDesc::from_info(&info, true).log_line();
    for wanted in ["AMD Radeon test adapter", "0x1002", "vulkan", "AMD test driver", "Adrenalin test version"] {
        assert!(line.contains(wanted), "startup log omits {wanted}: {line}");
    }
}

#[test]
fn startup_gpu_refusal_names_backend_retry_flags() {
    let text = refusal(&[], &[]);
    for retry in ["--gpu-backend dx12", "--gpu-backend vulkan", "--gpu-backend gl"] {
        assert!(text.contains(retry), "GPU startup refusal omits {retry}: {text}");
    }
}

#[test]
fn discrete_before_integrated_before_cpu() {
    let list = hybrid();
    let (idx, notes) = order(&list, None, None);
    assert_eq!(idx.len(), 5);
    assert!(notes.is_empty(), "{notes:?}");
    let n = names(&list, &idx);
    assert!(n[0].starts_with("Quadro") && n[1].starts_with("Quadro"), "{n:?}");
    assert!(n[2].starts_with("Intel") && n[3].starts_with("Intel"), "{n:?}");
    assert!(n[4].starts_with("Microsoft"), "{n:?}");
    if cfg!(target_os = "windows") {
        assert_eq!(n[0], "Quadro P3200/dx12", "the native backend first on Windows");
    }
}

#[test]
fn user_choice_wins_case_insensitively() {
    let list = hybrid();
    let (idx, notes) = order(&list, Some(" uhd "), Some(&list[1]));
    assert!(notes.is_empty(), "{notes:?}");
    let n = names(&list, &idx);
    assert!(n[0].starts_with("Intel") && n[1].starts_with("Intel"), "{n:?}");
    assert!(n[2].starts_with("Quadro"), "the high-performance GPU follows: {n:?}");
}

#[test]
fn unmatched_choice_is_said_and_auto_order_stands() {
    let list = hybrid();
    let (idx, notes) = order(&list, Some("radeon"), None);
    assert_eq!(idx.len(), 5);
    assert!(notes.iter().any(|n| n.contains("radeon")), "{notes:?}");
    assert!(names(&list, &idx)[0].starts_with("Quadro"));
}

#[test]
fn high_performance_pick_leads_on_its_best_backend() {
    // Two discrete GPUs: the one wgpu's HighPerformance request named
    // goes first even though it enumerated second.
    let list = vec![
        a("GPU A", 1, DeviceType::DiscreteGpu, Backend::Vulkan, true),
        a("GPU B", 2, DeviceType::DiscreteGpu, Backend::Vulkan, true),
        a("GPU B", 2, DeviceType::DiscreteGpu, Backend::Dx12, true),
    ];
    let (idx, _) = order(&list, None, Some(&list[1]));
    let n = names(&list, &idx);
    assert!(n[0].starts_with("GPU B") && n[1].starts_with("GPU B"), "{n:?}");
    assert_eq!(n[2], "GPU A/vulkan");
}

#[test]
fn unpresentable_adapters_are_skipped_by_name() {
    let list = vec![
        a("Quadro P3200", 0x10de, DeviceType::DiscreteGpu, Backend::Dx12, false),
        a("Intel(R) UHD Graphics", 0x8086, DeviceType::IntegratedGpu, Backend::Dx12, true),
    ];
    let (idx, notes) = order(&list, Some("quadro"), None);
    assert_eq!(idx, vec![1], "only the presentable adapter is tried");
    assert!(notes.iter().any(|n| n.contains("Quadro P3200") && n.contains("cannot present")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("quadro")), "the unmet choice is said: {notes:?}");
}

#[test]
fn nothing_to_order_is_empty_not_a_panic() {
    let (idx, notes) = order(&[], Some("x"), None);
    assert!(idx.is_empty());
    assert_eq!(notes.len(), 1);
}

#[test]
fn log_line_names_every_field() {
    let l = hybrid()[1].log_line();
    for part in ["Quadro P3200", "0x10de", "discrete", "vulkan"] {
        assert!(l.contains(part), "{l}");
    }
    assert!(a("x", 0, DeviceType::Cpu, Backend::Gl, false).log_line().contains("cannot present"));
}

fn caps(formats: Vec<wgpu::TextureFormat>, present: Vec<wgpu::PresentMode>, alpha: Vec<wgpu::CompositeAlphaMode>) -> wgpu::SurfaceCapabilities {
    wgpu::SurfaceCapabilities { formats, present_modes: present, alpha_modes: alpha, ..Default::default() }
}

#[test]
fn config_comes_from_reported_capabilities() {
    let c = caps(
        vec![wgpu::TextureFormat::Bgra8UnormSrgb, wgpu::TextureFormat::Bgra8Unorm],
        vec![wgpu::PresentMode::Mailbox, wgpu::PresentMode::Fifo],
        vec![wgpu::CompositeAlphaMode::PreMultiplied, wgpu::CompositeAlphaMode::Opaque],
    );
    let conf = surface_config(&c, 0, 600).expect("usable caps");
    assert_eq!(conf.format, wgpu::TextureFormat::Bgra8UnormSrgb, "the surface's preferred format");
    assert_eq!(conf.present_mode, wgpu::PresentMode::Fifo, "vsync, never a spin");
    assert_eq!(conf.alpha_mode, wgpu::CompositeAlphaMode::Opaque);
    assert_eq!((conf.width, conf.height), (1, 600), "a zero extent is clamped");

    // An adapter whose surface offers only Immediate + Inherit gets those.
    let odd = caps(vec![wgpu::TextureFormat::Rgba8Unorm], vec![wgpu::PresentMode::Immediate], vec![wgpu::CompositeAlphaMode::Inherit]);
    let conf = surface_config(&odd, 10, 10).expect("usable caps");
    assert_eq!(conf.present_mode, wgpu::PresentMode::Immediate);
    assert_eq!(conf.alpha_mode, wgpu::CompositeAlphaMode::Inherit);
}

#[test]
fn incompatible_surface_refuses_by_name() {
    let err = surface_config(&caps(vec![], vec![wgpu::PresentMode::Fifo], vec![]), 1, 1).unwrap_err();
    assert!(err.contains("format"), "{err}");
    let err = surface_config(&caps(vec![wgpu::TextureFormat::Rgba8Unorm], vec![], vec![]), 1, 1).unwrap_err();
    assert!(err.contains("present mode"), "{err}");
}

#[test]
fn same_gpu_named_differently_per_backend_still_leads_on_dx12() {
    // DXGI and Vulkan can name one GPU differently; the PCI ids match.
    let mut list = vec![
        a("Intel(R) UHD Graphics 630", 0x8086, DeviceType::IntegratedGpu, Backend::Dx12, true),
        a("NVIDIA Quadro P3200", 0x10de, DeviceType::DiscreteGpu, Backend::Vulkan, true),
        a("Quadro P3200", 0x10de, DeviceType::DiscreteGpu, Backend::Dx12, true),
    ];
    list[0].device = 0x3e9b;
    list[1].device = 0x1bb6;
    list[2].device = 0x1bb6;
    let (idx, _) = order(&list, None, Some(&list[1]));
    let n = names(&list, &idx);
    assert!(n[0].contains("Quadro") && n[1].contains("Quadro"), "both Quadro entries are the HP pick: {n:?}");
    if cfg!(target_os = "windows") {
        assert_eq!(n[0], "Quadro P3200/dx12", "DX12 first on Windows: {n:?}");
    }
    // Without PCI ids the name decides: a differently named zero-id entry
    // is not the high-performance pick.
    let anon = vec![
        AdapterDesc { device: 0, ..a("Soft GPU", 0, DeviceType::DiscreteGpu, Backend::Vulkan, true) },
        AdapterDesc { device: 0, ..a("Other GPU", 0, DeviceType::DiscreteGpu, Backend::Vulkan, true) },
    ];
    let (idx, _) = order(&anon, None, Some(&anon[1]));
    assert_eq!(idx, vec![1, 0]);
}

#[test]
fn refusal_names_tried_and_skipped_adapters() {
    let skip = vec!["\"Quadro P3200\" via dx12: skipped - cannot present to this window".to_string()];
    let all_skipped = refusal(&[], &skip);
    assert!(!all_skipped.contains("no GPU adapter was found"), "{all_skipped}");
    assert!(all_skipped.contains("Quadro P3200") && all_skipped.contains("cannot present"), "{all_skipped}");
    let tried = vec!["\"Intel(R) UHD\" via dx12: the device was lost during bring-up".to_string()];
    let both = refusal(&tried, &skip);
    assert!(both.contains("tried: \"Intel(R) UHD\"") && both.contains("skipped: \"Quadro"), "{both}");
    assert!(refusal(&[], &[]).contains("no GPU adapter was found on any backend"));
}

#[test]
fn empty_alpha_modes_refuse_by_name() {
    let c = caps(vec![wgpu::TextureFormat::Rgba8Unorm], vec![wgpu::PresentMode::Fifo], vec![]);
    let err = surface_config(&c, 1, 1).unwrap_err();
    assert!(err.contains("alpha mode"), "{err}");
}

#[test]
fn caps_without_render_attachment_refuse_by_name() {
    let mut c = caps(
        vec![wgpu::TextureFormat::Rgba8Unorm],
        vec![wgpu::PresentMode::Fifo],
        vec![wgpu::CompositeAlphaMode::Opaque],
    );
    c.usages = wgpu::TextureUsages::COPY_SRC;
    let err = surface_config(&c, 1, 1).unwrap_err();
    assert!(err.contains("cannot be rendered to"), "{err}");
}
