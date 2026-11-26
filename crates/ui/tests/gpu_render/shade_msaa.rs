//! #2834 + #2958: the ground shadow and the occluded faces draw inside the
//! viewport's one 3D pass, into its colour and depth targets. Their
//! pipelines must therefore have the pass's sample count, and
//! `View3D::set_samples` must build them again with the other pipelines. A
//! pipeline whose count differs from the pass is a wgpu validation error,
//! which wgpu's default handler raises as a panic: these frames would fail.
//! Same device, lock and skip policy as the rest of `gpu_render`.
//! Provenance: our own tests over our own pipelines.

use std::sync::Arc;

use super::{FORMAT, GPU_TEST_LOCK, SIDE, model_part, shared_device, test_cam};
use camcraft_ui::draw_list::Effects;
use camcraft_ui::shade_pass::Shade;
use camcraft_ui::state::DisplayMode;
use camcraft_ui::viewport::View3D;
use egui_wgpu::wgpu;

/// One ShadedEdges frame of `view`, read back.
fn frame(view: &View3D, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
    let cam = test_cam();
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("shade-msaa") });
    view.render(queue, &mut enc, &cam.mvp(1.0), cam.eye_f32(), DisplayMode::ShadedEdges);
    queue.submit([enc.finish()]);
    let (rgba, w, h) = view.snapshot_rgba(device, queue).expect("the frame reads back");
    assert_eq!((w, h), (SIDE, SIDE));
    rgba
}

#[test]
fn set_samples_rebuilds_the_shade_pipelines_for_one_and_four_samples() {
    let _gpu_guard = GPU_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((device, queue)) = shared_device() else {
        eprintln!("gpu_render shade_msaa: no adapter - SKIP");
        return;
    };
    let cube = [model_part(40.0)];
    let floor = cube[0].verts.iter().map(|v| v[2]).fold(f32::INFINITY, f32::min);
    let shade = Shade {
        shadow: Arc::new(camcraft_ui::ground_shadow::mesh(&[&cube[0].verts], floor)),
        occlusion: Arc::new(vec![0.5; cube[0].verts.len()]),
    };
    assert!(!shade.shadow.is_empty(), "the cube has a shadow mesh");
    let mut view = View3D::new(&device, FORMAT, SIDE, SIDE);
    assert_eq!(view.shade_samples(), 1, "built for the first count of the pass");
    view.set_document(&device, &queue, &cube);
    view.set_shade(&device, &queue, &shade);
    let on = Effects { ground_shadow: true, ambient_occlusion: true };
    for samples in [4, 1] {
        assert!(view.set_samples(&device, samples), "{samples}: a new count rebuilds");
        assert_eq!((view.samples(), view.shade_samples()), (samples, samples), "the shade pipelines follow the pass");
        view.set_effects(on);
        let shaded = frame(&view, &device, &queue);
        view.set_effects(Effects::default());
        let plain = frame(&view, &device, &queue);
        let differ = shaded.as_chunks::<4>().0.iter().zip(plain.as_chunks::<4>().0).filter(|(a, b)| a != b).count();
        assert!(differ > 100, "{samples}: the shadow and the occlusion draw: {differ} px differ");
    }
}
