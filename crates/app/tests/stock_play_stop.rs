//! #2280: the stock playback plays the program up to the first
//! operation with no toolpath, and never skips that operation to cut
//! the ones after it. The post and the Deviate verdict still refuse the
//! same document by name. Provenance: our own fixtures (a faced box)
//! over the M7.1 field engine; nothing from vendor binaries.
//!
//! Gate map (M4.4): covers app::stock_play (PlaySeed::gather, PlayIndex::stop, PlayIndex::position_at), app::stock_sim (plan_program_until_ungenerated), app::posting (program_ops_until_ungenerated), app::stock_view (display_inputs_of).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_play::{FieldPlay, PlaySeed, Progress};
use camcraft_app::stock_sim::{simulate_document, Preview, PreviewCamera, Stock, DEV_TOL};
use camcraft_app::stock_view::field_view_with;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

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
        blend_turns: false, blend_radius: None,
        picks: Default::default(),
    }
}

/// A faced box with the ops `ops` in program order: (name, cut Z,
/// generated).
fn doc(ops: &[(&str, f64, bool)]) -> World {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-HALF, -HALF, 0.0), Point3::new(HALF, HALF, 16.0))));
    for (name, cut_z, generated) in ops {
        let cmd = CreateOperation::new(&world, name, "EM-0400-2FL", SrcRef::Stock, face(*cut_z));
        history.execute(&world, cmd.expect("create face"));
        if *generated {
            history.execute(&world, GenerateOperation::new(&world, name).expect("generate face"));
        }
    }
    world
}

fn frame(s: &Stock) -> Preview {
    let cam = PreviewCamera::look_at(Point3::new(-25.0, -30.0, 45.0), Point3::new(0.0, 0.0, 10.0), Point3::new(0.0, 0.0, 1.0), 0.5)
        .expect("camera");
    s.preview(None, &cam, 24, 24, DEV_TOL).expect("frame")
}

fn same(a: &Preview, b: &Preview) -> bool {
    a.depth.iter().zip(&b.depth).all(|(x, y)| (x.is_nan() && y.is_nan()) || (x - y).abs() <= 1e-6)
}

#[test]
fn playback_plays_the_ops_before_the_first_one_without_a_toolpath() {
    let world = doc(&[("Face1", 18.0, true), ("Face2", 17.0, false)]);
    // Safety: the verdict and the post still refuse the document by name.
    let Err(err) = simulate_document(&world) else { panic!("the verdict refuses") };
    assert!(err.contains("Face2: no toolpath yet"), "{err}");
    assert!(camcraft_app::posting::program_ops(&world).is_err(), "the post refuses as before");

    let seed = PlaySeed::gather(&world, false, COARSE).expect("the playback starts");
    let index = seed.index();
    assert_eq!(index.stop(), Some("Face2"), "the playback names where it stops");
    let mut play = FieldPlay::from_seed(seed).expect("playback builds");
    assert!(play.total() > 4, "Face1's moves play: {}", play.total());
    assert_eq!(play.total(), index.total());

    // The end of playback is the stock that Face1 alone leaves.
    assert!(play.seek(usize::MAX).expect("to the end"));
    let only_face1 = doc(&[("Face1", 18.0, true)]);
    let full = field_view_with(&only_face1, COARSE).expect("Face1's stock");
    assert!(same(&frame(&play.view().stock), &frame(&full.stock)), "the playback shows Face1's removal");

    // On the timeline, an op at or past the stop holds the stock at the
    // plan's end; an op that is not in the document is still refused.
    let at_stop = Progress { op: "Face2".into(), segs_done: 3, span: 9 };
    assert_eq!(index.position_at(&at_stop, 0.5), Some((index.total(), 0.0)));
    let ghost = Progress { op: "NoSuchOp".into(), segs_done: 3, span: 9 };
    assert_eq!(index.position_at(&ghost, 0.5), None);
}

#[test]
fn playback_never_skips_an_op_without_a_toolpath_to_cut_the_next() {
    // Face1 has no toolpath; Face2 has one. Face2 must not be cut: the
    // stock it would cut is not the stock that Face1 leaves.
    let world = doc(&[("Face1", 18.0, false), ("Face2", 17.0, true)]);
    let seed = PlaySeed::gather(&world, false, COARSE).expect("the playback starts");
    let index = seed.index();
    assert_eq!(index.stop(), Some("Face1"));
    assert_eq!(index.total(), 0, "nothing plays before the stop");
    let face2 = Progress { op: "Face2".into(), segs_done: 2, span: 5 };
    assert_eq!(index.position_at(&face2, 0.5), Some((0, 0.0)), "Face2 is past the stop");
    let mut play = FieldPlay::from_seed(seed).expect("playback builds");
    let uncut = frame(&play.view().stock);
    play.seek(usize::MAX).expect("to the end");
    assert!(same(&frame(&play.view().stock), &uncut), "the stock stays uncut");
}

#[test]
fn a_whole_program_has_no_stop() {
    let world = doc(&[("Face1", 18.0, true)]);
    let index = PlaySeed::gather(&world, false, COARSE).expect("seed").index();
    assert_eq!(index.stop(), None);
}
