//! #424 / #425 end to end: the post's verification gate and generation
//! read the whole tool stack over its true z span. A face program's
//! final retract carries the holder up into a clamp arm the flute never
//! nears (#424), and its plunge starts with the holder's top in an arm
//! the old law, pinned at the plunge's foot, never looked at (#425).
//! Both refuse the post by op, band and move; the same arm lifted clear
//! posts.
//!
//! The face path (EM-0400-2FL: flute 0..12 r2, shank 12..52 r3, holder
//! 52..87 r22): move 0 plunges at (-32, -28) from z 25 to 19.5, the
//! feeds raster y -28..28 at z 19.5 (holder 71.5..106.5), the last move
//! retracts at (-32, 28) from 19.5 to 25 (holder 71.5..112).
//!
//! Gate map (M4.4): covers app::fixture, sim::fixture, app::operation, app::posting,
//! sim::verify.
//! Provenance: our own fixtures; the law is the Prime Directive's.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock, TagFixture};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::{FixturePart, Warning};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The fixtures suite's face program, generated with nothing tagged.
fn face_world() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 };
    h.execute(&world, SetStock::new(&world, stock, Location::default()).expect("box stock"));
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog entry");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    let params = OpParams::Face {
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
    };
    h.execute(
        &world,
        CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("create face op"),
    );
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face op"));
    (world, h)
}

/// Tag a box of steel `min..max` as a fixture of the (implicit) setup.
fn plant(world: &World, h: &mut History, name: &str, min: Point3, max: Point3) {
    let e = model::spawn(world, name);
    model::set_facets(world, &e, vec![Triangle::new(min, max, p(min.x, max.y, max.z))].into());
    model::set_rev(world, &e, 1);
    h.execute(world, TagFixture::new(world, name, None).expect("tag the steel"));
}

fn hits(world: &World) -> Vec<(usize, FixturePart)> {
    let op = model::find_by_name(world, "Face1").expect("Face1");
    model::toolpath_of(world, &op)
        .expect("generated")
        .warnings
        .iter()
        .filter_map(|w| match w {
            Warning::FixtureHit { at, fixture_part } => Some((*at, *fixture_part)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_retract_carrying_the_holder_into_a_clamp_arm_blocks_the_post() {
    // An arm at z 108..120 beside the last retract's XY: above every
    // feed's holder (top 106.5), inside the retract's (top 112).
    let (world, mut h) = face_world();
    assert!(posting::verification_gate(&world).is_empty());
    plant(&world, &mut h, "Arm1", p(-40.0, 35.0, 108.0), p(-25.0, 45.0, 120.0));
    let last = model::find_by_name(&world, "Face1")
        .and_then(|op| model::toolpath_of(&world, &op))
        .map(|t| t.path.moves.len().saturating_sub(1))
        .expect("a path");
    let problems = posting::verification_gate(&world);
    assert_eq!(
        problems,
        vec![format!("Face1: holder at move {last} strikes a fixture")],
        "the retract's holder is named, nothing else is"
    );
    // Generation speaks the same verdict.
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert_eq!(hits(&world), vec![(last, FixturePart::Holder)]);
}

#[test]
fn a_plunge_starting_with_the_holder_in_a_clamp_arm_blocks_the_post() {
    // An arm at z 108..120 over the plunge at (-32, -28): the holder's
    // top is 112 at the plunge's start, 106.5 at its foot.
    let (world, mut h) = face_world();
    plant(&world, &mut h, "Arm1", p(-40.0, -45.0, 108.0), p(-25.0, -35.0, 120.0));
    // Since #690 the face opens with an approach rapid (move 0) that ends
    // at clearance over the plunge, so its holder stands in the arm too:
    // both are named, the plunge (move 1) and the approach into it.
    let mut problems = posting::verification_gate(&world);
    problems.sort();
    assert_eq!(
        problems,
        vec![
            "Face1: holder at move 0 strikes a fixture".to_string(),
            "Face1: holder at move 1 strikes a fixture".to_string(),
        ]
    );
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    let mut got = hits(&world);
    got.sort_by_key(|&(at, _)| at);
    assert_eq!(got, vec![(0, FixturePart::Holder), (1, FixturePart::Holder)]);
}

#[test]
fn the_same_arm_above_the_stack_posts() {
    // Floored at the retract's holder top (112): flush, silent - the
    // strict z rule, whole stack.
    let (world, mut h) = face_world();
    plant(&world, &mut h, "Arm1", p(-40.0, 35.0, 112.0), p(-25.0, 45.0, 120.0));
    plant(&world, &mut h, "Arm2", p(-40.0, -45.0, 112.0), p(-25.0, -35.0, 120.0));
    assert!(posting::verification_gate(&world).is_empty(), "{:?}", posting::verification_gate(&world));
}
