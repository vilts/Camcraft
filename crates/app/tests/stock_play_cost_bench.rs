//! Playback cost, measured (#3365 for #3336): the worker side of a
//! playing stock simulation on two named fixtures - the fig8 2D Offset
//! and the 3D Pocket of the user's `Milling-test.step` - at the display
//! budget that playback uses. It prints the lattice (cells, cell size),
//! the moves, the ms of each cut chunk (`FieldPlay::seek`) and of each
//! move, the ms of each lattice pack (`pack_cut`, `pack_cut_patch`), and
//! the share of each in one worker publication. No wall time is
//! asserted: the tests print readings. The GPU upload is not timed here
//! (no device); its bytes are printed.
//! Provenance: our own fixtures over the M7.1 field engine; no vendor
//! material.
//!
//! Gate map (#3365): covers app::stock_play (seek, view, take_dirty),
//! app::stock_view (PackedBase), sim::stock_sim::preview (pack_cut,
//! pack_cut_patch).

use std::path::Path;
use std::time::{Duration, Instant};

use camcraft_app::commands::{AddTool, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::links::Links;
use camcraft_app::machining2d::{AdaptiveMethod, AdaptiveTrochParams, PocketStrategy};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, AreaControls, Containment, Height, HeightReference, OpParams, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_play::{FieldPlay, PlaySeed};
use camcraft_app::stock_sim::preview::{pack_cut, pack_cut_patch};
use camcraft_app::stock_sim::{cpu_layer_cutter, merge_max};
use camcraft_app::stock_view::{PackedBase, DEFAULT_TEXELS};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// The worker's chunk law (`stock_play_worker.rs`: `CHUNK_TARGET` 15 ms,
/// `CHUNK_MIN` 1, `CHUNK_MAX` 1024, first chunk 64), copied so the bench
/// cuts the same chunks as a playing worker.
const CHUNK_TARGET: Duration = Duration::from_millis(15);
const CHUNK_MAX: usize = 1024;
/// Texels of one 8 x 8 x 8 brick (`BRICK3`): the worker sends a patch
/// only while its bricks hold fewer texels than the whole layer.
const BRICK_TEXELS: usize = 512;
/// The longest cut walk: a debug run stops here and says how far it got.
const WALK_BUDGET: Duration = Duration::from_secs(90);

fn world_with(model: &str, tool: &str) -> (World, History, String) {
    let world = new_world();
    let mut h = History::default();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(model);
    let imp = ImportModel::new(&path).expect("import the fixture");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let t = catalog().into_iter().find(|(n, _)| *n == tool).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, tool, t).expect("add tool"));
    (world, h, name)
}

/// fig8.step (x -42.5..42.5, y 0..123.6, z 0..20) in a 100 x 140 x 22 mm
/// block centred on the part: the part's box plus about 7.5 mm a side.
/// EM-0800-4FL; the 2D Offset takes the fig8 Adaptive's heights
/// (`adaptive_fig8_crossing.rs`) retyped to Offset2D.
fn fig8_offset() -> (World, String) {
    let (world, mut h, part) = world_with("fig8.step", "EM-0800-4FL");
    let e = model::find_by_name(&world, &part).expect("part entity");
    let b = model::entity_world_aabb(&world, &e).expect("part aabb");
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let at = Location { pos: Point3::new(0.5 * (b.min.x + b.max.x), 0.5 * (b.min.y + b.max.y), 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 140.0, dz: 22.0 }, at).expect("stock"));
    let adaptive = OpParams::Adaptive2D {
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(27.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(15.0),
        stepdown: 5.0,
        feed: 2400.0,
        plunge_feed: 500.0,
        climb: true,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        method: AdaptiveMethod::Spiral,
        troch: AdaptiveTrochParams::default(),
        tilt: None,
        blend_turns: false,
        blend_radius: None,
        links: Links::PLANAR,
        rest: None,
        corner_relief: Default::default(),
        pass_start: Default::default(),
        picks: Default::default(),
    };
    let params = operation::retype_params(&adaptive, OpType::Offset2D as usize);
    assert!(matches!(params, OpParams::Offset2D { .. }), "the retype gives a 2D Offset");
    h.execute(&world, CreateOperation::new(&world, "2D Offset1", "EM-0800-4FL", SrcRef::Entity(part), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "2D Offset1").expect("generate"));
    (world, "fig8 2D Offset, stock 100 x 140 x 22 mm, EM-0800-4FL".into())
}

/// The 3D Pocket of `pocket3d_hsm.rs`: Milling-test.step in a 70 x 70 x
/// 27 mm block from Z -7, EM-1000-4FL, the default OffsetRings plan,
/// the tool inside the part's silhouette.
fn pocket3d() -> (World, String) {
    let (world, mut h, part) = world_with("Milling-test.step", "EM-1000-4FL");
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc).expect("stock"));
    let params = OpParams::Pocket3D {
        pass_start: Default::default(),
        links: Links::ROUGH3D,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 20.5),
        retract_z: Some(Height::of(HeightReference::TopOfStock, 2.0)),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::OffsetRings,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: false,
        method: AdaptiveMethod::Spiral,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "Op1", "EM-1000-4FL", SrcRef::Entity(part), params.clone()).expect("create"));
    let inside = AreaControls { containment: Some(Containment::Inside), ..Default::default() };
    h.execute(&world, EditOperationParams::with_area(&world, "Op1", params, inside).expect("area"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    (world, "3D Pocket (OffsetRings), Milling-test.step, stock 70 x 70 x 27 mm, EM-1000-4FL".into())
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// Mean, median and max of `v` in ms.
fn spread(v: &[Duration]) -> (f64, f64, f64) {
    let mut s: Vec<f64> = v.iter().copied().map(ms).collect();
    s.sort_by(f64::total_cmp);
    let mean = s.iter().sum::<f64>() / s.len().max(1) as f64;
    let med = s.get(s.len() / 2).copied().unwrap_or(0.0);
    (mean, med, s.last().copied().unwrap_or(0.0))
}

/// Walk the whole program as a playing worker does: a chunk of moves
/// (`seek`), then one publication (`take_dirty`, `view`, then a patch
/// pack, or the whole layer when the patch is not smaller), and print
/// the readings.
fn bench(world: &World, what: &str, budget: f64) {
    let mode = if cfg!(debug_assertions) { "debug" } else { "release" };
    let t0 = Instant::now();
    let seed = PlaySeed::gather(world, false, budget).expect("seed");
    let mut play = FieldPlay::from_seed(seed).expect("play");
    let build = t0.elapsed();
    let t1 = Instant::now();
    let base = {
        let v = play.view();
        PackedBase::of(&v.stock, None).map(|(b, _)| b).expect("the stock packs")
    };
    let base_pack = t1.elapsed();
    let meta = &base.meta;
    let texels = meta.dims.iter().map(|&d| d as usize).product::<usize>();
    let moves = play.total();
    let stats0 = play.view().stock.field().map(|f| f.cut_stats()).unwrap_or_default();
    println!("== {what} [{mode}]");
    println!(
        "lattice: dims {:?} = {texels} texels per layer, cell {:.3} mm; program {moves} moves; play build {:.0} ms, base pack {:.0} ms",
        meta.dims, meta.cell, ms(build), ms(base_pack)
    );
    let (mut chunk, mut chunks, mut views, mut patches, mut fulls) = (64usize, Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut chunk_moves, mut patch_bytes, mut full_packs_sent) = (Vec::new(), 0usize, 0usize);
    let walk = Instant::now();
    let _ = play.take_dirty(); // the first take restarts the ledger, as the worker's uncut frame does
    while play.at() < moves && walk.elapsed() < WALK_BUDGET {
        let from = play.at();
        let tc = Instant::now();
        play.seek(from.saturating_add(chunk).min(moves)).expect("seek");
        let dt = tc.elapsed();
        chunks.push(dt);
        chunk_moves.push(play.at() - from);
        if dt < CHUNK_TARGET / 2 {
            chunk = chunk.saturating_mul(2).min(CHUNK_MAX);
        } else if dt > CHUNK_TARGET * 2 {
            chunk = (chunk / 2).max(1);
        }
        // One publication, the worker's `snapshot`.
        let tv = Instant::now();
        let dirty = play.take_dirty();
        let view = play.view();
        views.push(tv.elapsed());
        let f = view.stock.field().expect("a field stock");
        match dirty.filter(|s| s.len().saturating_mul(BRICK_TEXELS) < texels) {
            Some(set) => {
                let tp = Instant::now();
                let p = pack_cut_patch(f, meta, &set);
                patches.push(tp.elapsed());
                patch_bytes += p.bytes();
            }
            None => {
                let tp = Instant::now();
                let cut = pack_cut(f, meta);
                fulls.push(tp.elapsed());
                full_packs_sent += cut.len() * 4;
            }
        }
        // The whole layer as well, every 8th chunk: what a repack on the
        // render path (`eye.rs` `full_cut`) or a landing frame costs.
        if chunks.len() % 8 == 1 {
            let tp = Instant::now();
            let cut = pack_cut(f, meta);
            fulls.push(tp.elapsed());
            std::hint::black_box(cut);
        }
    }
    let walked = play.at();
    let cut_total: Duration = chunks.iter().sum();
    let stats = play.view().stock.field().map(|f| f.cut_stats()).unwrap_or_default();
    let per_move = ms(cut_total) / walked.max(1) as f64;
    let (cm, cmed, cx) = spread(&chunks);
    let (vm, _, vx) = spread(&views);
    let (pm, pmed, px) = spread(&patches);
    let (fm, fmed, fx) = spread(&fulls);
    let mean_moves = walked as f64 / chunks.len().max(1) as f64;
    println!(
        "walked {walked}/{moves} moves in {} chunks ({:.1} s cut wall{})",
        chunks.len(),
        cut_total.as_secs_f64(),
        if walked < moves { ", stopped at the walk budget" } else { "" }
    );
    println!("cut chunk (seek): mean {cm:.2} ms, median {cmed:.2} ms, max {cx:.2} ms, mean {mean_moves:.1} moves per chunk");
    println!("cut per move: {per_move:.3} ms; {:.0} moves/s", 1e3 / per_move.max(1e-9));
    // How the cost spreads over the program: ms per move in each
    // quarter of the moves walked, and the slowest chunk's share.
    let mut quarters = [(Duration::ZERO, 0usize); 4];
    let mut done = 0usize;
    for (dt, n) in chunks.iter().zip(&chunk_moves) {
        let q = (done * 4 / walked.max(1)).min(3);
        quarters[q].0 += *dt;
        quarters[q].1 += n;
        done += n;
    }
    let qs: Vec<String> = quarters.iter().map(|(d, n)| format!("{:.3}", ms(*d) / (*n).max(1) as f64)).collect();
    println!("cut ms per move by quarter of the walk: {}; slowest chunk {:.0} % of the cut wall", qs.join(" / "), 100.0 * cx / ms(cut_total).max(1e-9));
    println!(
        "cut counters per move: cells evaluated {:.0}, bricks visited {:.1}, touched {:.1}, skipped {:.1}",
        (stats.cells_evaluated - stats0.cells_evaluated) as f64 / walked.max(1) as f64,
        (stats.bricks_visited - stats0.bricks_visited) as f64 / walked.max(1) as f64,
        (stats.bricks_touched - stats0.bricks_touched) as f64 / walked.max(1) as f64,
        (stats.bricks_skipped - stats0.bricks_skipped) as f64 / walked.max(1) as f64,
    );
    println!("take_dirty + view: mean {vm:.2} ms, max {vx:.2} ms");
    println!("pack_cut_patch: {} packs, mean {pm:.2} ms, median {pmed:.2} ms, max {px:.2} ms, mean {:.0} bytes", patches.len(), patch_bytes as f64 / patches.len().max(1) as f64);
    println!("pack_cut (whole layer): {} packs, mean {fm:.2} ms, median {fmed:.2} ms, max {fx:.2} ms, {} bytes", fulls.len(), texels * 4);
    let pack_pub = if patches.is_empty() { fm } else { pm };
    let frame = cm + vm + pack_pub;
    println!(
        "publication (chunk + view + pack): {frame:.2} ms = cut {:.0} %, view {:.0} %, pack {:.0} %; upload not timed (patch {:.0} B, whole layer {} B; {} whole layers sent)",
        100.0 * cm / frame.max(1e-9),
        100.0 * vm / frame.max(1e-9),
        100.0 * pack_pub / frame.max(1e-9),
        patch_bytes as f64 / patches.len().max(1) as f64,
        texels * 4,
        full_packs_sent / (texels * 4).max(1)
    );
    println!(
        "SUMMARY|{mode}|{what}|texels {texels}|cell {:.3}|moves {moves}|walked {walked}|chunk ms {cm:.2}|moves/chunk {mean_moves:.1}|ms/move {per_move:.3}|patch ms {pm:.2}|full ms {fm:.2}|view ms {vm:.2}|cut % {:.0}|pack % {:.0}",
        meta.cell,
        100.0 * cm / frame.max(1e-9),
        100.0 * pack_pub / frame.max(1e-9)
    );
}

#[test]
#[ignore = "heavy lane (playback perf bench): fig8 2D Offset generation and a full playback walk, minutes in debug - run with --release -- --ignored --nocapture"]
fn playback_cost_on_the_fig8_2d_offset() {
    let (world, what) = fig8_offset();
    bench(&world, &what, DEFAULT_TEXELS);
}

#[test]
#[ignore = "heavy lane (playback perf bench): 3D Pocket generation on Milling-test.step and a full playback walk, minutes in debug - run with --release -- --ignored --nocapture"]
fn playback_cost_on_the_3d_pocket() {
    let (world, what) = pocket3d();
    bench(&world, &what, DEFAULT_TEXELS);
}

/// The same walks at a quarter and a half of the display budget: how
/// the cut and the pack grow with the cell count (the default run is
/// the tests above).
#[test]
#[ignore = "heavy lane (playback perf bench): both fixtures at 1/4 and 1/2 of the display budget, release only (debug takes many minutes) - run with --release -- --ignored --nocapture"]
fn playback_cost_against_the_cell_count() {
    for (world, what) in [fig8_offset(), pocket3d()] {
        for div in [4.0, 2.0] {
            bench(&world, &format!("{what}, budget 1/{div}"), DEFAULT_TEXELS / div);
        }
    }
}

/// The layer mode's walk (#2629 S3b, for #3336): the ops with an f32
/// twin leave the f64 field and are cut onto the dense layer by the CPU
/// twin law (a GPU cutter is the UI's; no device here). One publication
/// each `step` moves, as a playing transport asks: the field's `seek`,
/// the layer cut (`layer_frame`), then the whole-layer pack and merge.
fn bench_layered(world: &World, what: &str, budget: f64, step: usize) {
    let mode = if cfg!(debug_assertions) { "debug" } else { "release" };
    let seed = PlaySeed::gather(world, false, budget).expect("seed");
    let mut play = FieldPlay::from_seed(seed).expect("play");
    let base = PackedBase::of(&play.view().stock, None).map(|(b, _)| b).expect("the stock packs");
    let kept = play.set_layer(base.meta.clone(), cpu_layer_cutter()).expect("the layer mode");
    let moves = play.total();
    let (mut seeks, mut layers, mut packs) = (Vec::new(), Vec::new(), Vec::new());
    let walk = Instant::now();
    while play.at() < moves && walk.elapsed() < WALK_BUDGET {
        let t = Instant::now();
        play.seek(play.at().saturating_add(step).min(moves)).expect("seek");
        seeks.push(t.elapsed());
        let t = Instant::now();
        let layer = play.layer_frame().expect("a layer frame");
        layers.push(t.elapsed());
        let t = Instant::now();
        let view = play.view();
        let mut whole = pack_cut(view.stock.field().expect("a field stock"), &base.meta);
        if let Some(layer) = &layer {
            merge_max(&mut whole, layer).expect("the layer fits the picture");
        }
        packs.push(t.elapsed());
        std::hint::black_box(whole);
    }
    let walked = play.at();
    let (sm, _, sx) = spread(&seeks);
    let (lm, lmed, lx) = spread(&layers);
    let (pm, _, px) = spread(&packs);
    let cut: Duration = seeks.iter().chain(&layers).sum();
    let per_move = ms(cut) / walked.max(1) as f64;
    println!("== {what}, layer mode, {step} moves a publication [{mode}]; ops kept on the field: {kept:?}; layer on at the end: {}", play.layer_on());
    println!("walked {walked}/{moves} moves in {} publications", seeks.len());
    println!("field seek: mean {sm:.2} ms, max {sx:.2} ms; layer cut: mean {lm:.2} ms, median {lmed:.2} ms, max {lx:.2} ms; pack + merge: mean {pm:.2} ms, max {px:.2} ms");
    println!(
        "SUMMARY|{mode}|{what}, layer mode|step {step}|moves {moves}|walked {walked}|ms/move {per_move:.3}|publication ms {:.2}|layer ms {lm:.2}|pack+merge ms {pm:.2}",
        sm + lm + pm
    );
}

/// The layer mode (the CPU twin law as the cutter) on both fixtures, at
/// the display budget: 1, 8 and 64 moves in each publication.
#[test]
#[ignore = "heavy lane (playback perf bench): both fixtures in the layer mode, release only - run with --release -- --ignored --nocapture"]
fn playback_cost_in_the_layer_mode() {
    for (world, what) in [fig8_offset(), pocket3d()] {
        for step in [1, 8, 64] {
            bench_layered(&world, &what, DEFAULT_TEXELS, step);
        }
    }
}
