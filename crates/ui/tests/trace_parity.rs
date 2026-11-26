//! M7.2 B: the GPU eye answers what the CPU eye answers. The compute
//! kernel (`trace.wgsl`) ports the sim's preview march term for term,
//! so on the same field, camera and frame the two must agree on which
//! pixels hit, how far, and what class - within f32 rounding of the
//! lattice and the one stated difference (the GPU classes by the
//! target's lattice, the CPU by its exact facets). Refusals name their
//! reason. Adapter and skip policy: the `gpu_render` precedent
//! (`headless_gpu`, #615 - loud SKIP, `CAMCRAFT_REQUIRE_GPU=1` fails).
//! Provenance: our own test rig over Hart's sphere tracing (SIGGRAPH
//! 1996); nothing from vendor binaries.
//!
//! Gate map (M4.4): covers ui::trace and sim's preview::pack seam.

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::stock_sim::preview::{pack_field, pack_target, Preview};
use camcraft_app::stock_sim::{Backend, PreviewCamera, SimOp, Stock, DEV_TOL, FIELD_BAND};
use camcraft_ui::headless_gpu::TestGpu;
use camcraft_ui::trace::TracePass;

/// Software adapter first, else the hardware one (#615); no adapter at
/// all prints a loud SKIP, and `CAMCRAFT_REQUIRE_GPU=1` fails instead.
/// #3605: the one device of this binary, held under the process GPU lock
/// for the life of the returned value. Parallel per-test devices crashed
/// a hybrid-GPU Windows box with 0xC0000005.
fn device_and_queue() -> Option<TestGpu> {
    match camcraft_ui::headless_gpu::test_gpu("trace_parity") {
        Ok(gpu) => gpu,
        Err(err) => panic!("{err}"),
    }
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn field_stock(lo: Point3, hi: Point3) -> Stock {
    let f = cube(lo, hi);
    Stock::from_facets(Backend::Field { band: FIELD_BAND }, &f.facets, 1.0).unwrap()
}

/// The stage-A fixture: a 10 x 10 x 5 box pocketed to z = `floor`.
fn pocketed(floor: f64) -> Stock {
    let mut s = field_stock(p(0.0, 0.0, 0.0), p(10.0, 10.0, 5.0));
    let mv = vec![Move {
        rotary: None,
        kind: MoveKind::Feed,
        from: p(5.0, 5.0, 6.0),
        to: p(5.0, 5.0, floor),
        feed: Some(500.0),
        arc: None,
        axis: None,
    }];
    s.simulate(&[SimOp { moves: &mv, radius: 2.0, ball: false, cone: None, corner: None }]).unwrap();
    s
}

fn top_down() -> PreviewCamera {
    PreviewCamera::look_at(p(5.0, 5.0, 40.0), p(5.0, 5.0, 0.0), p(0.0, 1.0, 0.0), 0.5).unwrap()
}

fn oblique() -> PreviewCamera {
    PreviewCamera::look_at(p(-12.0, -15.0, 22.0), p(5.0, 5.0, 2.0), p(0.0, 0.0, 1.0), 0.45).unwrap()
}

/// Per-pixel agreement of two frames: (hit-mask agreement, depth
/// agreement among common hits, class agreement among common hits).
fn agreement(cpu: &Preview, gpu: &Preview, depth_tol: f64) -> (f64, f64, f64, usize) {
    let n = cpu.depth.len();
    let (mut mask, mut both, mut depth, mut class) = (0usize, 0usize, 0usize, 0usize);
    for i in 0..n {
        let (a, b) = (cpu.depth[i].is_finite(), gpu.depth[i].is_finite());
        if a == b {
            mask += 1;
        }
        if a && b {
            both += 1;
            if (cpu.depth[i] - gpu.depth[i]).abs() <= depth_tol {
                depth += 1;
            }
            if cpu.class[i] == gpu.class[i] {
                class += 1;
            }
        }
    }
    let frac = |k: usize, of: usize| if of == 0 { 1.0 } else { k as f64 / of as f64 };
    (frac(mask, n), frac(depth, both), frac(class, both), both)
}

fn both_eyes(stock: &Stock, target: Option<&Stock>, cam: &PreviewCamera, w: u32, h: u32) -> Option<(Preview, Preview)> {
    let gpu = match device_and_queue() {
        Some(d) => d,
        None => {
            eprintln!("skip: no GPU adapter on this machine");
            return None;
        }
    };
    let (device, queue) = (gpu.device.clone(), gpu.queue.clone());
    let cpu = stock.preview(target, cam, w, h, DEV_TOL).unwrap();
    let mut pass = TracePass::new(&device).expect("the adapter builds compute kernels");
    let lat = pack_field(stock.field().unwrap()).expect("the fixture packs");
    let tl = target.map(|t| pack_target(t.field().unwrap()).expect("the target packs"));
    pass.upload(&device, &lat, tl.as_ref()).unwrap();
    let gpu = pass.trace(&device, &queue, cam, w, h, DEV_TOL).unwrap();
    Some((cpu, gpu))
}

#[test]
fn the_gpu_sees_the_box_the_cpu_sees() {
    let stock = field_stock(p(0.0, 0.0, 0.0), p(10.0, 10.0, 5.0));
    for cam in [top_down(), oblique()] {
        let Some((cpu, gpu)) = both_eyes(&stock, None, &cam, 64, 64) else { return };
        let (mask, depth, _, both) = agreement(&cpu, &gpu, 0.1);
        assert!(both > 100, "the box fills a real share of the frame: {both}");
        assert!(mask >= 0.99, "hit masks agree on {mask:.4} of pixels");
        assert!(depth >= 0.99, "depths agree within 0.1 mm on {depth:.4} of common hits");
        assert_eq!(gpu.unresolved, cpu.unresolved, "budget bails are the same law");
    }
}

#[test]
fn the_center_ray_lands_on_the_top_in_both_eyes() {
    let stock = field_stock(p(0.0, 0.0, 0.0), p(10.0, 10.0, 5.0));
    let Some((cpu, gpu)) = both_eyes(&stock, None, &top_down(), 64, 64) else { return };
    let i = 32 * 64 + 32;
    assert!((gpu.depth[i] - 35.0).abs() < 0.1, "gpu depth {}", gpu.depth[i]);
    assert!((gpu.depth[i] - cpu.depth[i]).abs() < 0.05, "cpu {} gpu {}", cpu.depth[i], gpu.depth[i]);
    assert!(gpu.shade[i] > 0.2, "the lit top cannot shade black: {}", gpu.shade[i]);
    assert_eq!(gpu.class[i], 3.0, "no target, no verdict");
}

#[test]
fn the_cut_rides_the_kernel() {
    // The pocket's floor at 2: the GPU must read the cut layer, not
    // the uncut box top at 5.
    let stock = pocketed(2.0);
    let Some((cpu, gpu)) = both_eyes(&stock, None, &top_down(), 64, 64) else { return };
    let i = 32 * 64 + 32;
    assert!((40.0 - gpu.depth[i] - 2.0).abs() < 0.25, "gpu floor at {}", 40.0 - gpu.depth[i]);
    let (mask, depth, _, _) = agreement(&cpu, &gpu, 0.1);
    assert!(mask >= 0.99, "hit masks agree on {mask:.4}");
    assert!(depth >= 0.98, "depths agree on {depth:.4} of common hits");
}

#[test]
fn deviation_classes_agree_with_the_cpu_verdict() {
    // Clean floor over the plate, rest on the uncut top, gouge where
    // the floor dips under it: the lattice oracle must name the same
    // pixels the exact oracle names, bar a sliver at class borders.
    for (floor, centre) in [(2.0, 0.0), (1.0, 2.0)] {
        let stock = pocketed(floor);
        let target = field_stock(p(0.0, 0.0, -8.0), p(10.0, 10.0, 2.0));
        let Some((cpu, gpu)) = both_eyes(&stock, Some(&target), &top_down(), 64, 64) else { return };
        let (_, _, class, both) = agreement(&cpu, &gpu, 0.1);
        assert!(both > 100);
        assert!(class >= 0.97, "classes agree on {class:.4} of common hits (floor {floor})");
        assert_eq!(gpu.class[32 * 64 + 32], centre, "centre pixel at floor {floor}");
    }
}

#[test]
fn the_kernel_refuses_its_bad_inputs_by_name() {
    let Some(gpu) = device_and_queue() else {
        eprintln!("skip: no GPU adapter on this machine");
        return;
    };
    let (device, queue) = (gpu.device.clone(), gpu.queue.clone());
    let mut pass = TracePass::new(&device).expect("kernel builds");
    let e = pass.trace(&device, &queue, &top_down(), 8, 8, DEV_TOL).unwrap_err();
    assert!(e.contains("no lattice"), "{e}");
    let stock = field_stock(p(0.0, 0.0, 0.0), p(10.0, 10.0, 5.0));
    let mut lat = pack_field(stock.field().unwrap()).unwrap();
    lat.cut.pop();
    let e = pass.upload(&device, &lat, None).unwrap_err();
    assert!(e.contains("disagree"), "{e}");
    let lat = pack_field(stock.field().unwrap()).unwrap();
    pass.upload(&device, &lat, None).unwrap();
    let e = pass.trace(&device, &queue, &top_down(), 0, 8, DEV_TOL).unwrap_err();
    assert!(e.contains("frame size"), "{e}");
    let e = pass.trace(&device, &queue, &top_down(), 4096, 4096, DEV_TOL).unwrap_err();
    assert!(e.contains("4M"), "{e}");
}
