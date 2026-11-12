//! M7.2 C (app half): `stock_play::FieldPlay` - the display field cut
//! move by move with the transport. The end of playback is exactly the
//! Deviate picture (`field_view_with`), a backward seek restores what a
//! forward cut to the same move showed, nothing is cut before the first
//! move completes, and the transport's script maps onto the plan by
//! name. Provenance: our own fixtures over the M7.1 field engine;
//! nothing from vendor binaries.
//!
//! Gate map (M4.4): covers app::stock_play, app::stock_play_worker, app::stock_view (display_parts, display_target), sim::sim seam.

use std::time::{Duration, Instant};

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::sim;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_play::{progress, FieldPlay, PlaySeed, Progress, MAX_CHECKPOINTS};
use camcraft_app::stock_play_worker::{PlayFrame, PlayWorker};
use camcraft_app::stock_sim::{Preview, PreviewCamera, Stock, DEV_TOL};
use camcraft_app::stock_sim::preview::pack_cut;
use camcraft_app::stock_view::{field_view_with, CutLayer};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// A 12x12 box: the smallest the Face program still makes several rows
/// of moves on, so a debug cut takes a second or two.
const HALF: f64 = 6.0;

/// ~1.5 mm cells, the `stock_view` suite's own cell law.
const COARSE: f64 = (2.0 * HALF) * (2.0 * HALF) * 20.0 / 3.6;

fn faced_doc() -> World {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    let lo = Point3::new(-HALF, -HALF, 0.0);
    let hi = Point3::new(HALF, HALF, 18.0);
    history.execute(&world, ImportModel::from_mesh("part", cube(lo, hi)));
    let params = OpParams::Face { holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(18.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    world
}

fn frame(s: &Stock) -> Preview {
    let cam = PreviewCamera::look_at(
        Point3::new(-25.0, -30.0, 45.0),
        Point3::new(0.0, 0.0, 10.0),
        Point3::new(0.0, 0.0, 1.0),
        0.5,
    )
    .expect("camera");
    s.preview(None, &cam, 24, 24, DEV_TOL).expect("frame")
}

/// Same hits, same depths (within f64 noise of the order-free max).
fn same(a: &Preview, b: &Preview) -> bool {
    a.depth.iter().zip(&b.depth).all(|(x, y)| {
        (x.is_nan() && y.is_nan()) || (x - y).abs() <= 1e-6
    })
}

#[test]
fn playback_unfolds_to_the_deviate_picture_and_seeks_back_exactly() {
    let world = faced_doc();
    let mut play = FieldPlay::with_budget(&world, false, COARSE).expect("playback builds");
    assert!(play.total() > 4, "the face program has rows to play: {}", play.total());
    assert!(!play.judged(), "unarmed playback carries no verdict");
    let uncut = frame(&play.view().stock);

    let half = play.total() / 2;
    assert!(play.seek(half).expect("forward"));
    let at_half = frame(&play.view().stock);
    assert!(!same(&uncut, &at_half), "half the program removed material the eye sees");

    assert!(play.seek(usize::MAX).expect("to the end"), "past the end clamps to the end");
    assert_eq!(play.at(), play.total());
    let full = field_view_with(&world, COARSE).expect("deviate view");
    assert!(same(&frame(&play.view().stock), &frame(&full.stock)), "the last frame IS the Deviate picture");

    assert!(play.seek(half).expect("backward"));
    assert!(same(&frame(&play.view().stock), &at_half), "a backward seek restores the forward cut");
    assert!(play.seek(0).expect("rewind"));
    assert!(same(&frame(&play.view().stock), &uncut), "move 0 is the uncut stock");
    assert!(!play.seek(0).expect("no-op"), "an unchanged seek says nothing changed");
    assert!(play.checkpoints() <= MAX_CHECKPOINTS + 1);
}

#[test]
fn the_transport_maps_onto_the_plan_by_name() {
    let world = faced_doc();
    let play = FieldPlay::with_budget(&world, false, COARSE).expect("playback builds");
    let e = model::find_by_name(&world, "Face1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("path").path;
    let tr = sim::Traverse::from_machine(&camcraft_app::posting::machine_value(&world));
    let sc = sim::script([("Face1", &tp)], &tr);

    let p0 = progress(&sc, 0.0).expect("a timeline");
    assert_eq!((p0.op.as_str(), p0.segs_done), ("Face1", 0), "nothing is cut before the first move completes");
    assert_eq!(play.index_at(&p0), Some(0));

    let end = progress(&sc, sc.total + 1.0).expect("a timeline");
    assert_eq!(play.index_at(&end), Some(play.total()), "the end cuts every move");

    let mid = progress(&sc, sc.total * 0.5).expect("a timeline");
    let k = play.index_at(&mid).expect("on the plan");
    assert!(k > 0 && k < tp.moves.len(), "mid-program is mid-plan: {k}");

    // A connector rapid ahead of the op is air, not a move.
    let lead = Progress { op: "Face1".into(), segs_done: 1, span: tp.moves.len() + 1 };
    assert_eq!(play.index_at(&lead), Some(0), "the connector cuts nothing");
    let ghost = Progress { op: "NoSuchOp".into(), segs_done: 3, span: 9 };
    assert_eq!(play.index_at(&ghost), None, "an op the plan lacks is a refusal, not a skip");
    assert!(progress(&sim::Script::default(), 1.0).is_none(), "an empty timeline has no position");
}

/// Drain the worker until a frame cut by `k` moves shows (or a minute
/// passes): every frame seen, in order.
fn frames_until(w: &PlayWorker, k: usize) -> Vec<PlayFrame> {
    let (mut seen, mut out) = (w.published(), Vec::new());
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(60) {
        if let Some(f) = w.take() {
            let f = f.expect("the worker cuts the face program");
            let done = f.at == k;
            out.push(f);
            if done {
                break;
            }
        }
        seen = w.wait_published(seen, Duration::from_millis(200));
    }
    out
}

#[test]
fn the_worker_publishes_a_model_that_follows_the_cutter() {
    let world = faced_doc();
    let w = PlayWorker::spawn(PlaySeed::gather(&world, false, COARSE).expect("seed")).expect("worker");
    assert!(w.needs_poll(), "the first picture is still pending");
    let total = w.index().total();
    assert!(total > 4, "rows to play: {total}");
    let first = frames_until(&w, 0);
    assert_eq!(first.last().map(|f| f.at), Some(0), "the uncut stock is published first");
    assert!(!w.needs_poll(), "a paused playhead at its displayed target is idle");
    let uncut = frame(&first[0].view.stock);
    // The playhead advances in steps; each published picture is cut by
    // no fewer moves than the one before, and the end is the end.
    let mut ats = vec![0usize];
    let mut last_pic = uncut;
    let mut changed = 0;
    for k in [total / 4, total / 2, 3 * total / 4, total] {
        w.request(k);
        assert!(w.needs_poll(), "a new playhead target needs frames");
        let fr = frames_until(&w, k);
        assert_eq!(fr.last().map(|f| f.at), Some(k), "the worker reaches the playhead");
        assert!(!w.needs_poll(), "the landed frame needs no further polling");
        for f in &fr {
            assert!(f.at >= *ats.last().unwrap_or(&0), "published views advance monotonically: {ats:?} then {}", f.at);
            ats.push(f.at);
        }
        let pic = frame(&fr.last().expect("a frame").view.stock);
        changed += usize::from(!same(&pic, &last_pic));
        last_pic = pic;
    }
    assert!(changed >= 2, "the model visibly updates as the cutter moves ({changed} of 4 steps)");
    let full = field_view_with(&world, COARSE).expect("deviate view");
    assert!(same(&last_pic, &frame(&full.stock)), "the worker's last frame IS the Deviate picture");
    // A backward request is followed, and the published lattice rides
    // one shared base the whole session.
    w.request(total / 2);
    let back = frames_until(&w, total / 2);
    let f = back.last().expect("a frame");
    assert_eq!(f.at, total / 2);
    let p = f.view.packed.as_ref().expect("a field view is packed on the worker");
    let p0 = first[0].view.packed.as_ref().expect("packed");
    assert!(std::sync::Arc::ptr_eq(&p.base, &p0.base), "the mesh layer is packed once per session");
    // The session's first picture sends its cut layer whole (M13.1).
    let CutLayer::Full(c) = &p0.cut else { panic!("the first frame carries the whole cut layer") };
    assert_eq!(c.len(), p0.base.mesh.len(), "the cut layer covers the base's extent");
}

/// An eye's cut layer in miniature (M13.1): a whole layer is copied, a
/// patch is applied when its chain covers the held frame, and anything
/// else repacks from the frame's own stock - `eye::rewrite_cut`'s law.
/// Every frame checks the held layer against a full repack, bit for bit.
fn apply(held: &mut Option<(u64, Vec<f32>)>, f: &PlayFrame) -> bool {
    let p = f.view.packed.as_ref().expect("a field view is packed on the worker");
    let field = f.view.stock.field().expect("a field stock");
    let patched = match (&p.cut, held.as_mut()) {
        (CutLayer::Patch { patch, .. }, Some((seq, layer))) if p.patches(*seq) => {
            patch.apply(layer).expect("the patch fits the layer");
            true
        }
        _ => false,
    };
    if !patched {
        let layer = match &p.cut {
            CutLayer::Full(c) => c.as_ref().clone(),
            CutLayer::Patch { .. } => pack_cut(field, &p.base.meta),
        };
        *held = Some((p.seq, layer));
    }
    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    let full = bits(&pack_cut(field, &p.base.meta));
    let (seq, layer) = held.as_mut().expect("a layer is held");
    *seq = p.seq;
    assert_eq!(bits(layer), full, "frame {} (move {}): the held cut layer IS a full repack", p.seq, f.at);
    patched
}

#[test]
fn patched_playback_layers_equal_a_full_repack_including_backward_seeks() {
    let world = faced_doc();
    // Half-size cells: at the coarse cell the whole lattice is a few
    // bricks, and a patch that big rightly travels as the full layer.
    let w = PlayWorker::spawn(PlaySeed::gather(&world, false, COARSE * 8.0).expect("seed")).expect("worker");
    let t = w.index().total();
    // Eye A takes every frame; eye B draws only every other one (a UI
    // that took a frame it never drew), so its chain breaks and heals.
    let (mut a, mut b, mut patches, mut n) = (None, None, 0usize, 0usize);
    for k in [0, t / 8, t / 4, 3 * t / 8, t / 2, t / 16, 5 * t / 8, 3 * t / 4, 1, 7 * t / 8, t] {
        w.request(k);
        for f in frames_until(&w, k) {
            patches += usize::from(apply(&mut a, &f));
            if n % 2 == 0 {
                apply(&mut b, &f);
            }
            n += 1;
        }
    }
    assert!(patches > 0, "forward playback sends patches ({n} frames)");
}

#[test]
fn the_dirty_ledger_follows_cuts_and_refuses_a_restore() {
    let world = faced_doc();
    let mut play = FieldPlay::from_seed(PlaySeed::gather(&world, false, COARSE).expect("seed")).expect("play");
    let total = play.total();
    assert!(play.take_dirty().is_none(), "the first take is a whole upload");
    assert_eq!(play.take_dirty().map(|d| d.len()), Some(0), "nothing cut, nothing dirty");
    play.seek(total).expect("cut to the end");
    assert!(play.take_dirty().is_some_and(|d| !d.is_empty()), "a cut dirties bricks");
    play.seek(total / 2).expect("seek back");
    assert!(play.take_dirty().is_none(), "a checkpoint restore is a whole upload");
    play.seek(total / 2).expect("stand still");
    assert_eq!(play.take_dirty().map(|d| d.len()), Some(0), "standing still dirties nothing");
}

/// M13.1 leftover: jump mode keeps no ledger (every take is a whole
/// upload), and leaving it starts the ledger fresh from a whole upload,
/// so a patch can never be relative to a cut the ledger missed.
#[test]
fn jump_mode_sends_whole_layers_and_hands_back_a_fresh_ledger() {
    let world = faced_doc();
    let mut play = FieldPlay::from_seed(PlaySeed::gather(&world, false, COARSE).expect("seed")).expect("play");
    let total = play.total();
    assert!(play.take_dirty().is_none(), "the first take is a whole upload");
    play.set_batched(true);
    assert!(play.batched());
    play.seek(total / 2).expect("cut half, batched");
    assert!(play.take_dirty().is_none(), "jump mode: the whole layer, every take");
    assert!(!play.view().stock.field().expect("field").tracks_dirty(), "the field keeps no ledger while batched");
    play.set_batched(false);
    assert!(play.view().stock.field().expect("field").tracks_dirty(), "the ledger returns with play mode");
    assert!(play.take_dirty().is_none(), "the first take after a jump is a whole upload");
    play.seek(total).expect("cut the rest");
    assert!(play.take_dirty().is_some_and(|d| !d.is_empty()), "then the ledger follows cuts again");
    // The next checkpoint boundary is where a jump's stride ends.
    play.seek(0).expect("rewind");
    let b = play.next_boundary();
    assert!(b > 0 && b <= total, "a boundary past move 0 within the program: {b}");
    play.seek(b).expect("to the boundary");
    assert!(play.next_boundary() > b || b == total, "the boundary advances");
    // The worker's jump lands on the same picture as a direct seek.
    let w = PlayWorker::spawn(PlaySeed::gather(&world, false, COARSE).expect("seed")).expect("worker");
    w.request(total);
    let fr = frames_until(&w, total);
    let last = fr.last().expect("the jump lands");
    assert_eq!(last.at, total);
    let full = field_view_with(&world, COARSE).expect("deviate view");
    assert!(same(&frame(&last.view.stock), &frame(&full.stock)), "the jump's last frame IS the Deviate picture");
}

#[test]
fn a_worker_refusal_is_said_by_name_and_drop_joins() {
    // An empty document has no program: the seed itself refuses.
    assert!(PlaySeed::gather(&new_world(), false, COARSE).is_err());
    let world = faced_doc();
    let w = PlayWorker::spawn(PlaySeed::gather(&world, false, COARSE).expect("seed")).expect("worker");
    w.request(usize::MAX); // clamps to the end
    drop(w); // stops mid-cut and joins: no thread outlives the handle
}
