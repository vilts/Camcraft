//! #3362: 3D Finish "Pass starts at" (Auto / Inside / Outside), the
//! shared parameter of #3316. The generate arm hands the value to the
//! Contour and Flat passes: on a 20 x 20 x 10 mm block, Inside cuts the
//! innermost offset pass first, Outside and Auto cut the outermost pass
//! first, move for move the same. The verifier finds no blocking
//! finding on any side. The other patterns keep their moves and their
//! stamp with a saved value. The project file writes no `auto`, and a
//! file without the key reads Auto.
//!
//! Gate map: covers app::operation (generate's Finish3D arm, params
//! digest), app::project (params, params_impl), camcraft_sim::verify,
//! camcraft_motion::finishing3d (shadow, flat).
//! Provenance: our own regression of the maintainer's request; no vendor
//! material consulted.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, History, ImportModel, SetStock};
use camcraft_app::finishing3d::FinishPattern;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::PassStart;
use camcraft_app::operation::{self, Height, OpParams, OpType, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::verify::Warning;
use camcraft_app::{model, project, tools, world};
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

const TOOL: &str = "BN-0600";
const TOP: f64 = 10.0;
/// The block centre in XY.
const C: f64 = 10.0;

fn document() -> (World, History) {
    let world = world::new_world();
    let mut h = History::default();
    let part = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, TOP));
    h.execute(&world, ImportModel::from_mesh("part", part));
    let stock = StockShape::Box { dx: 28.0, dy: 28.0, dz: 12.0 };
    let loc = Location { pos: Point3::new(C, C, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, stock, loc).unwrap());
    let tool = tools::catalog().into_iter().find(|(name, _)| *name == TOOL).unwrap().1;
    h.execute(&world, AddTool::new(&world, TOOL, tool).unwrap());
    (world, h)
}

fn params(pattern: FinishPattern, start: PassStart) -> OpParams {
    let mut p = operation::seed_params(OpType::Finish3D.ordinal());
    let OpParams::Finish3D { pattern: pat, pass_start, tolerance, stepover, clearance_z, top_z, bottom_z, .. } = &mut p
    else {
        panic!("expected the 3D Finish seed");
    };
    (*pat, *pass_start, *tolerance, *stepover) = (pattern, start, 0.05, Some(3.0));
    *clearance_z = Height::fixed(18.0);
    *top_z = Height::fixed(TOP);
    *bottom_z = Height::fixed(0.0);
    p
}

fn operation_with(world: &World, h: &mut History, p: OpParams) -> Operation {
    h.execute(world, CreateOperation::new(world, "Finish", TOOL, SrcRef::Entity("part".into()), p).unwrap());
    let entity = model::find_by_name(world, "Finish").unwrap();
    model::operation_of(world, &entity).unwrap()
}

/// The half-widths (from the block centre, 0.01 mm) of the top-face
/// passes, in cut order, one entry for each change of pass.
fn pass_order(t: &Toolpath) -> Vec<i64> {
    let hw = |q: Point3| (q.x - C).abs().max((q.y - C).abs());
    let mut v: Vec<i64> = Vec::new();
    for m in t.moves.iter().filter(|m| {
        m.kind == MoveKind::Feed
            && (m.from.z - TOP).abs() < 1e-6
            && (m.to.z - TOP).abs() < 1e-6
            && hw(m.from) <= C + 1e-6
            && hw(m.to) <= C + 1e-6
    }) {
        let h = (hw(m.to) * 100.0).round() as i64;
        if v.last() != Some(&h) {
            v.push(h);
        }
    }
    v
}

#[test]
fn the_generate_arm_hands_pass_start_to_contour_and_flat() {
    let (world, mut h) = document();
    let mut op = operation_with(&world, &mut h, params(FinishPattern::Contour, PassStart::Auto));
    for pattern in [FinishPattern::Contour, FinishPattern::Flat] {
        let mut gen_with = |s: PassStart| {
            op.params = params(pattern, s);
            operation::generate(&world, &op, "Finish").unwrap_or_else(|e| panic!("{pattern:?} {s:?}: {e}"))
        };
        let (auto, aw) = gen_with(PassStart::Auto);
        let (inside, iw) = gen_with(PassStart::Inside);
        let (outside, ow) = gen_with(PassStart::Outside);
        for (what, w) in [("auto", &aw), ("inside", &iw), ("outside", &ow)] {
            let blocking: Vec<&Warning> = w.iter().filter(|w| w.blocks_post()).collect();
            assert!(blocking.is_empty(), "{pattern:?} {what}: the verifier blocks: {blocking:?}");
        }
        let (a, i) = (pass_order(&auto), pass_order(&inside));
        assert!(a.len() >= 4 && i.len() >= 4, "{pattern:?}: {a:?} {i:?}");
        assert_eq!(a[..4], [1000, 700, 400, 100], "{pattern:?}: Auto cuts outside-in: {a:?}");
        assert_eq!(i[..4], [100, 400, 700, 1000], "{pattern:?}: Inside cuts inside-out: {i:?}");
        assert_eq!(auto.moves, outside.moves, "{pattern:?}: Outside is Auto, move for move");
        assert_ne!(auto.moves, inside.moves, "{pattern:?}: Inside changes the order");
    }
}

fn stamp(world: &World, op: &Operation) -> operation::GenStamp {
    operation::current_stamp(world, TOOL, &op.src, &op.params, "Finish", op.setup.as_deref()).unwrap()
}

/// A saved Inside on a pattern without a nested ring order changes no
/// move and keeps the stamp; on Contour and Flat it moves the stamp.
#[test]
fn other_patterns_keep_their_paths_and_stamp_with_a_saved_pass_start() {
    let (world, mut h) = document();
    let mut op = operation_with(&world, &mut h, params(FinishPattern::Waterline, PassStart::Auto));
    for pattern in [FinishPattern::Waterline, FinishPattern::Parallel, FinishPattern::Project, FinishPattern::Radial] {
        op.params = params(pattern, PassStart::Auto);
        let (old_stamp, (old_path, _)) = (stamp(&world, &op), operation::generate(&world, &op, "Finish").unwrap());
        op.params = params(pattern, PassStart::Inside);
        let (new_stamp, (new_path, _)) = (stamp(&world, &op), operation::generate(&world, &op, "Finish").unwrap());
        assert_eq!(old_stamp, new_stamp, "{pattern:?}: the stamp holds");
        assert_eq!(old_path.moves, new_path.moves, "{pattern:?}: no move changes");
    }
    for pattern in [FinishPattern::Contour, FinishPattern::Flat] {
        op.params = params(pattern, PassStart::Auto);
        let auto = stamp(&world, &op);
        op.params = params(pattern, PassStart::Inside);
        assert_ne!(auto, stamp(&world, &op), "{pattern:?}: Inside moves the stamp");
    }
}

/// The project file: Auto writes no key, Inside and Outside their word,
/// a file without the key reads Auto, and each value reads back.
#[test]
fn finish3d_pass_start_rides_the_project_file() {
    let auto = params(FinishPattern::Contour, PassStart::Auto);
    let wire = project::params_to_json(&auto);
    assert_eq!(wire["finish3d"].get("pass_start"), None, "Auto stays off the wire: {wire}");
    assert_eq!(project::params_from_json(&wire).expect("an old file reads"), auto, "absent is Auto");
    for (s, word) in [(PassStart::Inside, "inside"), (PassStart::Outside, "outside")] {
        let p = params(FinishPattern::Contour, s);
        let wire = project::params_to_json(&p);
        assert_eq!(wire["finish3d"].get("pass_start").and_then(Value::as_str), Some(word), "{wire}");
        assert_eq!(project::params_from_json(&wire).expect("reads back"), p);
    }
    let mut bad = project::params_to_json(&auto);
    bad["finish3d"]["pass_start"] = json!("sideways");
    assert!(project::params_from_json(&bad).is_err(), "an unknown word is refused");
}
