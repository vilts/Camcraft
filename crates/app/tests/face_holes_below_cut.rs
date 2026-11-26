//! #2437: a Face on a model source whose through-holes stand wholly below
//! the cut plane. The model's silhouette shows the holes, but no model
//! material stands at the cut plane round them: the rows cut stock (or
//! air) over them, so the holes play no part - one flat pass, no lift, no
//! finding. A hole whose rim the cut plane does reach keeps the lift of
//! the default law (`face_holes`, M13.11).
//!
//! Provenance: our own fixture over `common::solid_part` (a flange: a
//! plate with two through-holes beside a boss); no vendor material
//! consulted.
//!
//! Gate map (#2437): covers app::operation::generate (Face arm),
//! app::operation::material_source::standing_above.

mod common;

use common::solid_part::{circle, plate_cell, rect, walls};
use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3};
use camcraft_app::machining2d::FaceHoles;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::{Location, Toolpath};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-1000-4FL";
/// The plate's top, where the through-holes open.
const PLATE_TOP: f64 = 10.0;
/// The boss's top: the model's top.
const BOSS_TOP: f64 = 15.0;
const CLEARANCE: f64 = 30.0;

/// A 100 x 60 x 10 plate with a 60 x 60 x 5 boss on its middle and a
/// dia 6 through-hole in each end, 5 mm off the end's edge: one closed
/// shell of three cells.
fn flange() -> Mesh {
    let mut f = Vec::new();
    walls(&rect(-50.0, -30.0, 50.0, 30.0), 0.0, PLATE_TOP, true, &mut f);
    plate_cell(&rect(-50.0, -30.0, -30.0, 30.0), Some((&circle((-42.0, -22.0), 3.0, 32), 0.0)), 0.0, PLATE_TOP, &mut f);
    plate_cell(&rect(30.0, -30.0, 50.0, 30.0), Some((&circle((42.0, 22.0), 3.0, 32), 0.0)), 0.0, PLATE_TOP, &mut f);
    let boss = rect(-30.0, -30.0, 30.0, 30.0);
    plate_cell(&boss, None, 0.0, BOSS_TOP, &mut f);
    walls(&boss, PLATE_TOP, BOSS_TOP, true, &mut f);
    Mesh::from_facets(f)
}

fn face_at(cut_z: Height) -> OpParams {
    OpParams::Face {
        holes: FaceHoles::Lift,
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(CLEARANCE),
        cut_z,
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// The flange in a fitted box stock with 2 mm over the boss.
fn job() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog entry");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("flange", flange()));
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 60.0, dz: BOSS_TOP + 2.0 }, at).expect("stock"));
    (world, h)
}

fn generate(world: &World, h: &mut History, name: &str, cut_z: Height) -> Toolpath {
    h.execute(world, CreateOperation::new(world, name, TOOL, SrcRef::Entity("flange".into()), face_at(cut_z)).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("cached path")
}

fn plunges(t: &Toolpath) -> usize {
    t.path.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count()
}

fn gouges(t: &Toolpath) -> Vec<&Warning> {
    t.warnings.iter().filter(|w| matches!(w, Warning::Gouge { .. })).collect()
}

#[test]
fn holes_below_the_cut_plane_are_not_lifted_over() {
    let (world, mut h) = job();
    let t = generate(&world, &mut h, "Top", Height::of(HeightReference::TopOfModel, 0.0));
    assert_eq!(plunges(&t), 1, "one flat pass, no re-entry after a lift: notes {:?}", t.path.notes);
    assert!(t.path.notes.iter().all(|n| !n.contains("lifted over")), "{:?}", t.path.notes);
    let feeds: Vec<_> = t.path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(!feeds.is_empty(), "the pass cuts");
    assert!(feeds.iter().all(|m| m.from.z == BOSS_TOP && m.to.z == BOSS_TOP), "every cut stands on the cut plane");
    // Each row runs the plate end to end: over both holes' columns.
    for (x, y) in [(-42.0, -22.0), (42.0, 22.0)] {
        assert!(
            feeds.iter().any(|m| (m.from.y - y).abs() <= 5.0 && m.from.y == m.to.y && m.from.x.min(m.to.x) < x - 5.0 && m.from.x.max(m.to.x) > x + 5.0),
            "a row cuts straight across the hole at ({x}, {y})"
        );
    }
}

#[test]
fn a_face_above_holes_below_the_cut_plane_has_no_gouge_finding() {
    let (world, mut h) = job();
    let t = generate(&world, &mut h, "Top", Height::of(HeightReference::TopOfModel, 0.0));
    assert!(gouges(&t).is_empty(), "nothing of the part stands above the cut plane: {:?}", t.warnings);
}

#[test]
fn a_hole_whose_rim_the_cut_plane_reaches_is_still_lifted_over() {
    // The cut plane on the plate's top: the holes' rims are faced, and
    // the default law lifts over them. The boss stands above the plane.
    let (world, mut h) = job();
    let t = generate(&world, &mut h, "Plate", Height::fixed(PLATE_TOP));
    assert!(plunges(&t) > 1, "the rows across a hole re-enter after the lift: notes {:?}", t.path.notes);
    assert!(t.path.notes.iter().any(|n| n.contains("lifted over")), "{:?}", t.path.notes);
}
