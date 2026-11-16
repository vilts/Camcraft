//! #1516 (app half): the move under the playhead is cut as far as the
//! tool has travelled - on a copy of the completed-move stock, so the
//! end of a move is the plain seek bit for bit, a scrub back restores
//! it, and the worker publishes the same picture. Provenance: our own
//! fixtures over the M7.1 field engine; nothing from vendor binaries.
//!
//! Gate map (M4.4): covers app::stock_play, app::stock_play_head, app::stock_play_worker.

use std::time::{Duration, Instant};

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::sim;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_play::{progress, FieldPlay, PlaySeed, Progress};
use camcraft_app::stock_play_worker::{PlayFrame, PlayWorker};
use camcraft_app::stock_sim::{Preview, PreviewCamera, Stock, DEV_TOL};
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

/// Drain the worker until a frame at fraction `frac` of move `k`
/// shows (or a minute passes): every frame seen, in order.
fn frames_until_at(w: &PlayWorker, k: usize, frac: f64) -> Vec<PlayFrame> {
    let (mut seen, mut out) = (w.published(), Vec::new());
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(60) {
        if let Some(f) = w.take() {
            let f = f.expect("the worker cuts the face program");
            let done = f.at == k && f.frac == frac;
            out.push(f);
            if done {
                break;
            }
        }
        seen = w.wait_published(seen, Duration::from_millis(200));
    }
    out
}

/// Air samples of the shown stock over the box, bit for bit.
fn air(s: &Stock) -> Vec<bool> {
    let mut out = Vec::new();
    for i in 0..40 {
        for j in 0..40 {
            for k in 0..30 {
                let p = Point3::new(i as f64 * 0.3 - HALF, j as f64 * 0.3 - HALF, k as f64 * 0.3 + 15.0);
                out.push(s.distance(p) > 0.0);
            }
        }
    }
    out
}

fn removed(s: &Stock) -> usize {
    air(s).iter().filter(|&&a| a).count()
}

/// #1516: the move under the playhead is cut as far as the tool has
/// travelled, on a copy - the end of the move is the plain seek, bit
/// for bit, and scrubbing back to its start restores the at-k picture.
#[test]
fn the_move_in_flight_cuts_as_far_as_the_tool_travelled() {
    let world = faced_doc();
    let mut play = FieldPlay::from_seed(PlaySeed::gather(&world, false, COARSE * 8.0).expect("seed")).expect("play");
    let total = play.total();
    // The first move that removes a good run of material.
    let (mut k, mut before) = (0, removed(&play.view().stock));
    let after = loop {
        assert!(k < total, "the face program removes material somewhere");
        play.seek(k + 1).expect("cut one more move");
        let n = removed(&play.view().stock);
        if n >= before + 40 {
            break n;
        }
        (k, before) = (k + 1, n);
    };
    let (at_k, at_k1) = ({ play.seek(k).expect("back"); air(&play.view().stock) }, { play.seek(k + 1).expect("on"); air(&play.view().stock) });
    play.seek(k).expect("back to the move's start");
    let _ = play.take_dirty();
    assert!(play.seek_to(k, 0.5).expect("half the move"), "a fraction changes the picture");
    assert_eq!((play.at(), play.frac()), (k, 0.5));
    let mid = removed(&play.view().stock);
    assert!(before < mid && mid < after, "half the move removes part of its material: {before} < {mid} < {after}");
    assert!(play.take_dirty().is_some_and(|d| !d.is_empty()), "the partial cut's bricks travel");
    assert!(!play.seek_to(k, 0.5).expect("stand still"), "an unchanged fraction changes nothing");
    assert!(play.seek_to(k + 1, 0.0).expect("the move ends"));
    assert_eq!(air(&play.view().stock), at_k1, "the end of the move IS the plain seek, bit for bit");
    assert!(play.seek_to(k, 0.0).expect("scrub back"));
    assert_eq!(air(&play.view().stock), at_k, "scrubbing back restores the move's start");
    let _ = play.take_dirty(); // the restore's whole upload
    play.seek_to(k, 0.5).expect("half again");
    assert!(play.take_dirty().is_some_and(|d| !d.is_empty()), "a partial picture taken");
    assert!(play.seek_to(k, 0.0).expect("to the start"));
    assert!(play.take_dirty().is_some_and(|d| !d.is_empty()), "dropping a partial re-sends its bricks");
    assert_eq!(air(&play.view().stock), at_k, "a dropped partial leaves the completed-move stock");
    play.seek_to(total, 0.7).expect("the end");
    assert_eq!(play.frac(), 0.0, "nothing is in flight at the end");
    play.seek_to(k, f64::NAN).expect("a NaN fraction");
    assert_eq!(air(&play.view().stock), at_k, "a NaN fraction cuts nothing");
}

#[test]
fn the_playhead_fraction_applies_only_to_a_move_in_flight() {
    let world = faced_doc();
    let play = FieldPlay::with_budget(&world, false, COARSE).expect("playback builds");
    let (ix, len) = (play.index(), play.total());
    let at = |segs_done, span, f| ix.position_at(&Progress { op: "Face1".into(), segs_done, span }, f);
    assert_eq!(at(0, len + 1, 0.6), Some((0, 0.0)), "the connector rapid cuts nothing");
    assert_eq!(at(1, len + 1, 0.6), Some((0, 0.6)), "behind the connector the first move is in flight");
    assert_eq!(at(2, len, 0.6), Some((2, 0.6)));
    assert_eq!(at(len, len, 0.6), Some((len, 0.0)), "a finished op has nothing in flight");
    assert_eq!(at(2, len, f64::NAN), Some((2, 0.0)), "a NaN fraction is no travel");
    assert_eq!(at(2, len, 9.0), Some((2, 1.0)), "past the end clamps to the whole move");
    let ghost = Progress { op: "NoSuchOp".into(), segs_done: 1, span: 3 };
    assert_eq!(ix.position_at(&ghost, 0.5), None, "an op the plan lacks is a refusal");
    // The live timeline: mid-segment the fraction is the drawn tool's.
    let e = model::find_by_name(&world, "Face1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("path").path;
    let tr = sim::Traverse::from_machine(&camcraft_app::posting::machine_value(&world));
    let sc = sim::script([("Face1", &tp)], &tr);
    let t = sc.total * 0.5;
    let f = sc.travelled(t);
    assert!((0.0..=1.0).contains(&f), "a fraction: {f}");
    let p = progress(&sc, t).expect("a timeline");
    assert_eq!(ix.position_at(&p, f).map(|q| q.0), ix.index_at(&p), "the fraction never moves the move index");
}

#[test]
fn the_worker_publishes_the_move_in_flight() {
    let world = faced_doc();
    let w = PlayWorker::spawn(PlaySeed::gather(&world, false, COARSE).expect("seed")).expect("worker");
    let total = w.index().total();
    let k = total / 2;
    w.request_at(k, 0.5);
    let fr = frames_until_at(&w, k, 0.5);
    let f = fr.last().expect("a frame");
    assert_eq!((f.at, f.frac), (k, 0.5), "the worker shows the move in flight");
    assert!(!w.needs_poll(), "the landed partial frame needs no further polling");
    let mut play = FieldPlay::from_seed(PlaySeed::gather(&world, false, COARSE).expect("seed")).expect("play");
    play.seek_to(k, 0.5).expect("direct");
    assert!(same(&frame(&f.view.stock), &frame(&play.view().stock)), "the worker's picture IS the direct partial seek");
    w.request_at(k + 1, 0.0);
    let end = frames_until_at(&w, k + 1, 0.0);
    play.seek(k + 1).expect("direct end");
    assert!(same(&frame(&end.last().expect("a frame").view.stock), &frame(&play.view().stock)), "then the move completes");
}
