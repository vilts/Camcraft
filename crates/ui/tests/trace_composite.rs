//! M7.2 B3b: the traced stock lands in the 3D pass. The composite
//! paints where the field was hit, replaces the translucent sheet, and
//! its written depth sorts against the meshes both ways - a model in
//! front of the stock hides it, a model behind it is hidden. Adapter and
//! skip policy: the `gpu_render` precedent (`headless_gpu`, #615 - loud
//! SKIP, `CAMCRAFT_REQUIRE_GPU=1` fails).
//! Provenance: our own test rig; nothing from vendor binaries.
//!
//! Gate map (M4.4): covers ui::trace::{eye, composite} and
//! viewport::View3D::render_with.

use std::sync::Arc;

use egui_wgpu::wgpu;

use camcraft_app::geometry::{cube, Point3};
use camcraft_app::stock_sim::{Backend, Stock, FIELD_BAND};
use camcraft_app::stock_view::FieldView;
use camcraft_ui::state::DisplayMode;
use camcraft_ui::trace::eye::StockEye;
use camcraft_ui::viewport::{Camera, GpuPart, View3D};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const W: u32 = 96;
const H: u32 = 72;

/// Software adapter first, else the hardware one (#615); no adapter at
/// all prints a loud SKIP, and `CAMCRAFT_REQUIRE_GPU=1` fails instead.
fn device_and_queue() -> Option<(wgpu::Device, wgpu::Queue)> {
    match camcraft_ui::headless_gpu::device_for_test("trace_composite") {
        Ok(gpu) => gpu.map(|g| (g.device, g.queue)),
        Err(err) => panic!("{err}"),
    }
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn field_view() -> Arc<FieldView> {
    let f = cube(p(0.0, 0.0, 0.0), p(10.0, 10.0, 5.0));
    let stock = Stock::from_facets(Backend::Field { band: FIELD_BAND }, &f.facets, 0.5).unwrap();
    Arc::new(FieldView { stock, target: None, target_note: Some("test".into()), cell: 0.5, tol: camcraft_app::stock_sim::DEV_TOL, packed: None })
}

fn verts(lo: Point3, hi: Point3) -> Vec<[f32; 3]> {
    cube(lo, hi)
        .facets
        .iter()
        .flat_map(|t| [t.a, t.b, t.c])
        .map(|v| [v.x as f32, v.y as f32, v.z as f32])
        .collect()
}

/// A horizontal sheet at height `z` covering the stock's footprint
/// (counter-clockwise from above: the camera sees its front face).
fn plate(z: f32) -> GpuPart {
    let q = [[-20.0, -20.0, z], [30.0, -20.0, z], [30.0, 30.0, z], [-20.0, -20.0, z], [30.0, 30.0, z], [-20.0, 30.0, z]];
    GpuPart { verts: q.to_vec(), selected: false, stock: false, fill: false, classes: Vec::new() }
}

fn stock_part() -> GpuPart {
    GpuPart { verts: verts(p(0.0, 0.0, 0.0), p(10.0, 10.0, 5.0)), selected: false, stock: true, fill: false, classes: Vec::new() }
}

/// Render `parts` with or without the traced stock; the centre pixel.
fn centre(dev: &wgpu::Device, q: &wgpu::Queue, parts: &[GpuPart], traced: bool) -> [u8; 4] {
    centre_with(dev, q, parts, traced, false)
}

fn centre_with(dev: &wgpu::Device, q: &wgpu::Queue, parts: &[GpuPart], traced: bool, cpu: bool) -> [u8; 4] {
    let mut view = View3D::new(dev, FORMAT, W, H);
    view.set_document(dev, q, parts);
    let cam = Camera { target: [5.0, 5.0, 2.5], yaw: 0.9, pitch: 0.9, dist: 30.0, ..Camera::default() };
    let mvp = cam.mvp(f64::from(W) / f64::from(H));
    let mut eye = StockEye::new(dev, FORMAT).unwrap();
    if cpu {
        eye = eye.cpu_only();
    }
    let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let fv = field_view();
    if traced {
        let mut ready = eye.prepare(dev, q, &mut enc, Some(&fv), &cam, &mvp, (W, H)).unwrap();
        if cpu {
            // The CPU march runs off this thread (M13.1): the first
            // call only asks for the picture; it is bound once it lands.
            assert!(!ready, "the first CPU frame is pending, never traced on the caller's thread");
            assert!(eye.wait_cpu_picture(std::time::Duration::from_secs(30)), "the CPU worker finishes the picture");
            ready = eye.prepare(dev, q, &mut enc, Some(&fv), &cam, &mvp, (W, H)).unwrap();
        }
        assert!(ready, "a sane field and camera must yield a traced frame");
        let draw = |pass: &mut wgpu::RenderPass<'_>| eye.draw(pass);
        view.render_with(q, &mut enc, &mvp, cam.eye_f32(), DisplayMode::ShadedEdges, Some(&draw));
    } else {
        view.render(q, &mut enc, &mvp, cam.eye_f32(), DisplayMode::ShadedEdges);
    }
    q.submit([enc.finish()]);
    let (rgba, w, h) = view.snapshot_rgba(dev, q).unwrap();
    let i = ((h / 2 * w + w / 2) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2)
}

#[test]
fn the_traced_stock_paints_opaque_gray_where_the_sheet_was() {
    let Some((dev, q)) = device_and_queue() else {
        eprintln!("trace_composite: no adapter - skipped");
        return;
    };
    let sheet = centre(&dev, &q, &[stock_part()], false);
    let traced = centre(&dev, &q, &[stock_part()], true);
    assert!(!close(sheet, traced), "the trace must replace the sheet: {sheet:?} vs {traced:?}");
    // Unjudged (no target): a flat gray, never green - r, g, b within a few levels.
    let [r, g, b, a] = traced;
    assert_eq!(a, 255);
    assert!(r.abs_diff(g) <= 6 && g.abs_diff(b) <= 6, "class 3 paints gray: {traced:?}");
    assert!(g > 40, "a lit hit is not the background: {traced:?}");
}

#[test]
fn the_traced_depth_sorts_against_models_both_ways() {
    let Some((dev, q)) = device_and_queue() else {
        eprintln!("trace_composite: no adapter - skipped");
        return;
    };
    let traced_alone = centre(&dev, &q, &[stock_part()], true);
    // A plate above the stock hides it: the pixel is the plate's own.
    let above = [plate(8.0), stock_part()];
    assert!(close(centre(&dev, &q, &above, true), centre(&dev, &q, &[plate(8.0)], false)));
    // A plate below the stock is hidden by it: the pixel is the trace's.
    let below = [plate(-6.0), stock_part()];
    assert!(close(centre(&dev, &q, &below, true), traced_alone));
}

/// M13.1 leftover: the CPU fallback never marches on the caller's
/// thread. The first `prepare` returns inside a short budget with the
/// march in flight (the slot keeps what it drew: `Ok(false)`), the
/// picture arrives on the worker, and the next `prepare` binds it.
#[test]
fn the_cpu_fallback_marches_off_the_ui_thread_and_the_picture_arrives() {
    let Some((dev, q)) = device_and_queue() else {
        eprintln!("trace_composite: no adapter - skipped");
        return;
    };
    let cam = Camera { target: [5.0, 5.0, 2.5], yaw: 0.9, pitch: 0.9, dist: 30.0, ..Camera::default() };
    let mvp = cam.mvp(f64::from(W) / f64::from(H));
    let mut eye = StockEye::new(&dev, FORMAT).unwrap().cpu_only();
    let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    // A frame big enough that a march on this thread would show in
    // the clock: the CPU thumbnail budget's worth of pixels.
    let big = (1024, 1024);
    let fv = field_view();
    let t0 = std::time::Instant::now();
    let first = eye.prepare(&dev, &q, &mut enc, Some(&fv), &cam, &mvp, big).unwrap();
    let dt = t0.elapsed();
    assert!(!first, "the first frame is pending: the slot draws what it drew before");
    assert!(dt < std::time::Duration::from_millis(250), "prepare returned at once with the march in flight: {dt:?}");
    assert!(eye.wait_cpu_picture(std::time::Duration::from_secs(60)), "the worker finishes the picture");
    let second = eye.prepare(&dev, &q, &mut enc, Some(&fv), &cam, &mvp, big).unwrap();
    assert!(second, "the finished picture is bound on the next frame");
    // A moved camera keeps the last picture up while the next marches.
    let moved = Camera { target: [5.0, 5.0, 2.5], yaw: 1.2, pitch: 0.7, dist: 28.0, ..Camera::default() };
    let mvp2 = moved.mvp(f64::from(W) / f64::from(H));
    let t1 = std::time::Instant::now();
    let kept = eye.prepare(&dev, &q, &mut enc, Some(&fv), &moved, &mvp2, big).unwrap();
    assert!(kept, "the last finished picture stands while the next is pending");
    assert!(t1.elapsed() < std::time::Duration::from_millis(250), "and the frame still did not wait");
}

#[test]
fn the_cpu_fallback_paints_what_the_gpu_eye_paints() {
    let Some((dev, q)) = device_and_queue() else {
        eprintln!("trace_composite: no adapter - skipped");
        return;
    };
    let gpu = centre_with(&dev, &q, &[stock_part()], true, false);
    let cpu = centre_with(&dev, &q, &[stock_part()], true, true);
    // Same field, same eye, same shade law: the fallback is a speedup
    // lost, never a different picture.
    assert!(gpu.iter().zip(cpu).all(|(a, b)| a.abs_diff(b) <= 8), "{gpu:?} vs {cpu:?}");
}
