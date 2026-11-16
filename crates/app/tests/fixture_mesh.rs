//! #339 slice 1 end to end: a vise modelled as a closed mesh, its jaws
//! clamping the stock with the face pass running between them. Its one
//! world box covers the whole pass, so the box laws refused the post;
//! the mesh law clears it, while the same vise as an open sheet keeps
//! the box verdict and a jaw moved into the pass still blocks the post.
//!
//! Gate map (M4.4): covers app::fixture, sim::fixture, app::operation, app::posting, sim::verify.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock, TagFixture};
use camcraft_app::fixture;
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The face world of the fixtures suite: 60 mm box stock, a 4 mm end
/// mill, a face pass at z 19.5 under a 25 mm clearance.
fn face_world() -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Face {
                holes: Default::default(),
                tilt: None,
                blend_turns: false,
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(19.5),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        )
        .expect("face op"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, history)
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

/// The pass's XY extent (every move's endpoints).
fn path_xy(world: &flecs_ecs::prelude::World) -> (f64, f64, f64, f64) {
    let op = model::find_by_name(world, "Face1").expect("Face1");
    let path = model::toolpath_of(world, &op).expect("generated").path;
    path.moves.iter().flat_map(|m| [m.from, m.to]).fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(x0, y0, x1, y1), q| (x0.min(q.x), y0.min(q.y), x1.max(q.x), y1.max(q.y)),
    )
}

/// Jaws `gap` mm beyond the tool's edge on either side of the pass in
/// y, standing to z 21 (1.5 mm over the cut, inside the flute's window)
/// on a base under the stock: the vise's box covers the whole pass.
fn vise(world: &flecs_ecs::prelude::World, gap: f64) -> Vec<Triangle> {
    let (x0, y0, x1, y1) = path_xy(world);
    let edge = 2.0 + gap; // the 4 mm cutter's radius, plus the gap
    let (xa, xb) = (x0 - 10.0, x1 + 10.0);
    let mut t = cube(p(xa, y0 - edge - 20.0, -15.0), p(xb, y1 + edge + 20.0, -5.0));
    t.extend(cube(p(xa, y0 - edge - 20.0, -5.0), p(xb, y0 - edge, 21.0)));
    t.extend(cube(p(xa, y1 + edge, -5.0), p(xb, y1 + edge + 20.0, 21.0)));
    t
}

fn plant(world: &flecs_ecs::prelude::World, history: &mut History, name: &str, tris: Vec<Triangle>) {
    let e = model::spawn(world, name);
    model::set_facets(world, &e, tris.into());
    model::set_rev(world, &e, 1);
    history.execute(world, TagFixture::new(world, name, None).expect("tagged"));
}

fn struck(problems: &[String]) -> bool {
    problems.iter().any(|pr| pr.starts_with("Face1:") && pr.contains("strikes a fixture"))
}

#[test]
fn a_face_pass_between_closed_vise_jaws_posts() {
    let (world, mut history) = face_world();
    assert!(posting::verification_gate(&world).is_empty(), "a clean world posts");
    plant(&world, &mut history, "Vise1", vise(&world, 3.0));
    let boxes = fixture::boxes_for(&world, None);
    assert_eq!(boxes.len(), 1, "one vise, one box");
    let (x0, y0, x1, y1) = path_xy(&world);
    assert!(
        boxes[0].min.x < x0 && boxes[0].max.x > x1 && boxes[0].min.y < y0 && boxes[0].max.y > y1,
        "the vise's box covers the whole pass"
    );
    let problems = posting::verification_gate(&world);
    assert!(problems.is_empty(), "the jaws' real steel stands 3 mm off the cutter: {problems:?}");
    // Generation speaks the gate's verdict (M13.8 owed): regenerated
    // with the vise planted, the pass carries no fixture warning.
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    let op = model::find_by_name(&world, "Face1").expect("Face1");
    let warns = model::toolpath_of(&world, &op).expect("generated").warnings;
    assert!(
        !warns.iter().any(|w| matches!(w, Warning::FixtureHit { .. })),
        "generation must not convict the gap pass either: {warns:?}"
    );
}

#[test]
fn the_same_vise_as_an_open_sheet_keeps_the_box_verdict() {
    let (world, mut history) = face_world();
    let mut open = vise(&world, 3.0);
    open.truncate(open.len() - 2); // a jaw face missing: no inside to speak of
    plant(&world, &mut history, "Vise1", open);
    assert!(struck(&posting::verification_gate(&world)), "an open mesh is judged by its box");
}

#[test]
fn a_jaw_moved_into_the_pass_still_blocks_the_post() {
    let (world, mut history) = face_world();
    // -1 mm: the jaws stand 1 mm INSIDE the cutter's reach at each
    // side's outermost pass.
    plant(&world, &mut history, "Vise1", vise(&world, -1.0));
    let problems = posting::verification_gate(&world);
    assert!(struck(&problems), "the gate must name op and hit: {problems:?}");
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    let op = model::find_by_name(&world, "Face1").expect("Face1");
    let warns = model::toolpath_of(&world, &op).expect("generated").warnings;
    assert!(warns.iter().any(|w| matches!(w, Warning::FixtureHit { .. })), "and generation warns: {warns:?}");
}
