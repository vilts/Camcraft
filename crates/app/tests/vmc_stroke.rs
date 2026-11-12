//! The built-in VMC 3-axis stroke (#457): its travel bands are read in
//! PROGRAM coordinates through the work offset (`app::machine` module
//! doc, `assets/machines/AGENTS.md`), so the file states each stroke
//! full length either way. The shipped `-500..0` Z band was a
//! machine-frame number read as a program one - every retract above
//! the work origin was "out of stroke" and no program posted. Pinned
//! here: the reported plate contour posts clean on the built-in VMC,
//! and a clearance genuinely past the stroke still refuses by name.
//!
//! Provenance: general machine-tool practice (a slide's stroke, a work
//! offset placed anywhere along it); every number is our own fixture.
//!
//! Gate map (M4.4): covers app::machine, post::machine, app::posting, app::commands.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::links::Links;
use camcraft_app::machine::{self, check_document};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

const TOOL: &str = "EM-0600-4FL";

fn contour(clearance: f64) -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(clearance),
        retract_z: Height::fixed(3.0),
        top_z: Height::fixed(1.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 1.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Linear { length: 6.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: Links::PLANAR,
    }
}

/// A 1 mm plate under a 2D Contour, the document machine the built-in
/// VMC 3-axis, G54 unplaced (the world origin, the plate's floor).
fn plate_on_the_vmc(clearance: f64) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, "VMC 3-axis").expect("built-in machine"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 1.0 }, Default::default());
    h.execute(&world, stock.expect("plate stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("plate", cube(Point3::ZERO, Point3::new(40.0, 30.0, 1.0))));
    let op = CreateOperation::new(&world, "2D Contour1", TOOL, SrcRef::Entity("plate".into()), contour(clearance));
    h.execute(&world, op.expect("create contour"));
    h.execute(&world, GenerateOperation::new(&world, "2D Contour1").expect("generate"));
    let e = model::find_by_name(&world, "2D Contour1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    assert!(cache.warnings.is_empty(), "the fixture generates silent: {:?}", cache.warnings);
    world
}

fn mounted(world: &World) -> machine::Machine {
    let (path, m) = machine::of(world);
    assert_eq!(path, posting::builtin_path("VMC 3-axis"), "the built-in is the document machine");
    m.expect("the built-in VMC is mounted")
}

#[test]
fn a_plate_contour_on_the_built_in_vmc_passes_the_stroke_and_posts() {
    let world = plate_on_the_vmc(26.0);
    let m = mounted(&world);
    let z = m.axis(2).and_then(|a| a.travel).expect("the VMC states a Z stroke");
    assert_eq!(z, (-500.0, 500.0), "the 500 mm column, full stroke each way");
    let c = check_document(&world, &m).expect("a fresh program checks");
    assert!(c.is_ok(), "rapid at Z 26, cuts at Z 0..1 - inside any VMC: {:?}", c.problems);
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "the gate is clean: {:?}", report.problems);

    let path = std::env::temp_dir().join(format!("camcraft_vmc_stroke_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    let posted = posted.unwrap_or_else(|e| panic!("the built-in VMC must post a plate contour: {e}"));
    assert!(!posted.program.is_empty());
}

#[test]
fn a_clearance_past_the_vmc_column_still_refuses_by_name() {
    // 520 mm above G54 is past the column whatever the work offset: a
    // 500 mm stroke cannot hold Z 0 and Z 520 at once.
    let world = plate_on_the_vmc(520.0);
    let m = mounted(&world);
    let c = check_document(&world, &m).expect("checks");
    assert!(
        c.problems.iter().any(|p| p.starts_with("2D Contour1:")
            && p.contains("reaches 520.000 in Z")
            && p.contains("the stroke is -500.000..500.000")),
        "{:?}",
        c.problems
    );
    assert!(!c.problems.iter().any(|p| p.contains(" in X") || p.contains(" in Y")), "{:?}", c.problems);
    let report = posting::verification_report(&world);
    assert!(report.problems.iter().any(|p| p.contains("the stroke is")), "the over-travel blocks the post: {:?}", report.problems);
    let path = std::env::temp_dir().join(format!("camcraft_vmc_overtravel_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_err(), "an over-travel never posts");
}
