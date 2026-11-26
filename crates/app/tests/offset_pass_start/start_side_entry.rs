//! #3843: a new 2D Offset seeds the Helix entry. When an edit moves its
//! "Pass starts at" from Auto to Inside or Outside and keeps that seed,
//! the same undoable EditOperationParams sets Arc: a start-side pass
//! enters in the plane of its cut. Undo restores Auto and Helix in one
//! step. The MCP `edit_operation` door takes the same law.
//!
//! Gate map: covers app::commands (EditOperationParams), app::operation
//! (types::start_side_entry), app::mcp (edit_operation, undo).
//! Provenance: our own regression of the maintainer's words on #3741;
//! no vendor material consulted.

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::commands::{AddTool, ApplyCutRecipe, CreateOperation, EditOperationParams, History, SetStock};
use camcraft_app::cut_recipe::CutRecipe;
use camcraft_app::machining2d::PassStart;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{OpParams, OpType, SrcRef, seed_params};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const EM8: &str = "EM-0800-4FL";

fn world() -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == EM8).map(|(_, t)| t).expect("catalog entry");
    h.execute(&world, AddTool::new(&world, "CC", t).expect("tool"));
    world
}

fn with_start(mut p: OpParams, start: PassStart) -> OpParams {
    if let OpParams::Offset2D { pass_start, .. } = &mut p {
        *pass_start = start;
    }
    p
}

fn read(world: &World) -> (PassStart, Entry3D) {
    let e = model::find_by_name(world, "O").expect("op");
    match model::operation_of(world, &e).map(|o| o.params) {
        Some(OpParams::Offset2D { pass_start, entry, .. }) => (pass_start, entry),
        other => panic!("a 2D Offset op: {other:?}"),
    }
}

#[test]
fn a_start_side_edit_of_a_seeded_offset_takes_arc_and_undo_restores_helix() {
    for start in [PassStart::Inside, PassStart::Outside] {
        let world = world();
        let mut h = History::default();
        let seed = seed_params(OpType::Offset2D.ordinal());
        h.execute(&world, CreateOperation::new(&world, "O", "CC", SrcRef::Stock, seed.clone()).expect("create"));
        assert_eq!(read(&world), (PassStart::Auto, Entry3D::Helix), "the seed");
        h.execute(&world, EditOperationParams::new(&world, "O", with_start(seed, start)).expect("edit"));
        assert_eq!(read(&world), (start, Entry3D::Arc), "{start:?} enters in the plane of its cut");
        h.undo(&world);
        assert_eq!(read(&world), (PassStart::Auto, Entry3D::Helix), "one undo restores both");
    }
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    s.handle(&msg).expect("a request gets a response")["result"].clone()
}

#[test]
fn mcp_start_side_edit_takes_arc_and_a_later_helix_choice_stays() {
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    assert!(ok(&call(&mut s, 1, "set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }))));
    assert!(ok(&call(&mut s, 2, "add_tool", json!({ "catalog": EM8 }))));
    let add = call(&mut s, 3, "add_operation", json!({ "kind": "2D Offset", "tool": EM8, "name": "O1" }));
    assert!(ok(&add), "{add}");
    let edit = call(&mut s, 4, "edit_operation", json!({ "name": "O1", "params": { "pass_start": "outside" } }));
    assert!(ok(&edit), "{edit}");
    let text = edit.to_string();
    assert!(text.contains("\"pass_start\":\"outside\""), "{text}");
    assert!(text.contains("\"entry\":\"arc\""), "Outside takes Arc: {text}");
    // The user selects Helix again: the choice stays.
    let helix = call(&mut s, 5, "edit_operation", json!({ "name": "O1", "params": { "entry": "helix" } }));
    assert!(ok(&helix), "{helix}");
    assert!(helix.to_string().contains("\"entry\":\"helix\""), "{helix}");
}

#[test]
fn mcp_add_operation_with_start_side_takes_arc() {
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    assert!(ok(&call(&mut s, 1, "set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }))));
    assert!(ok(&call(&mut s, 2, "add_tool", json!({ "catalog": EM8 }))));
    let add = call(&mut s, 3, "add_operation", json!({ "kind": "2D Offset", "tool": EM8, "name": "O1", "params": { "pass_start": "outside" } }));
    assert!(ok(&add), "{add}");
    let job = call(&mut s, 4, "get_job", json!({}));
    assert!(ok(&job), "{job}");
    let text = job.to_string();
    assert!(text.contains("\"pass_start\":\"outside\""), "{text}");
    assert!(text.contains("\"entry\":\"arc\""), "add_operation with start side takes Arc: {text}");
}

#[test]
fn mcp_edit_operation_with_explicit_helix_and_start_side_keeps_helix() {
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    assert!(ok(&call(&mut s, 1, "set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }))));
    assert!(ok(&call(&mut s, 2, "add_tool", json!({ "catalog": EM8 }))));
    let add = call(&mut s, 3, "add_operation", json!({ "kind": "2D Offset", "tool": EM8, "name": "O1" }));
    assert!(ok(&add), "{add}");
    let edit = call(&mut s, 4, "edit_operation", json!({ "name": "O1", "params": { "pass_start": "inside", "entry": "helix" } }));
    assert!(ok(&edit), "{edit}");
    let text = edit.to_string();
    assert!(text.contains("\"pass_start\":\"inside\""), "{text}");
    assert!(text.contains("\"entry\":\"helix\""), "explicit Helix in same edit stays Helix: {text}");
}

#[test]
fn cut_recipe_keeps_explicit_helix_on_start_side_offset() {
    let world = world();
    let mut h = History::default();
    let mut params = seed_params(OpType::Offset2D.ordinal());
    if let OpParams::Offset2D { pass_start, entry, .. } = &mut params {
        *pass_start = PassStart::Outside;
        *entry = Entry3D::Helix;
    }
    h.execute(
        &world,
        CreateOperation::with_named_entry(&world, "O1", "CC", SrcRef::Stock, params.clone(), true)
            .expect("create"),
    );
    let recipe = CutRecipe::capture(&world, "O1", "OutsideHelix").expect("capture");
    assert_eq!(recipe.params, params);

    h.execute(
        &world,
        ApplyCutRecipe::new(&world, &recipe, "O2", SrcRef::Stock).expect("apply"),
    );
    let e = model::find_by_name(&world, "O2").expect("op O2");
    match model::operation_of(&world, &e).map(|o| o.params) {
        Some(OpParams::Offset2D { pass_start, entry, .. }) => {
            assert_eq!(pass_start, PassStart::Outside);
            assert_eq!(entry, Entry3D::Helix, "recipe preserves explicit Helix");
        }
        other => panic!("expected Offset2D: {other:?}"),
    }
}
