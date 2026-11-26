//! The stock's shape on the post stream (#2769): `PARAM StockShape`,
//! `StockShapeBottom` and, for a bar, `StockShapeDiameter`, so a post can
//! write a blank definition (Fanuc Manual Guide i G1902 / G1900) that is
//! the setup's stock, or say why it cannot.
//!
//! Provenance: our own fixtures and checks.
//!
//! Gate map (M4.4): covers app::posting (stream_records: the stock-shape
//! PARAMs, posting_stock_shape).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::cnc;
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use serde_json::Value;

/// A face op on this stock, streamed.
fn stream(stock: StockShape, at: Location) -> Vec<cnc::Record> {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("catalog tool"));
    let face = OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(45.0),
        cut_z: Height::fixed(29.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0600-4FL", SrcRef::Stock, face);
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    posting::stream_records(&world).expect("a mill streams")
}

fn param(records: &[cnc::Record], n: &str) -> Option<Value> {
    records.iter().find_map(|r| match r {
        cnc::Record::Param { name, value } if name == n => Some(value.clone()),
        _ => None,
    })
}

fn at(x: f64, y: f64, z: f64, rot: [f64; 4]) -> Location {
    Location { pos: Point3::new(x, y, z), rot }
}

const ID: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

#[test]
fn a_box_stock_states_box_and_its_bottom() {
    let r = stream(StockShape::Box { dx: 100.0, dy: 60.0, dz: 30.0 }, at(0.0, 0.0, -30.0, ID));
    assert_eq!(param(&r, "StockShape"), Some(Value::from("box")));
    let bottom = param(&r, "StockShapeBottom").and_then(|v| v.as_f64()).expect("a bottom");
    assert!((bottom + 30.0).abs() < 1e-9, "the box's minimum Z: {bottom}");
    assert_eq!(param(&r, "StockShapeDiameter"), None, "a box states no diameter");
    let top = r.iter().position(|x| matches!(x, cnc::Record::Param { name, .. } if name == "StockZTop"));
    let shape = r.iter().position(|x| matches!(x, cnc::Record::Param { name, .. } if name == "StockShape"));
    assert_eq!(shape, top.map(|t| t + 1), "the shape follows the stock box");
}

#[test]
fn a_cylinder_states_its_axis_and_diameter() {
    let r = stream(StockShape::Cylinder { diameter: 80.0, height: 30.0 }, at(0.0, 0.0, 0.0, ID));
    assert_eq!(param(&r, "StockShape"), Some(Value::from("cylinder-z")));
    assert_eq!(param(&r, "StockShapeDiameter").and_then(|v| v.as_f64()), Some(80.0));

    // Turned 90 degrees about Y: the bar's axis runs along X.
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let r = stream(StockShape::Cylinder { diameter: 40.0, height: 120.0 }, at(0.0, 0.0, 20.0, [0.0, h, 0.0, h]));
    assert_eq!(param(&r, "StockShape"), Some(Value::from("cylinder-x")));
}

#[test]
fn a_stock_off_the_world_axes_states_rotated() {
    // 30 degrees about Z: the box's bounds are not the box.
    let (s, c) = (15f64.to_radians().sin(), 15f64.to_radians().cos());
    let r = stream(StockShape::Box { dx: 100.0, dy: 60.0, dz: 30.0 }, at(0.0, 0.0, 0.0, [0.0, 0.0, s, c]));
    assert_eq!(param(&r, "StockShape"), Some(Value::from("rotated")));

    // A quarter turn about Z keeps the edges on the world axes.
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let r = stream(StockShape::Box { dx: 100.0, dy: 60.0, dz: 30.0 }, at(0.0, 0.0, 0.0, [0.0, 0.0, h, h]));
    assert_eq!(param(&r, "StockShape"), Some(Value::from("box")));
}
