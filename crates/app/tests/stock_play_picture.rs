//! #2611: the stock the eye draws during playback shows the removal.
//! The UI's playback path is replayed headless: the transport asks the
//! worker for positions one after the other, each published frame is
//! composed into one resident cut layer the way the GPU eye does it
//! (a whole layer replaces, a patch over the resident frame writes its
//! runs, any other patch repacks the frame whole), and the drawn
//! material is counted as `max(mesh, cut) < 0` texels. No pixels.
//! Provenance: our own fixtures over the M7.1 field engine; nothing
//! from vendor binaries.
//!
//! #2629 (GPU stock sim S3b) at the end: the layer mode draws the field's
//! picture within the twin's claim margin, and a layer fault falls back
//! to the field picture.
//!
//! Gate map (M4.4): covers app::stock_play, app::stock_play_cut, app::stock_play_worker, app::stock_play_worker_frame, app::stock_play_worker_precalc, app::stock_view.

use std::time::{Duration, Instant};

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_play::{FieldPlay, PlaySeed, Precalc, PRECALC_BUDGET};
use camcraft_app::stock_play_worker::{PlayFrame, PlayWorker};
use camcraft_app::stock_sim::preview::pack_cut;
use camcraft_app::stock_sim::{cpu_layer_cutter, LayerCutter};
use camcraft_app::stock_view::{field_view_with, CutLayer, PackedBase};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// A staged face over a `side` x `side` block (the `stock_play_precalc`
/// fixture).
fn faced_doc(side: f64, stepover: f64, stepdown: Option<f64>) -> World {
    faced_doc_with(side, stepover, stepdown, false)
}

/// `faced_doc`; `blend` joins the passes with arcs (no f32 twin, #2628).
fn faced_doc_with(side: f64, stepover: f64, stepdown: Option<f64>, blend: bool) -> World {
    let world = new_world();
    let mut history = History::default();
    let half = side / 2.0;
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: side, dy: side, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    let (lo, hi) = (Point3::new(-half, -half, 0.0), Point3::new(half, half, 14.0));
    history.execute(&world, ImportModel::from_mesh("part", cube(lo, hi)));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(14.0),
        top_z: stepdown.map(|_| Height::fixed(19.0)),
        stepdown,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: blend, blend_radius: None,
        picks: Default::default(),
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    world
}

/// Texels drawn as material: `max(mesh, cut) < 0`.
fn solid(mesh: &[f32], cut: &[f32]) -> usize {
    mesh.iter().zip(cut).filter(|(m, c)| m.max(**c) < 0.0).count()
}

/// The eye's resident state: the frame number it holds and its cut layer.
struct Resident {
    seq: u64,
    cut: Vec<f32>,
}

/// Take `frame` into the resident layer as `eye.rs` `rewrite_cut` /
/// `upload` do.
fn compose(res: &mut Option<Resident>, frame: &PlayFrame) {
    let packed = frame.view.packed.as_ref().expect("the worker packs its frames for the eye");
    let whole = |p: &camcraft_app::stock_view::Packed| match &p.cut {
        CutLayer::Full(c) => c.as_ref().clone(),
        CutLayer::Patch { .. } => pack_cut(frame.view.stock.field().expect("a field stock"), &p.base.meta),
    };
    match res {
        Some(r) if packed.patches(r.seq) => {
            let CutLayer::Patch { patch, .. } = &packed.cut else { unreachable!("patches() is only true for a patch") };
            patch.apply(&mut r.cut).expect("the patch fits the layer");
            r.seq = packed.seq;
        }
        _ => *res = Some(Resident { seq: packed.seq, cut: whole(packed) }),
    }
}

fn wait_precalc(w: &PlayWorker) -> Precalc {
    let t0 = Instant::now();
    loop {
        let p = w.precalc();
        if !matches!(p, Precalc::Running { .. }) || t0.elapsed() > Duration::from_secs(120) {
            return p;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Play the program as the transport does - one position after the
/// other, every frame the worker publishes taken and composed - and
/// return the drawn material at each landed position with the frame.
fn play_through(w: &PlayWorker, positions: &[(usize, f64)], res: &mut Option<Resident>) -> Vec<(usize, PlayFrame)> {
    let mut landed = Vec::new();
    for &(k, f) in positions {
        w.request_at(k, f);
        let (mut seen, t0) = (w.published(), Instant::now());
        loop {
            assert!(t0.elapsed() < Duration::from_secs(120), "no frame at move {k}");
            if let Some(fr) = w.take() {
                let fr = fr.expect("the worker cuts the face program");
                compose(res, &fr);
                if fr.at == k && fr.frac.to_bits() == f.to_bits() {
                    landed.push((k, fr));
                    break;
                }
            }
            seen = w.wait_published(seen, Duration::from_millis(100));
        }
    }
    landed
}

fn check_playback(world: &World, cells: f64) {
    let seed = || PlaySeed::gather(world, false, cells).expect("seed");
    let w = PlayWorker::spawn_precalc(seed(), PRECALC_BUDGET).expect("worker");
    assert!(matches!(wait_precalc(&w), Precalc::Ready { .. }), "the calculation finishes: {:?}", w.precalc());
    let total = w.index().total();
    let uncut = FieldPlay::from_seed(seed()).expect("play").view().stock;
    let (ub, ucut) = PackedBase::of(&uncut, None).expect("packs");
    let full = field_view_with(world, cells).expect("deviate view");
    let (fb, fcut) = PackedBase::of(&full.stock, None).expect("packs");
    let (start, finished) = (solid(&ub.mesh, &ucut), solid(&fb.mesh, &fcut));
    assert!(start > finished, "the face removes material: uncut {start}, finished {finished} texels");
    // The transport from the start to the end, a few moves a frame, the
    // move in flight half drawn on the way.
    let step = (total / 40).max(1);
    let mut positions: Vec<(usize, f64)> = (0..total).step_by(step).map(|k| (k, if k % 2 == 0 { 0.0 } else { 0.5 })).collect();
    positions.push((total, 0.0));
    let mut res = None;
    let landed = play_through(&w, &positions, &mut res);
    let r = res.as_ref().expect("frames were drawn");
    let (_, last) = landed.last().expect("the end landed");
    let base = &last.view.packed.as_ref().expect("packed").base;
    let drawn = solid(&base.mesh, &r.cut);
    assert!(drawn < start, "the drawn stock at the end shows a removal: {drawn} texels drawn, uncut {start}");
    assert_eq!(drawn, finished, "the drawn stock at the end is the finished stock");
    // Played again from the start: at each landed position the drawn
    // layer is that frame's own stock, and the removal grows.
    let mut res = None;
    let mut prev = start;
    for &(k, f) in &positions {
        play_through(&w, &[(k, f)], &mut res);
        let r = res.as_ref().expect("a frame was drawn");
        let drawn = solid(&base.mesh, &r.cut);
        assert!(drawn <= prev, "move {k}+{f}: the drawn material only goes down ({drawn} after {prev})");
        prev = drawn;
    }
    assert_eq!(prev, finished, "the replay ends at the finished stock");
}

/// The small face at about 1 mm cells.
#[test]
fn the_drawn_stock_shows_the_removal_as_the_program_plays() {
    check_playback(&faced_doc(12.0, 1.5, Some(2.5)), 12.0 * 12.0 * 20.0);
}

/// A longer program at the viewport's own display budget.
#[test]
fn the_drawn_stock_shows_the_removal_at_the_display_budget() {
    check_playback(&faced_doc(24.0, 0.8, Some(2.5)), camcraft_app::stock_view::DEFAULT_TEXELS);
}

/// The largest gap between two drawn layers, `max(mesh, cut)` clamped at
/// `-cap` (under it nothing shows; in the air the mesh wins), and the
/// largest amount by which `layer` draws more removal than `field`.
fn gaps(mesh: &[f32], layer: &[f32], field: &[f32], cap: f32) -> (f32, f32) {
    mesh.iter().zip(layer.iter().zip(field)).fold((0.0f32, 0.0f32), |(gap, over), (m, (l, f))| {
        let (l, f) = (m.max(*l).max(-cap), m.max(*f).max(-cap));
        (gap.max((l - f).abs()), over.max(l - f))
    })
}

/// The twin's claim margin at this fixture's scale, with room: 32 f32
/// epsilons of a tile scale under 100 mm, doubled, plus the flat's
/// golden-section term (1e-6 of a move under 30 mm). Far under a cell.
const TWIN_TOL: f32 = 4.0 * 32.0 * f32::EPSILON * 100.0 + 1e-4;

#[test]
fn the_layer_playback_draws_the_field_picture_within_the_twin_margin() {
    let world = faced_doc(12.0, 1.5, Some(2.5));
    let cells = 12.0 * 12.0 * 20.0;
    let seed = || PlaySeed::gather(&world, false, cells).expect("seed");
    let field = PlayWorker::spawn(seed()).expect("the field worker");
    let layered = PlayWorker::spawn_layered(seed(), cpu_layer_cutter()).expect("the layer worker");
    let total = field.index().total();
    let step = (total / 12).max(1);
    let mut positions: Vec<(usize, f64)> = (0..total).step_by(step).map(|k| (k, if k % 2 == 0 { 0.0 } else { 0.5 })).collect();
    positions.extend([(total, 0.0), (step, 0.0)]); // the end, then a backward seek
    let (mut fres, mut lres) = (None, None);
    let mut differs = false;
    for &(k, f) in &positions {
        play_through(&field, &[(k, f)], &mut fres);
        let landed = play_through(&layered, &[(k, f)], &mut lres);
        let (_, frame) = landed.last().expect("the layer worker landed");
        let packed = frame.view.packed.as_ref().expect("packed");
        assert!(matches!(packed.cut, CutLayer::Full(_)), "move {k}+{f}: a layer frame is always whole");
        let (fc, lc) = (&fres.as_ref().expect("field").cut, &lres.as_ref().expect("layer").cut);
        let (gap, over) = gaps(&packed.base.mesh, lc, fc, packed.base.meta.cap as f32);
        assert!(over <= 1e-6, "move {k}+{f}: the layer removes {over} mm more than the field");
        assert!(gap <= TWIN_TOL, "move {k}+{f}: the layer lags the field by {gap} mm (bound {TWIN_TOL})");
        let uncut = solid(&packed.base.mesh, &vec![f32::MIN; lc.len()]);
        differs |= solid(&packed.base.mesh, lc) < uncut;
    }
    assert!(differs, "the layer playback shows a removal");
}

/// A cutter that answers `ok` batches, then fails (a GPU kernel that
/// faults in the middle of the playback).
fn failing_after(ok: usize) -> std::sync::Arc<LayerCutter> {
    let cpu = cpu_layer_cutter();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    std::sync::Arc::new(move |meta: &_, items: &_, base: &[f32]| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= ok {
            return Err("test: the GPU device was lost".to_string());
        }
        cpu(meta, items, base)
    })
}

#[test]
fn a_layer_fault_falls_back_to_the_field_picture_bit_for_bit() {
    let world = faced_doc(12.0, 1.5, Some(2.5));
    let seed = || PlaySeed::gather(&world, false, 12.0 * 12.0 * 20.0).expect("seed");
    let mut plain = FieldPlay::from_seed(seed()).expect("play");
    let (base, _) = PackedBase::of(&plain.view().stock, None).expect("packs");
    for ok in [0, 2] {
        let mut play = FieldPlay::from_seed(seed()).expect("play");
        let kept = play.set_layer(base.meta.clone(), failing_after(ok)).expect("the layer mode");
        assert!(kept.is_empty(), "the face op is cut by the layer: {kept:?}");
        let k = play.total() / 2;
        // the first frame (and, for ok = 2, a second one) cuts on the layer
        play.seek_to(k / 2, 0.0).expect("seek");
        let first = play.layer_frame().expect("a frame");
        assert_eq!(first.is_some(), ok > 0, "ok = {ok}: the layer answers until it faults");
        play.seek_to(k, 0.5).expect("seek");
        let late = play.layer_frame().expect("a fault is a fallback, not an error");
        assert!(late.is_none() && !play.layer_on(), "ok = {ok}: the fault turns the layer mode off");
        plain.seek_to(k, 0.5).expect("seek");
        let f = |p: &FieldPlay| pack_cut(p.view().stock.field().expect("a field stock"), &base.meta);
        let (got, want) = (f(&play), f(&plain));
        assert!(got.iter().zip(&want).all(|(a, b)| a.to_bits() == b.to_bits()), "ok = {ok}: the rebuilt field is the field picture");
        assert_eq!(play.at(), k);
    }
}

#[test]
fn the_layer_mode_is_set_once_before_the_first_cut() {
    let world = faced_doc(12.0, 1.5, Some(2.5));
    let seed = || PlaySeed::gather(&world, false, 12.0 * 12.0 * 20.0).expect("seed");
    let mut play = FieldPlay::from_seed(seed()).expect("play");
    let (base, _) = PackedBase::of(&play.view().stock, None).expect("packs");
    play.seek(3).expect("seek");
    let err = play.set_layer(base.meta.clone(), cpu_layer_cutter()).expect_err("after a cut");
    assert!(err.contains("before the first cut"), "{err}");
    let mut fresh = FieldPlay::from_seed(seed()).expect("play");
    fresh.set_layer(base.meta.clone(), cpu_layer_cutter()).expect("the first time");
    assert!(fresh.set_layer(base.meta.clone(), cpu_layer_cutter()).is_err(), "a second time");
}

/// #3336 (S3c): the UI's start. The layer worker that calculates before
/// playback ends its calculation ready and draws the field's picture.
#[test]
fn the_layer_playback_with_the_calculation_before_playback_draws_the_field_picture() {
    let world = faced_doc(12.0, 1.5, Some(2.5));
    let seed = || PlaySeed::gather(&world, false, 12.0 * 12.0 * 20.0).expect("seed");
    let field = PlayWorker::spawn_precalc(seed(), PRECALC_BUDGET).expect("the field worker");
    let layered = PlayWorker::spawn_layered_precalc(seed(), PRECALC_BUDGET, cpu_layer_cutter()).expect("the layer worker");
    for w in [&field, &layered] {
        assert!(matches!(wait_precalc(w), Precalc::Ready { .. }), "the calculation finishes: {:?}", w.precalc());
    }
    let total = field.index().total();
    let (mut fres, mut lres) = (None, None);
    for pos in [(total / 3, 0.5), (total, 0.0), (total / 5, 0.0)] {
        play_through(&field, &[pos], &mut fres);
        let landed = play_through(&layered, &[pos], &mut lres);
        let packed = landed.last().and_then(|(_, f)| f.view.packed.clone()).expect("a packed layer frame");
        assert!(matches!(packed.cut, CutLayer::Full(_)), "{pos:?}: a layer frame is always whole");
        let (fc, lc) = (&fres.as_ref().expect("field").cut, &lres.as_ref().expect("layer").cut);
        let (gap, over) = gaps(&packed.base.mesh, lc, fc, packed.base.meta.cap as f32);
        assert!(over <= 1e-6 && gap <= TWIN_TOL, "{pos:?}: the layer picture is {gap} mm from the field picture, {over} mm over");
        assert!(solid(&packed.base.mesh, lc) < solid(&packed.base.mesh, &vec![f32::MIN; lc.len()]), "{pos:?}: a removal shows");
    }
}

/// #3336: a program with no op for the layer keeps the layer mode off.
/// A layer frame is always the whole layer; the field's play frames are
/// patches. An arc has no f32 twin until #2628.
#[test]
fn a_program_with_no_twin_op_keeps_the_layer_mode_off() {
    let world = faced_doc_with(12.0, 1.5, Some(2.5), true);
    let seed = PlaySeed::gather(&world, false, 12.0 * 12.0 * 20.0).expect("seed");
    let mut play = FieldPlay::from_seed(seed).expect("play");
    let (base, _) = PackedBase::of(&play.view().stock, None).expect("packs");
    let kept = play.set_layer(base.meta.clone(), failing_after(0)).expect("the layer mode is asked");
    assert_eq!(kept.len(), 1, "the blended face stays on the field: {kept:?}");
    assert!(kept[0].contains("arc"), "the reason names the arc: {kept:?}");
    assert!(!play.layer_on(), "no op for the layer: the mode is off");
    play.seek_to(play.total() / 2, 0.0).expect("seek");
    assert!(play.layer_frame().expect("no layer, no fault").is_none(), "the frame is the field's own");
}
