//! #339 slice 3 end to end: a 3+2 face (indexed B 10 deg - every move
//! posed) between the jaws of a vise modelled as a closed mesh. The
//! vise's one world box covers the whole pass, so the box laws refused
//! the post; the mesh law reads the leaning stack and clears it, while
//! the same vise with its jaws moved into the pass still blocks the post
//! and warns at generation.
//!
//! Gate map (M4.4): covers app::fixture, sim::fixture, app::operation, app::posting, sim::verify.

use camcraft_app::cl::is_vertical;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock, TagFixture};
use camcraft_app::fixture;
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef, Tilt};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The fixtures suite's face world - 60 mm box stock, a 4 mm end mill,
/// a face at z 19.5 - indexed B 10 deg about the stock's top centre.
fn tilted_face_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
    history.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    let tilt = Tilt { a_deg: 0.0, b_deg: 10.0, origin: [30.0, 30.0, 20.0] };
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: Some(tilt),
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let op = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, op.expect("tilted face op"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, history)
}

fn path(world: &World) -> camcraft_app::cl::Toolpath {
    let op = model::find_by_name(world, "Face1").expect("Face1");
    model::toolpath_of(world, &op).expect("generated").path
}

/// A closed, outward box: 12 facets on 8 exact corners.
fn cube(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let c = |i: usize| {
        p(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z })
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads
        .iter()
        .flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))])
        .collect()
}

/// Jaws `gap` mm beyond the cutter's edge on either side of the pass in
/// y (the lean is in x, along the jaws), standing 1.5 mm over the
/// pass's lowest tip, on a base under the stock: the vise's one box
/// covers every move of the pass.
fn vise(world: &World, gap: f64) -> Vec<Triangle> {
    let moves = path(world).moves;
    let (mut x0, mut y0, mut z0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for q in moves.iter().flat_map(|m| [m.from, m.to]) {
        (x0, y0, z0, x1, y1) = (x0.min(q.x), y0.min(q.y), z0.min(q.z), x1.max(q.x), y1.max(q.y));
    }
    let edge = 2.0 + gap; // the 4 mm cutter's radius, plus the gap
    let (xa, xb, top) = (x0 - 10.0, x1 + 10.0, z0 + 1.5);
    let mut t = cube(p(xa, y0 - edge - 20.0, -15.0), p(xb, y1 + edge + 20.0, -5.0));
    t.extend(cube(p(xa, y0 - edge - 20.0, -5.0), p(xb, y0 - edge, top)));
    t.extend(cube(p(xa, y1 + edge, -5.0), p(xb, y1 + edge + 20.0, top)));
    t
}

fn plant(world: &World, history: &mut History, tris: Vec<Triangle>) {
    let e = model::spawn(world, "Vise1");
    model::set_facets(world, &e, tris.into());
    model::set_rev(world, &e, 1);
    history.execute(world, TagFixture::new(world, "Vise1", None).expect("tagged"));
}

fn struck(problems: &[String]) -> bool {
    problems.iter().any(|pr| pr.starts_with("Face1:") && pr.contains("strikes a fixture"))
}

fn generated_hits(world: &World, history: &mut History) -> usize {
    history.execute(world, GenerateOperation::new(world, "Face1").expect("regenerate"));
    let op = model::find_by_name(world, "Face1").expect("Face1");
    let warns = model::toolpath_of(world, &op).expect("generated").warnings;
    warns.iter().filter(|w| matches!(w, Warning::FixtureHit { .. })).count()
}

#[test]
fn a_tilted_face_between_closed_vise_jaws_posts() {
    let (world, mut history) = tilted_face_world();
    let moves = path(&world).moves;
    assert!(
        !moves.is_empty() && moves.iter().all(|m| m.axis.is_some_and(|a| !is_vertical(&a))),
        "the 3+2 index poses every move"
    );
    assert!(!struck(&posting::verification_gate(&world)), "no steel, no fixture problem");
    plant(&world, &mut history, vise(&world, 3.0));
    // The box law alone convicts: the vise's one box covers the pass.
    let (turning, fixed) = fixture::workholding_for(&world, None);
    assert_eq!((turning.len(), fixed.len()), (1, 0), "one vise, turning by default, no table");
    let t = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    let wh = camcraft_app::verify::Workholding { turning: &turning, fixed: &fixed, axis: None };
    let p = path(&world);
    let boxed = camcraft_app::verify::bands_vs_workholding(&p, &t.geom(), &wh, None, 0.01);
    assert!(!boxed.is_empty(), "the world box convicts the tilted pass");
    let problems = posting::verification_gate(&world);
    assert!(!struck(&problems), "the jaws' real steel stands 3 mm off the leaning cutter: {problems:?}");
    assert_eq!(generated_hits(&world, &mut history), 0, "generation speaks the gate's verdict");
}

#[test]
fn a_jaw_moved_into_the_tilted_pass_still_blocks_the_post() {
    let (world, mut history) = tilted_face_world();
    // -1 mm: the jaws stand 1 mm inside the cutter's reach.
    plant(&world, &mut history, vise(&world, -1.0));
    let problems = posting::verification_gate(&world);
    assert!(struck(&problems), "the gate must name op and hit: {problems:?}");
    assert!(generated_hits(&world, &mut history) > 0, "and generation warns");
}
