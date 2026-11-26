//! #3336 (GPU stock sim S3c of #2629): the layer cutter of the Simulate
//! view on a real device. The playback's layer, cut by `gpu_cutter` on
//! the device of this test, is the layer of the CPU twin law within the
//! twin's claim margin at each seek; and a device that cannot compute
//! gives the CPU law's layer, never an error. One test, one device
//! (#3589, #3605: no two devices alive in a process). Adapter and skip
//! policy: `headless_gpu` (#615 - loud SKIP, `CAMCRAFT_REQUIRE_GPU=1`
//! fails). Provenance: our own fixtures (the staged face of
//! `stock_play_picture`) over the M7.1 field engine.
//!
//! Gate map (M4.4): covers ui::play_layer (gpu_cutter, cut_input).

use std::sync::Arc;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_play::{FieldPlay, PlaySeed};
use camcraft_app::stock_sim::{cpu_layer_cutter, LayerCutter};
use camcraft_app::stock_view::PackedBase;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_ui::headless_gpu::{device_for_test, HeadlessGpu};
use camcraft_ui::play_layer::gpu_cutter;

/// A staged face over a 12 x 12 x 20 block (the `stock_play_picture`
/// fixture).
fn faced_doc() -> World {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 12.0, dy: 12.0, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    let (lo, hi) = (Point3::new(-6.0, -6.0, 0.0), Point3::new(6.0, 6.0, 14.0));
    history.execute(&world, ImportModel::from_mesh("part", cube(lo, hi)));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 1.5,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(14.0),
        top_z: Some(Height::fixed(19.0)),
        stepdown: Some(2.5),
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
        blend_radius: None,
        picks: Default::default(),
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    world
}

/// The twin's claim margin at this fixture's scale, with room (the
/// bound of `stock_play_picture`): far under a cell.
const TWIN_TOL: f32 = 4.0 * 32.0 * f32::EPSILON * 100.0 + 1e-4;

/// A playback of the face in the layer mode with `cutter`.
fn layered(world: &World, cutter: Arc<LayerCutter>) -> FieldPlay {
    let seed = PlaySeed::gather(world, false, 12.0 * 12.0 * 20.0).expect("seed");
    let mut play = FieldPlay::from_seed(seed).expect("play");
    let (base, _) = PackedBase::of(&play.view().stock, None).expect("the stock packs");
    let kept = play.set_layer(base.meta.clone(), cutter).expect("the layer mode");
    assert!(kept.is_empty(), "the face op is cut by the layer: {kept:?}");
    play
}

/// The lattice of the playback's packed frame.
fn cpu_meta(world: &World) -> camcraft_app::stock_sim::preview::LatticeMeta {
    let seed = PlaySeed::gather(world, false, 12.0 * 12.0 * 20.0).expect("seed");
    let play = FieldPlay::from_seed(seed).expect("play");
    PackedBase::of(&play.view().stock, None).expect("the stock packs").0.meta
}

/// The layers of `play` at each position; the mode must stay on.
fn layers(play: &mut FieldPlay, positions: &[(usize, f64)]) -> Vec<Arc<Vec<f32>>> {
    positions
        .iter()
        .map(|&(k, f)| {
            play.seek_to(k, f).expect("seek");
            let layer = play.layer_frame().expect("a frame").expect("the layer mode answers");
            assert!(play.layer_on(), "move {k}+{f}: the cutter must not fault");
            layer
        })
        .collect()
}

fn worst(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

#[test]
fn the_gpu_cutter_draws_the_cpu_twin_layer_and_a_device_without_compute_takes_the_cpu_law() {
    let gpu = match device_for_test("play_layer_gpu") {
        Ok(Some(gpu)) => gpu,
        Ok(None) => return, // the loud SKIP is printed
        Err(err) => panic!("{err}"),
    };
    let no_compute = HeadlessGpu {
        device: gpu.device.clone(),
        queue: gpu.queue.clone(),
        kind: gpu.kind,
        name: gpu.name.clone(),
        compute: Err("test: this device has no compute".into()),
    };
    let computes = gpu.compute.is_ok();
    let world = faced_doc();
    let mut cpu = layered(&world, cpu_layer_cutter());
    let total = cpu.total();
    let step = (total / 6).max(1);
    let mut positions: Vec<(usize, f64)> = (step..total).step_by(step).map(|k| (k, if k % 2 == 0 { 0.0 } else { 0.5 })).collect();
    positions.extend([(total, 0.0), (step, 0.0)]); // the end, then a backward seek
    let want = layers(&mut cpu, &positions);
    let floor = want[0].iter().copied().fold(f32::INFINITY, f32::min);
    assert!(want.iter().any(|l| l.iter().any(|v| *v > floor)), "the fixture cuts the layer");

    // One batch through the same mapping, to read which law answers: on a
    // device that computes it must be the kernel, not a silent fallback.
    if computes {
        use camcraft_app::cl::{Move, MoveKind};
        use camcraft_app::stock_sim::{twin_items, twin_work, SdfTool};
        let meta = cpu_meta(&world);
        let (from, to) = (Point3::new(-3.0, -3.0, 16.0), Point3::new(3.0, 2.0, 15.0));
        let moves = [Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(600.0), arc: None, axis: None }];
        let work = twin_work(&moves, &SdfTool::Ball { r: 2.0 }, &meta).expect("the twin cuts a ball move");
        let input = camcraft_ui::play_layer::cut_input(&meta, &twin_items(&work, &meta).expect("items"));
        let kernel = camcraft_gpu::CutKernel::new(&gpu.device).expect("the kernel compiles");
        let out = camcraft_gpu::cut_onto_or_cpu_on(Ok((&gpu, &kernel)), &input, &want[0]).expect("a batch");
        assert!(matches!(out.path, camcraft_gpu::CutPath::Gpu { .. }), "the kernel answers on {}: {:?}", gpu.name, out.path);
    }
    let got = layers(&mut layered(&world, gpu_cutter(gpu)), &positions);
    for ((&(k, f), g), w) in positions.iter().zip(&got).zip(&want) {
        let d = worst(g, w);
        assert!(d <= TWIN_TOL, "move {k}+{f}: the GPU layer is {d} mm from the CPU twin layer (bound {TWIN_TOL})");
    }
    eprintln!("play_layer_gpu: {} positions compared; the device computes: {computes}", positions.len());

    // The fault: no compute on the device. The CPU law of camcraft-gpu
    // answers each batch, and the layer mode stays on.
    let fallen = layers(&mut layered(&world, gpu_cutter(no_compute)), &positions);
    for ((&(k, f), g), w) in positions.iter().zip(&fallen).zip(&want) {
        let d = worst(g, w);
        assert!(d <= TWIN_TOL, "move {k}+{f}: the fallback layer is {d} mm from the CPU twin layer");
    }
}
