//! #2608: a step to the next operation shows the same stock as playback
//! to that position. The step goes to the start of the next operation on
//! the document's own timeline; the stock playback reads that position
//! as "all moves of the earlier operations, nothing of this one". The
//! test compares the stepped picture with playback move by move, across
//! a tool change, and with an operation that has no toolpath.
//! Provenance: our own fixtures (a faced box) over the M7.1 field engine;
//! nothing from vendor binaries.
//!
//! Gate map: covers app::stock_play (PlaySeed::gather, PlayIndex::position_at,
//! stock_play_head::progress), camcraft_sim::sim (Script::next_op_start, prev_op_start)
//! through app::setup::operation_order and app::posting::machine_value.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::sim::{self, Script};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_play::{progress, FieldPlay, PlaySeed};
use camcraft_app::stock_sim::{Preview, PreviewCamera, Stock, DEV_TOL};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, setup};

const HALF: f64 = 6.0;
/// ~1.5 mm cells, the `stock_play` suite's own cell law.
const COARSE: f64 = (2.0 * HALF) * (2.0 * HALF) * 20.0 / 3.6;

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
        blend_radius: None,
        picks: Default::default(),
    }
}

/// A faced box with the ops `ops` in program order: (name, tool, cut Z,
/// generated).
fn doc(ops: &[(&str, &str, f64, bool)]) -> World {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    for name in ["EM-0400-2FL", "EM-0600-4FL"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry");
        history.execute(&world, AddTool::new(&world, name, tool).expect("catalog tool"));
    }
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-HALF, -HALF, 0.0), Point3::new(HALF, HALF, 16.0))));
    for (name, tool, cut_z, generated) in ops {
        let cmd = CreateOperation::new(&world, name, tool, SrcRef::Stock, face(*cut_z));
        history.execute(&world, cmd.expect("create face"));
        if *generated {
            history.execute(&world, GenerateOperation::new(&world, name).expect("generate face"));
        }
    }
    world
}

/// The timeline the transport plays: the same walk as the UI's
/// `sim_script` (program order, the ops with a toolpath).
fn timeline(world: &World) -> Script {
    let tr = sim::Traverse::from_machine(&posting::machine_value(world));
    let paths: Vec<(String, camcraft_app::cl::Toolpath)> = setup::operation_order(world)
        .into_iter()
        .filter_map(|(n, e)| model::toolpath_of(world, &e).map(|tp| (n, tp.path)))
        .collect();
    sim::script(paths.iter().map(|(n, t)| (n.as_str(), t)), &tr)
}

fn moves_of(world: &World, op: &str) -> usize {
    model::find_by_name(world, op).and_then(|e| model::toolpath_of(world, &e)).map_or(0, |tp| tp.path.moves.len())
}

fn frame(s: &Stock) -> Preview {
    let cam = PreviewCamera::look_at(Point3::new(-25.0, -30.0, 45.0), Point3::new(0.0, 0.0, 10.0), Point3::new(0.0, 0.0, 1.0), 0.5)
        .expect("camera");
    s.preview(None, &cam, 24, 24, DEV_TOL).expect("frame")
}

fn same(a: &Preview, b: &Preview) -> bool {
    a.depth.iter().zip(&b.depth).all(|(x, y)| (x.is_nan() && y.is_nan()) || (x - y).abs() <= 1e-6)
}

/// The picture at playhead `t`, by the transport's own reading of it.
fn stepped(world: &World, sc: &Script, t: f64) -> (usize, f64, Preview) {
    let seed = PlaySeed::gather(world, false, COARSE).expect("the playback starts");
    let index = seed.index();
    let p = progress(sc, t).expect("a position on the timeline");
    let (k, frac) = index.position_at(&p, sc.travelled(t)).expect("the op is in the plan");
    let mut play = FieldPlay::from_seed(seed).expect("playback builds");
    play.seek_to(k, frac).expect("seek to the step");
    (k, frac, frame(&play.view().stock))
}

/// The picture of playback move by move up to move `k`.
fn played(world: &World, k: usize) -> Preview {
    let mut play = FieldPlay::from_seed(PlaySeed::gather(world, false, COARSE).expect("seed")).expect("playback builds");
    for i in 1..=k {
        play.seek(i).expect("one move on");
    }
    frame(&play.view().stock)
}

#[test]
fn next_from_inside_operation_1_shows_the_stock_after_operation_1() {
    // Face2 has another tool: the step crosses a tool change.
    let world = doc(&[("Face1", "EM-0400-2FL", 18.0, true), ("Face2", "EM-0600-4FL", 17.0, true)]);
    let sc = timeline(&world);
    let starts = sc.op_starts();
    assert_eq!(starts.len(), 2, "two operations on the timeline");
    let inside = starts[1].1 * 0.5;
    let t = sc.next_op_start(inside).expect("a next operation");
    assert_eq!(sc.at(t).op.and_then(|i| sc.ops.get(i)).map(String::as_str), Some("Face2"));

    let face1 = moves_of(&world, "Face1");
    assert!(face1 > 4, "Face1 has moves: {face1}");
    let (k, frac, pic) = stepped(&world, &sc, t);
    assert_eq!((k, frac), (face1, 0.0), "the step stands after every move of Face1, before any of Face2");
    assert!(same(&pic, &played(&world, face1)), "the stepped stock is the stock of playback to that move");

    // "Previous" from there goes back to the start, and shows the uncut stock.
    let back = sc.prev_op_start(t).expect("a previous operation");
    assert_eq!(back, 0.0, "the start of Face1 is the start of the program");
    let (k0, _, pic0) = stepped(&world, &sc, back);
    // #2901 / PR #2985: Face1 opens with its rapid seated on its own entry
    // X/Y, a segment that takes no time. At t = 0 that segment is already
    // complete, so the playback stands after it. Those moves are rapids
    // (no feed), so they cut nothing: the picture is still the uncut stock.
    let face1_op = sc.segs.first().map(|s| s.op).expect("Face1 has segments");
    let lead = sc.segs.iter().take_while(|s| s.op == face1_op && s.secs == 0.0).count();
    assert!(sc.segs[..lead].iter().all(|s| s.feed.is_none()), "the zero-time opening segments are rapids");
    assert_eq!(k0, lead, "the start stands after the zero-time opening rapid of Face1, before any cut");
    assert!(same(&pic0, &played(&world, 0)), "the start shows the uncut stock");
}

#[test]
fn an_operation_without_a_toolpath_is_no_stop_and_the_stock_holds_before_it() {
    // Face2 has no toolpath: the timeline steps from Face1 to Face3, and
    // the playback holds the stock that Face1 leaves (#2280), as it does
    // in plain playback.
    let world = doc(&[
        ("Face1", "EM-0400-2FL", 18.0, true),
        ("Face2", "EM-0600-4FL", 17.0, false),
        ("Face3", "EM-0600-4FL", 16.0, true),
    ]);
    let sc = timeline(&world);
    let names: Vec<&str> = sc.op_starts().iter().filter_map(|&(op, _)| sc.ops.get(op).map(String::as_str)).collect();
    assert_eq!(names, ["Face1", "Face3"], "Face2 is no stop");
    let t = sc.next_op_start(0.0).expect("a next operation");
    assert_eq!(sc.op_starting_at(t), Some("Face3"));
    assert_eq!(sc.next_op_start(t), None, "Face3 is the last stop");
    let face1 = moves_of(&world, "Face1");
    let (k, _, pic) = stepped(&world, &sc, t);
    assert_eq!(k, face1, "the playback holds at the plan's end");
    assert!(same(&pic, &played(&world, face1)), "the same stock as plain playback");
}
