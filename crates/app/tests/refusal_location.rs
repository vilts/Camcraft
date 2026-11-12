//! #261 part 2: a refused generation names the spot it is about. The
//! generate command's error carries the world point of the hole (or
//! strip, or plunge) its refusal speaks of, beside the unchanged
//! sentence, so the UI can mark and frame it; a refusal about no spot
//! carries none.
//!
//! Provenance: our own tests over our own refusal model; no vendor
//! material consulted.
//!
//! Gate map (M4.4): covers app::commands, app::operation, motion::machining2d.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CommandError, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::drilling::Hole;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0800-4FL";

/// A 60 x 60 x 20 box with one 8 mm end mill and a Bore op over `holes`.
fn bore_world(holes: Vec<Hole>) -> World {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("box stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let params = OpParams::Bore {
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes,
    };
    history.execute(&world, CreateOperation::new(&world, "BOR", TOOL, SrcRef::Stock, params).expect("create"));
    world
}

fn hole(x: f64, y: f64, radius: f64) -> Hole {
    Hole { xy: Point2::new(x, y), radius, entry_z: 20.0, floor_z: 5.0, through: false, stepped: false }
}

fn refusal(world: &World) -> CommandError {
    match GenerateOperation::new(world, "BOR") {
        Ok(_) => panic!("the bore door was meant to refuse"),
        Err(e) => e,
    }
}

#[test]
fn a_refused_hole_names_its_mouth_in_world() {
    // An 8 mm cutter in a 7 mm hole: no rim circle to ride.
    let e = refusal(&bore_world(vec![hole(40.0, 15.0, 3.5)]));
    assert_eq!(e.location(), Some(Point3::new(40.0, 15.0, 20.0)), "the hole's mouth: {e}");
    let text = e.to_string();
    assert!(
        text.starts_with("could not generate the toolpath: BOR: invalid machining parameters: ")
            && text.contains("no circle left inside the 7 mm hole at (40, 15)"),
        "the sentence is the one it always was: {text}"
    );
}

#[test]
fn the_spot_is_the_hole_the_refusal_is_about() {
    // The first hole rides; the second is the counterbore the ride
    // declines - the spot is the second hole, not the first.
    let mut stepped = hole(45.0, 40.0, 10.0);
    stepped.stepped = true;
    let e = refusal(&bore_world(vec![hole(15.0, 15.0, 10.0), stepped]));
    assert!(e.to_string().contains("counterbore fold"), "{e}");
    assert_eq!(e.location(), Some(Point3::new(45.0, 40.0, 20.0)));
}

#[test]
fn a_refusal_about_no_spot_names_none() {
    let e = CommandError::Machining("BOR: invalid machining parameters: stepdown must be positive".into());
    assert_eq!(e.location(), None);
    // And the located twin reads exactly like the plain one.
    let at = CommandError::MachiningAt { msg: "BOR: x".into(), at: Point3::new(1.0, 2.0, 3.0) };
    assert_eq!(at.to_string(), CommandError::Machining("BOR: x".into()).to_string());
}
