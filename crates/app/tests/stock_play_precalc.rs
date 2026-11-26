//! #2181: the stock removal calculated before playback. After the
//! calculation the stock at each program position IS the live cut's,
//! bit for bit; a jump forward or back replays from a keyframe and cuts
//! at most one stride; the keyframe store stays inside its memory
//! budget; a stopped calculation is "incomplete" by name and the live
//! cut takes over; and the post gate reads the same before and after.
//! Provenance: our own fixtures over the M7.1 field engine; nothing
//! from vendor binaries.
//!
//! Gate map (M4.4): covers app::stock_play_precalc, app::stock_play_worker_precalc, app::stock_play, app::stock_play_worker, camcraft_sim::play_keys, camcraft_sim::play_precalc.

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
use camcraft_app::stock_sim::Stock;
use camcraft_app::stock_view::{field_view_with, CutLayer, PackedBase};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// A staged face over a `side` x `side` block: the `stock_play` suite's
/// fixture with the density as a parameter.
fn faced_doc(side: f64, stepover: f64, stepdown: Option<f64>) -> World {
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
        blend_turns: false, blend_radius: None,
        picks: Default::default(),
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    world
}

/// ~1 mm cells over the 12 mm block: several bricks, a second or two
/// of debug cutting.
const CELLS: f64 = 12.0 * 12.0 * 20.0;

fn small() -> World {
    faced_doc(12.0, 1.5, Some(2.5))
}

/// The cut layer as bits, packed on one shared lattice.
fn bits(s: &Stock, base: &PackedBase) -> Vec<u32> {
    pack_cut(s.field().expect("a field stock"), &base.meta).iter().map(|x| x.to_bits()).collect()
}

fn precalculated(world: &World, keys: usize, min_stride: usize, budget: usize) -> FieldPlay {
    let mut play = FieldPlay::from_seed(PlaySeed::gather(world, false, CELLS).expect("seed")).expect("play");
    play.begin_precalc(keys, min_stride, budget).expect("begin");
    while !play.precalc_step().expect("a stride cuts") {}
    play
}

#[test]
fn the_precalculated_stock_is_the_live_cut_at_every_position() {
    let world = small();
    let mut live = FieldPlay::from_seed(PlaySeed::gather(&world, false, CELLS).expect("seed")).expect("live");
    let base = PackedBase::of(&live.view().stock, None).expect("packs").0;
    let mut pre = precalculated(&world, 8, 1, PRECALC_BUDGET);
    let total = pre.total();
    assert!(total > 16 && pre.checkpoints() > 4, "a program with keyframes: {total} moves, {} keys", pre.checkpoints());
    // Every position, in an order that mixes forward and backward jumps.
    let mut ks: Vec<usize> = (0..=total).collect();
    ks.sort_by_key(|k| (k * 7919) % (total + 1));
    for k in ks {
        live.seek(k).expect("live cut");
        pre.seek(k).expect("replay");
        assert_eq!(bits(&pre.view().stock, &base), bits(&live.view().stock, &base), "move {k}: the replay IS the live cut");
    }
}

#[test]
fn a_jump_after_the_calculation_cuts_at_most_one_stride() {
    let world = small();
    let mut play = precalculated(&world, 8, 1, PRECALC_BUDGET);
    let (total, stride) = (play.total(), play.stride());
    assert!(stride < total / 4, "keyframes are dense enough to mean something: stride {stride} of {total}");
    let full = field_view_with(&world, CELLS).expect("deviate view");
    let base = PackedBase::of(&full.stock, None).expect("packs").0;
    assert_eq!(bits(&play.view().stock, &base), bits(&full.stock, &base), "the calculation ends at the Deviate picture");
    for k in [total / 5, 4 * total / 5, 1, total - 1, total / 2, 0, total] {
        let before = play.moves_cut();
        play.seek(k).expect("jump");
        let cut = play.moves_cut() - before;
        assert!(cut <= stride, "a jump to {k} cut {cut} moves, more than one stride ({stride}): no new full calculation");
    }
    assert_eq!(bits(&play.view().stock, &base), bits(&full.stock, &base), "and the end is still the end");
}

#[test]
fn the_keyframes_stay_within_the_memory_budget() {
    let world = small();
    let roomy = precalculated(&world, 64, 1, PRECALC_BUDGET);
    let one = roomy.keyframe_bytes() / roomy.checkpoints().max(1);
    assert!(one > 0, "a keyframe costs bytes");
    // A budget of about three average keyframes: the store thins.
    let budget = one * 3;
    let tight = precalculated(&world, 64, 1, budget);
    assert!(tight.keyframe_bytes() <= budget, "{} bytes held over a {budget} budget", tight.keyframe_bytes());
    assert!(tight.checkpoints() < roomy.checkpoints() && tight.stride() > roomy.stride(), "thinning doubles the stride");
    assert_eq!(tight.at(), tight.total(), "a thinned calculation still runs to the end");
    // Nothing fits: only the uncut stock stays, and replay still works.
    let none = precalculated(&world, 64, 1, 0);
    assert_eq!((none.checkpoints(), none.keyframe_bytes()), (1, 0), "a zero budget keeps only the uncut stock");
}

#[test]
fn a_stopped_calculation_leaves_exact_keyframes_and_cuts_the_rest_live() {
    let world = small();
    let mut play = FieldPlay::from_seed(PlaySeed::gather(&world, false, CELLS).expect("seed")).expect("play");
    play.begin_precalc(8, 1, PRECALC_BUDGET).expect("begin");
    assert!(!play.precalc_step().expect("one stride"), "one stride is not the program");
    let total = play.total();
    play.seek(total).expect("the rest cuts live");
    let full = field_view_with(&world, CELLS).expect("deviate view");
    let base = PackedBase::of(&full.stock, None).expect("packs").0;
    assert_eq!(bits(&play.view().stock, &base), bits(&full.stock, &base), "a partial calculation never changes the picture");
}

/// The #2192 review case: the calculation stops after it thinned once,
/// then live playback seeks over the whole program. Each keyframe the
/// live cut keeps is counted against the same budget, and the replay is
/// still the live cut.
#[test]
fn a_live_seek_after_a_cancel_keeps_the_byte_budget() {
    let world = small();
    let roomy = precalculated(&world, 64, 1, PRECALC_BUDGET);
    let budget = roomy.keyframe_bytes() / roomy.checkpoints().max(1) * 3;
    assert!(budget > 0, "a keyframe costs bytes");
    let mut play = FieldPlay::from_seed(PlaySeed::gather(&world, false, CELLS).expect("seed")).expect("play");
    play.begin_precalc(64, 1, budget).expect("begin");
    let first = play.stride();
    while play.stride() == first {
        assert!(!play.precalc_step().expect("a stride cuts"), "the budget thins before the end");
    }
    // Cancelled here: from now on the playback cuts live.
    let (total, done) = (play.total(), play.at());
    assert!(done < total, "the stop is before the end: {done} of {total}");
    let mut live = FieldPlay::from_seed(PlaySeed::gather(&world, false, CELLS).expect("seed")).expect("live");
    let base = PackedBase::of(&live.view().stock, None).expect("packs").0;
    for k in [total, 0, total / 2, total, total / 3, 1, total] {
        play.seek(k).expect("live cut past the cancel");
        live.seek(k).expect("live");
        let held = play.keyframes().held_bytes();
        assert_eq!(play.keyframe_bytes(), held, "move {k}: every keyframe the live cut kept is counted");
        assert!(held <= budget, "move {k}: {held} bytes in {} keyframes over a {budget} budget", play.checkpoints());
        assert_eq!(bits(&play.view().stock, &base), bits(&live.view().stock, &base), "move {k}: the replay IS the live cut");
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

fn frame_at(w: &PlayWorker, k: usize) -> Stock {
    w.request(k);
    let (mut seen, t0) = (w.published(), Instant::now());
    while t0.elapsed() < Duration::from_secs(120) {
        if let Some(f) = w.take() {
            let f = f.expect("the worker cuts the face program");
            if f.at == k {
                return f.view.stock.clone();
            }
        }
        seen = w.wait_published(seen, Duration::from_millis(100));
    }
    panic!("no frame at move {k}");
}

#[test]
fn the_worker_calculates_first_then_replays_and_the_post_gate_is_unchanged() {
    // A program longer than the worker's minimum stride (64 moves).
    let world = faced_doc(24.0, 0.8, Some(2.5));
    let gate_before = camcraft_app::posting::verification_gate(&world);
    let w = PlayWorker::spawn_precalc(PlaySeed::gather(&world, false, CELLS).expect("seed"), PRECALC_BUDGET).expect("worker");
    assert!(w.needs_poll(), "the UI polls while the calculation runs");
    let total = w.index().total();
    assert!(total > 128, "two strides at least: {total}");
    let Precalc::Ready { keys, bytes, stride, .. } = wait_precalc(&w) else { panic!("the calculation finishes: {:?}", w.precalc()) };
    assert!(keys > 2 && bytes <= PRECALC_BUDGET && stride < total, "keys {keys}, bytes {bytes}, stride {stride}");
    let mut live = FieldPlay::from_seed(PlaySeed::gather(&world, false, CELLS).expect("seed")).expect("live");
    let base = PackedBase::of(&live.view().stock, None).expect("packs").0;
    for k in [total / 2, total / 7, total, 1] {
        live.seek(k).expect("live");
        assert_eq!(bits(&frame_at(&w, k), &base), bits(&live.view().stock, &base), "move {k}: the worker's replay IS the live cut");
    }
    drop(w);
    assert_eq!(camcraft_app::posting::verification_gate(&world), gate_before, "playback is display only: the post gate reads the same");
}

#[test]
fn a_cancelled_calculation_is_incomplete_by_name_and_playback_goes_on() {
    let world = faced_doc(24.0, 0.8, Some(2.5));
    let w = PlayWorker::spawn_precalc(PlaySeed::gather(&world, false, CELLS).expect("seed"), PRECALC_BUDGET).expect("worker");
    w.cancel_precalc();
    let total = w.index().total();
    match wait_precalc(&w) {
        Precalc::Incomplete { done, why, .. } => {
            assert_eq!(why, "cancelled");
            assert!(done < total, "a cancel stops before the end: {done} of {total}");
        }
        // The calculation may finish before the cancel is read: that
        // is a whole store, never a partial one shown as whole.
        Precalc::Ready { .. } => {}
        p => panic!("a cancel ends the calculation: {p:?}"),
    }
    assert!(!matches!(w.precalc(), Precalc::Running { .. }));
    let full = field_view_with(&world, CELLS).expect("deviate view");
    let base = PackedBase::of(&full.stock, None).expect("packs").0;
    assert_eq!(bits(&frame_at(&w, total), &base), bits(&full.stock, &base), "past a cancel the live cut still reaches the end");
    assert_eq!(PlayWorker::spawn(PlaySeed::gather(&world, false, CELLS).expect("seed")).expect("worker").precalc(), Precalc::Off);
}

/// The worker's first frame at move `k`, packed lattice and all.
fn packed_frame_at(w: &PlayWorker, k: usize) -> PlayFrame {
    w.request(k);
    let (mut seen, t0) = (w.published(), Instant::now());
    while t0.elapsed() < Duration::from_secs(120) {
        if let Some(f) = w.take() {
            let f = f.expect("the worker cuts the face program");
            if f.at == k {
                return f;
            }
        }
        seen = w.wait_published(seen, Duration::from_millis(100));
    }
    panic!("no frame at move {k}");
}

/// Texels the eye draws as material: the composed `max(mesh, cut)` of
/// the packed lattice is below zero.
fn solid(mesh: &[f32], cut: &[f32]) -> usize {
    mesh.iter().zip(cut).filter(|(m, c)| m.max(**c) < 0.0).count()
}

/// #2396: the picture the GPU eye draws at the start of playback is the
/// UNCUT stock, also after the calculation before playback. The worker
/// packs the session's constant mesh layer once; packed after the
/// calculation, that layer was the finished stock (the eikonal pass
/// writes `max(mesh, cut)` into it, and later cut layers only lift
/// cells), so every frame showed the finished part - the stock body sat
/// inside the model and was never seen.
#[test]
fn the_picture_at_the_start_is_the_uncut_stock_after_the_calculation() {
    let world = small();
    let w = PlayWorker::spawn_precalc(PlaySeed::gather(&world, false, CELLS).expect("seed"), PRECALC_BUDGET).expect("worker");
    assert!(matches!(wait_precalc(&w), Precalc::Ready { .. }), "the calculation finishes: {:?}", w.precalc());
    let frame = packed_frame_at(&w, 0);
    let packed = frame.view.packed.as_ref().expect("the worker packs its frames for the eye");
    let CutLayer::Full(cut) = &packed.cut else { panic!("the first frame of a session carries its whole cut layer") };
    let uncut = FieldPlay::from_seed(PlaySeed::gather(&world, false, CELLS).expect("seed")).expect("play").view().stock;
    let (ub, ucut) = PackedBase::of(&uncut, None).expect("packs");
    let full = field_view_with(&world, CELLS).expect("deviate view");
    let (fb, fcut) = PackedBase::of(&full.stock, None).expect("packs");
    let (want, finished) = (solid(&ub.mesh, &ucut), solid(&fb.mesh, &fcut));
    assert!(want > finished && finished > 0, "the face removes material: uncut {want}, finished {finished} texels");
    assert_eq!(solid(&packed.base.mesh, cut), want, "move 0 draws the uncut stock, not the finished part");
    assert_eq!(
        packed.base.mesh.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        ub.mesh.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        "the session's mesh layer is the uncut stock's"
    );
}

/// What the keyframes cost as whole copies of the cut layers (the #2181
/// store): the price that #2194's shared bricks are measured against.
fn whole_copy_bytes(play: &FieldPlay) -> usize {
    let keys = play.keyframes();
    let at = (0..=play.total()).step_by(play.stride()).chain([play.total()]);
    let mut seen = std::collections::BTreeSet::new();
    at.filter_map(|k| keys.floor(k)).filter(|c| seen.insert(c.0)).map(|c| c.1.field().map_or(0, |f| f.cut_layer_bytes())).sum()
}

/// #2194: two keyframes share each cut brick that no move between them
/// changed (a visit that changes no cell copies nothing), so the store
/// holds the staged face in a quarter of what whole copies cost at most
/// - and the replay is still the live cut.
#[test]
fn the_keyframes_share_the_bricks_that_no_move_changed() {
    let world = faced_doc(24.0, 0.8, Some(2.5));
    // ~0.5 mm cells: 4 mm bricks, so a stride of moves touches a few
    // of the block's bricks and leaves the others alone.
    let fine = 24.0 * 24.0 * 20.0 * 8.0;
    let mut pre = FieldPlay::from_seed(PlaySeed::gather(&world, false, fine).expect("seed")).expect("play");
    pre.begin_precalc(32, 1, PRECALC_BUDGET).expect("begin");
    while !pre.precalc_step().expect("a stride cuts") {}
    let (held, whole) = (pre.keyframe_bytes(), whole_copy_bytes(&pre));
    println!("{} keys; {held} bytes held; whole copies {whole} bytes", pre.checkpoints());
    assert!(pre.checkpoints() > 16 && held > 0, "a store with keyframes: {} keys, {held} bytes", pre.checkpoints());
    assert_eq!(held, pre.keyframes().held_bytes(), "the running count is the audit's");
    assert!(held.saturating_mul(4) <= whole, "{held} bytes held; whole copies cost {whole}: the store falls by a factor of 4 at least");
    let mut live = FieldPlay::from_seed(PlaySeed::gather(&world, false, fine).expect("seed")).expect("live");
    let base = PackedBase::of(&live.view().stock, None).expect("packs").0;
    let total = pre.total();
    for k in [total / 2, 1, total, total / 3, 0, total - 1] {
        live.seek(k).expect("live cut");
        pre.seek(k).expect("replay");
        assert_eq!(bits(&pre.view().stock, &base), bits(&live.view().stock, &base), "move {k}: a shared brick is never written through");
    }
}

/// The reading for the PR (#2181): calculation time and keyframe memory
/// on the dense face the playback bench uses.
#[test]
#[ignore = "heavy lane (playback precalc reading): the dense 100 mm face, release ~1 min - run with --release -- --ignored --nocapture"]
fn precalc_reading_on_the_dense_face() {
    let world = faced_doc(100.0, 0.5, Some(0.5));
    let seed = PlaySeed::gather(&world, false, camcraft_app::stock_view::DEFAULT_TEXELS).expect("seed");
    let w = PlayWorker::spawn_precalc(seed, PRECALC_BUDGET).expect("worker");
    let t0 = Instant::now();
    let p = wait_precalc(&w);
    println!("moves {}; {p:?}; wall {:.2} s", w.index().total(), t0.elapsed().as_secs_f64());
    let Precalc::Ready { bytes, .. } = p else { panic!("the calculation finishes: {p:?}") };
    assert!(bytes <= PRECALC_BUDGET);
    drop(w);
    // #2194: the same store, read against whole copies of its keyframes.
    let seed = PlaySeed::gather(&world, false, camcraft_app::stock_view::DEFAULT_TEXELS).expect("seed");
    let mut play = FieldPlay::from_seed(seed).expect("play");
    play.begin_precalc(camcraft_app::stock_play::MAX_KEYFRAMES, camcraft_app::stock_play::MIN_STRIDE, PRECALC_BUDGET).expect("begin");
    while !play.precalc_step().expect("a stride cuts") {}
    let (held, whole) = (play.keyframe_bytes(), whole_copy_bytes(&play));
    println!("{} keys, stride {}; {held} bytes held; whole copies {whole} bytes; factor {:.1}", play.checkpoints(), play.stride(), whole as f64 / held.max(1) as f64);
    assert_eq!(held, bytes, "the worker's store is this store");
    assert!(held.saturating_mul(6) <= whole, "{held} bytes held; whole copies cost {whole}: the dense face falls by a factor of 6 at least");
}
