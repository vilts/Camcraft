//! #1510: a second setup whose WCS is turned onto another face. The
//! tool cuts along the machine's Z only, so a facing generated there
//! machined the part as it lies (the wrong face) and reported OK. It is
//! now refused by name at generation, the setup before it still
//! generates, and a WCS turned only about Z generates as before (its
//! post refusal is #315's, in rotated_wcs.rs).
//! Provenance: our own fixtures over the 3-axis tool-axis law.
//!
//! Gate map (#1510): covers app::setup_wcs_tilt, app::operation
//! (collect_job, selection_geometry), app::commands (GenerateOperation,
//! SetWcsOrigin, CreateSetup).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SetStock, SetWcsOrigin};
use camcraft_app::geometry::{Point3, quat_from_euler};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// Setup1 with a box stock and a faced op, then Setup2 (active) with the
/// same stock and its WCS turned by `rpy_deg`.
fn two_setups(rpy_deg: [f64; 3]) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    h.execute(&world, CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face()).expect("Face1"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("Setup1's face generates"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("Setup2 stock"));
    let [r, p, y] = rpy_deg.map(f64::to_radians);
    let loc = Location { pos: Point3::new(0.0, 0.0, 20.0), rot: quat_from_euler(r, p, y) };
    h.execute(&world, SetWcsOrigin::new(&world, loc).expect("turn Setup2's WCS"));
    h.execute(&world, CreateOperation::new(&world, "Face2", TOOL, SrcRef::Stock, face()).expect("Face2"));
    (world, h)
}

#[test]
fn a_tilted_setup_wcs_refuses_generation_by_name() {
    let (world, _h) = two_setups([90.0, 0.0, 0.0]);
    let err = GenerateOperation::new(&world, "Face2").err().expect("a tilted WCS must not generate");
    let msg = err.to_string();
    assert!(
        msg.contains("setup 'Setup2': its WCS (G54) has its Z axis tilted 90.000 deg off the machine's Z")
            && msg.contains("Part alignment"),
        "named refusal, got: {msg}"
    );
    assert!(GenerateOperation::new(&world, "Face1").is_ok(), "Setup1's square WCS still generates");
}

#[test]
fn a_wcs_turned_only_about_z_still_generates() {
    let (world, _h) = two_setups([0.0, 0.0, 30.0]);
    assert!(GenerateOperation::new(&world, "Face2").is_ok(), "a yaw-only WCS keeps the machine's Z");
}
