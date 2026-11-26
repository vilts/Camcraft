//! #3360: Bore takes the shared "Pass starts at" parameter of #3316. The
//! Bore arm of generate hands it to the ring ladder: Outside cuts each
//! level's wall ring first and walks in; Auto keeps the centre-out walk.
//! The value rides the undoable EditOperationParams, the MCP
//! `edit_operation` door and the project file, and Auto is absent from
//! the file and the digest.
//!
//! Gate map: covers app::operation (generate, the Bore arm; params
//! digest), app::mcp (edit_operation, undo), app::project (params,
//! params_impl), motion::machining2d (bore, pass_start).
//! Provenance: our own tests over the shared ring law; no vendor material
//! consulted.

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, History, SetStock};
use camcraft_app::drilling::Hole;
use camcraft_app::geo2d::Point2;
use camcraft_app::machining2d::PassStart;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, project};

const EM8: &str = "EM-0800-4FL";
/// A Ø30 hole at (30, 30) with a Ø8 cutter: the wall ring rides at 11 mm.
const WALL: f64 = 11.0;
const C: (f64, f64) = (30.0, 30.0);

fn world() -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == EM8).map(|(_, t)| t).expect("catalog entry");
    h.execute(&world, AddTool::new(&world, "CC", t).expect("tool"));
    world
}

fn bore(start: PassStart) -> OpParams {
    OpParams::Bore {
        pass_start: start,
        break_through: None,
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
        holes: vec![Hole { xy: Point2::new(C.0, C.1), radius: 15.0, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false }],
        picks: Default::default(),
    }
}

fn generate(world: &World, params: OpParams) -> Toolpath {
    let op = Operation {
        tool: "CC".into(),
        src: SrcRef::Stock,
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    operation::generate(world, &op, "B").map(|(p, _)| p).expect("a legal bore")
}

/// The ring radii of the first level (Z 15), in cut order, once each.
fn first_level_radii(t: &Toolpath) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for m in t.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        if (m.to.z - 15.0).abs() > 1e-6 || (m.from.z - 15.0).abs() > 1e-6 {
            continue;
        }
        let r = ((m.to.x - C.0).hypot(m.to.y - C.1) * 100.0).round() / 100.0;
        if r > 1e-3 && !out.iter().any(|&q| (q - r).abs() < 0.02) {
            out.push(r);
        }
    }
    out
}

#[test]
fn bore_outside_cuts_the_wall_ring_first_through_generate() {
    let world = world();
    let auto = first_level_radii(&generate(&world, bore(PassStart::Auto)));
    let outside = first_level_radii(&generate(&world, bore(PassStart::Outside)));
    assert!(auto.len() >= 2, "{auto:?}");
    assert!(auto.windows(2).all(|w| w[0] < w[1]), "Auto walks centre-out: {auto:?}");
    assert!((outside[0] - WALL).abs() < 0.02, "Outside starts on the wall: {outside:?}");
    assert!(outside.windows(2).all(|w| w[0] > w[1]), "Outside walks in: {outside:?}");
    // Inside: a hole has no inner wall, so the centre-out order.
    assert_eq!(generate(&world, bore(PassStart::Inside)).moves, generate(&world, bore(PassStart::Auto)).moves);
}

#[test]
fn bore_pass_start_is_undoable_and_rides_the_project_file() {
    let world = world();
    let mut h = History::default();
    h.execute(&world, CreateOperation::new(&world, "B", "CC", SrcRef::Stock, bore(PassStart::Auto)).expect("create"));
    let e = model::find_by_name(&world, "B").expect("op");
    let read = |world: &World| match model::operation_of(world, &e).map(|o| o.params) {
        Some(OpParams::Bore { pass_start, .. }) => pass_start,
        other => panic!("a Bore op: {other:?}"),
    };
    h.execute(&world, EditOperationParams::new(&world, "B", bore(PassStart::Outside)).expect("edit"));
    assert_eq!(read(&world), PassStart::Outside);
    h.undo(&world);
    assert_eq!(read(&world), PassStart::Auto, "undo restores Auto");
    // The file: Auto stays off the wire, Outside round-trips by its key.
    assert!(!project::params_to_json(&bore(PassStart::Auto)).to_string().contains("pass_start"));
    let wire = project::params_to_json(&bore(PassStart::Outside));
    assert!(wire.to_string().contains("\"pass_start\":\"outside\""), "{wire}");
    assert_eq!(project::params_from_json(&wire).expect("reads back"), bore(PassStart::Outside));
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    s.handle(&msg).expect("a request gets a response")["result"].clone()
}

#[test]
fn mcp_sets_bore_pass_start_refuses_an_unknown_word_and_undo_restores_it() {
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    assert!(ok(&call(&mut s, 1, "set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }))));
    assert!(ok(&call(&mut s, 2, "add_tool", json!({ "catalog": EM8 }))));
    let hole = json!({ "xy": [30.0, 30.0], "radius": 15.0, "entry_z": 20.0, "floor_z": 10.0, "through": false });
    let add = call(&mut s, 3, "add_operation", json!({ "kind": "Bore", "tool": EM8, "name": "B1", "params": { "holes": [hole] } }));
    assert!(ok(&add), "{add}");
    assert!(!add.to_string().contains("pass_start"), "Auto is absent: {add}");
    let edit = call(&mut s, 4, "edit_operation", json!({ "name": "B1", "params": { "pass_start": "outside" } }));
    assert!(ok(&edit), "{edit}");
    assert!(edit.to_string().contains("\"pass_start\":\"outside\""), "{edit}");
    let bad = call(&mut s, 5, "edit_operation", json!({ "name": "B1", "params": { "pass_start": "sideways" } }));
    assert_eq!(bad["isError"], json!(true), "an unknown word is refused: {bad}");
    let auto = call(&mut s, 6, "edit_operation", json!({ "name": "B1", "params": { "pass_start": "auto" } }));
    assert!(ok(&auto), "auto is a known word: {auto}");
    assert!(!auto.to_string().contains("pass_start"), "Auto is not written: {auto}");
    assert!(ok(&call(&mut s, 7, "undo", json!({}))));
    let job = call(&mut s, 8, "get_job", json!({})).to_string();
    assert!(job.contains("\"pass_start\":\"outside\""), "undo restores Outside: {job}");
}
