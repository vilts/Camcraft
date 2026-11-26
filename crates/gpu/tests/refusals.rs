//! Refusals of the compute seam (#2620): each input the kernel cannot take
//! is an `Err` that names why, never a panic, so the caller keeps its CPU
//! path. No GPU needed. Gate map: covers camcraft_gpu::binning,
//! camcraft_gpu::probe, camcraft_gpu::switch. Our own tests.

use camcraft_gpu::{bin_twin, compute_switch, pad_boxes, plan, probe, Compute, GridSpec, COMPUTE_ENV};

fn spec() -> GridSpec {
    GridSpec { x0: 0.0, y0: 0.0, s: 1.0, nx: 4, ny: 4 }
}

fn roomy() -> Compute {
    Compute {
        max_storage_buffer_binding_size: 1 << 27,
        max_buffer_size: 1 << 28,
        max_compute_workgroup_storage_size: 16384,
        max_compute_workgroups_per_dimension: 65535,
    }
}

#[test]
fn non_finite_boxes_and_specs_are_refused_by_name() {
    let err = pad_boxes(&[[0.0, f64::NAN, 1.0, 1.0]], &spec()).err().unwrap_or_default();
    assert!(err.contains("facet box 0") && err.contains("not finite"), "{err}");
    for bad in [GridSpec { s: 0.0, ..spec() }, GridSpec { s: f64::INFINITY, ..spec() }, GridSpec { x0: f64::NAN, ..spec() }] {
        let err = pad_boxes(&[[0.0, 0.0, 1.0, 1.0]], &bad).err().unwrap_or_default();
        assert!(err.contains("cell size"), "{bad:?}: {err}");
    }
    let err = pad_boxes(&[], &GridSpec { nx: 0, ..spec() }).err().unwrap_or_default();
    assert!(err.contains("each axis"), "{err}");
    let err = pad_boxes(&[], &GridSpec { s: 1.0e-300, ..spec() }).err().unwrap_or_default();
    assert!(err.contains("f32"), "{err}");
    let err = pad_boxes(&[[0.0, 0.0, 1.0e13, 1.0]], &spec()).err().unwrap_or_default();
    assert!(err.contains("f32 range"), "{err}");
}

#[test]
fn a_plan_past_the_device_limits_is_refused_by_name() {
    let boxes: Vec<[f64; 4]> = (0..100).map(|i| [0.0, 0.0, 3.0, f64::from(i % 4)]).collect();
    let p = pad_boxes(&boxes, &spec()).expect("pads");
    assert!(plan(&p, &roomy()).is_ok());
    let small = Compute { max_storage_buffer_binding_size: 64, ..roomy() };
    let err = plan(&p, &small).err().unwrap_or_default();
    assert!(err.contains("facet box buffer") && err.contains("device limit"), "{err}");
    let one = Compute { max_compute_workgroups_per_dimension: 1, ..roomy() };
    let err = plan(&p, &one).err().unwrap_or_default();
    assert!(err.contains("workgroups"), "{err}");
}

#[test]
fn an_empty_set_plans_and_bins_to_empty_cells() {
    let p = pad_boxes(&[], &spec()).expect("pads");
    let plan = plan(&p, &roomy()).expect("plans");
    assert_eq!((plan.n, plan.words, plan.groups), (0, 0, 0));
    let lists = bin_twin(&p).expect("twin bins");
    assert_eq!(lists.cells(), 16);
    assert!((0..16).all(|c| lists.cell(c).is_empty()) && lists.cell(99).is_empty());
}

#[test]
fn no_compute_and_the_off_switch_are_named() {
    let caps = wgpu_caps_without_compute();
    let err = probe(caps.0, &caps.1, &caps.2).err().unwrap_or_default();
    assert!(err.starts_with("no compute"), "{err}");
    let err = compute_switch(Some("off")).err().unwrap_or_default();
    assert!(err.contains(COMPUTE_ENV), "{err}");
}

fn wgpu_caps_without_compute() -> (wgpu::Backend, wgpu::DownlevelCapabilities, wgpu::Limits) {
    let caps = wgpu::DownlevelCapabilities { flags: wgpu::DownlevelFlags::empty(), ..Default::default() };
    (wgpu::Backend::Gl, caps, wgpu::Limits::downlevel_webgl2_defaults())
}

#[test]
fn host_budget_refuses_large_empty_and_nonempty_grids_before_allocation() {
    for boxes in [vec![], vec![[0.0, 0.0, 0.0, 0.0]]] {
        let p = pad_boxes(&boxes, &GridSpec { nx: 65536, ny: 65535, ..spec() }).expect("pads");
        let err = plan(&p, &roomy()).expect_err("host budget must refuse");
        assert!(err.contains("host memory budget"), "{err}");
        let err = bin_twin(&p).expect_err("twin must refuse before allocating cells");
        assert!(err.contains("host memory budget"), "{err}");
    }
}

#[test]
fn twin_host_budget_counts_lists_and_offsets_together() {
    use camcraft_gpu::binning::MAX_HOST_BYTES;
    let bytes_per_cell = std::mem::size_of::<Vec<u32>>() + std::mem::size_of::<usize>();
    let nx = 4096;
    let ny = (MAX_HOST_BYTES / bytes_per_cell as u64 / u64::from(nx) + 1) as u32;
    let p = pad_boxes(&[], &GridSpec { nx, ny, ..spec() }).expect("pads");
    let err = bin_twin(&p).expect_err("combined host storage must refuse");
    assert!(err.contains("host memory budget"), "{err}");
}
