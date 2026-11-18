//! #1720: a WCS axis may be stored as flipped (X+ becomes X-). Part 1
//! stores and shows the flags only: no program is posted or generated
//! with a flip set - both are refused by name until the flip is cleared,
//! and clearing it posts the same program as before. The flags are one
//! undoable command, ride the project file (a file without them opens
//! with no flip), survive delete-and-undo, and have an MCP tool.
//! Provenance: our own fixtures over our own WCS model.
//!
//! Gate map (#1720): covers app::setup_wcs_flip, app::commands::set_wcs_flip
//! (SetWcsFlip; DeleteSetup, DeleteEntity, CreateSetup undo), app::posting
//! (verification_gate), app::operation (collect_job), app::project,
//! app::mcp::wcs_flip_tools, app::mcp::job (get_job).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, DeleteEntity, DeleteSetup, GenerateOperation, History, SetStock, SetWcsFlip,
};
use camcraft_app::mcp::Server;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project, setup_wcs_flip};

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

/// A 60 x 60 x 20 box faced at Z 19.5 and generated, no WCS placed.
fn faced_block() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face()).expect("Face1"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, h)
}

fn flip(world: &World, h: &mut History, setup: Option<&str>, f: [bool; 3]) {
    h.execute(world, SetWcsFlip::for_setup(world, setup, f).expect("flip"));
}

#[test]
fn the_flip_is_one_undoable_command_and_undo_removes_a_wcs_it_placed() {
    let (world, mut h) = faced_block();
    assert!(model::find_by_name(&world, "G54").is_none(), "no WCS placed yet");
    flip(&world, &mut h, None, [true, false, true]);
    assert_eq!(setup_wcs_flip::of_setup(&world, None), [true, false, true]);
    flip(&world, &mut h, None, [false, true, false]);
    assert_eq!(setup_wcs_flip::of_setup(&world, None), [false, true, false]);
    assert!(h.undo(&world));
    assert_eq!(setup_wcs_flip::of_setup(&world, None), [true, false, true], "undo restores the flags before");
    assert!(h.undo(&world));
    assert!(model::find_by_name(&world, "G54").is_none(), "undo removes the WCS the command placed");
    assert!(h.redo(&world) && h.redo(&world));
    assert_eq!(setup_wcs_flip::of_setup(&world, None), [false, true, false], "redo lands both again");
    flip(&world, &mut h, None, [false; 3]);
    let g54 = model::find_by_name(&world, "G54").expect("G54");
    assert_eq!(setup_wcs_flip::of_entity(&world, &g54), [false; 3], "all false clears the flip");
}

#[test]
fn the_command_refuses_an_unknown_setup_by_name() {
    let (world, _h) = faced_block();
    let err = SetWcsFlip::for_setup(&world, Some("Nope"), [true, false, false]).err().expect("refused");
    assert!(err.to_string().contains("Nope"), "{err}");
}

#[test]
fn a_flipped_wcs_refuses_the_post_by_name_and_clearing_it_posts_as_before() {
    let (world, mut h) = faced_block();
    let before = posting::stream_records(&world).expect("stream");
    assert!(posting::verification_gate(&world).is_empty(), "the fixture posts");
    flip(&world, &mut h, None, [true, false, false]);
    let gate = posting::verification_gate(&world);
    assert!(
        gate.iter().any(|p| p.contains("the WCS (G54) has its X axis flipped")
            && p.contains("posting or generating with a flipped WCS is not available yet")
            && p.contains("(operations: Face1)")),
        "named refusal, got: {gate:?}"
    );
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-wcsflip-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    assert!(posting::post_setup(&world, &path).is_err(), "a flipped WCS must not post");
    assert!(!path.exists(), "a refusal leaves no file");
    flip(&world, &mut h, None, [true, true, true]);
    let gate = posting::verification_gate(&world);
    assert!(gate.iter().any(|p| p.contains("its X, Y and Z axes flipped")), "{gate:?}");
    flip(&world, &mut h, None, [false; 3]);
    assert!(posting::verification_gate(&world).is_empty(), "a cleared flip posts again");
    let after = posting::stream_records(&world).expect("stream");
    assert_eq!(format!("{before:?}"), format!("{after:?}"), "the same program as before the flip");
}

#[test]
fn a_flipped_wcs_refuses_generation_by_name() {
    let (world, mut h) = faced_block();
    flip(&world, &mut h, None, [false, false, true]);
    let err = GenerateOperation::new(&world, "Face1").err().expect("a flipped WCS must not generate");
    assert!(err.to_string().contains("the WCS (G54) has its Z axis flipped"), "named refusal, got: {err}");
    flip(&world, &mut h, None, [false; 3]);
    assert!(GenerateOperation::new(&world, "Face1").is_ok(), "a cleared flip generates");
}

#[test]
fn a_setups_flip_names_the_setup_and_leaves_the_other_setup_alone() {
    let (world, mut h) = faced_block();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    flip(&world, &mut h, Some("Setup2"), [false, true, false]);
    assert_eq!(setup_wcs_flip::of_setup(&world, Some("Setup1")), [false; 3]);
    assert!(posting::verification_gate(&world).is_empty(), "no posted op runs in Setup2");
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("Setup2 stock"));
    h.execute(&world, CreateOperation::new(&world, "Face2", TOOL, SrcRef::Stock, face()).expect("Face2"));
    let err = GenerateOperation::new(&world, "Face2").err().expect("refused");
    assert!(err.to_string().contains("setup 'Setup2': its WCS"), "{err}");
    assert!(GenerateOperation::new(&world, "Face1").is_ok(), "Setup1 still generates");
}

#[test]
fn delete_and_undo_keep_the_flip() {
    let (world, mut h) = faced_block();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    flip(&world, &mut h, Some("Setup2"), [true, false, false]);
    let wcs = camcraft_app::setup::wcs_entity_name(&world, Some("Setup2"));
    h.execute(&world, DeleteSetup::new(&world, "Setup2").expect("delete Setup2"));
    assert!(h.undo(&world));
    assert_eq!(setup_wcs_flip::of_setup(&world, Some("Setup2")), [true, false, false], "DeleteSetup undo");
    h.execute(&world, DeleteEntity::new(&world, &wcs).expect("delete the WCS"));
    assert!(model::find_by_name(&world, &wcs).is_none());
    assert!(h.undo(&world));
    assert_eq!(setup_wcs_flip::of_setup(&world, Some("Setup2")), [true, false, false], "DeleteEntity undo");
}

#[test]
fn a_new_setup_that_takes_the_datum_takes_its_flip() {
    // CreateSetup moves a never-named setup's WCS onto Setup1 (the datum
    // transfer); the flip rides along, and undo puts it back.
    let (world, mut h) = faced_block();
    flip(&world, &mut h, None, [false, true, false]);
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    assert_eq!(setup_wcs_flip::of_setup(&world, Some("Setup1")), [false, true, false], "{:?}", camcraft_app::setup::names(&world));
    assert!(h.undo(&world));
    assert_eq!(setup_wcs_flip::of_setup(&world, None), [false, true, false], "undo keeps the flip");
}

fn saved(world: &World, tag: &str) -> String {
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-wcsflip-{tag}-{}.json", std::process::id()));
    project::save(world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read back");
    let _ = std::fs::remove_file(&path);
    text
}

#[test]
fn the_project_file_keeps_the_flags_and_an_old_file_opens_with_no_flip() {
    let (world, mut h) = faced_block();
    flip(&world, &mut h, None, [true, false, true]);
    let text = saved(&world, "keep");
    let doc: Value = serde_json::from_str(&text).expect("json");
    let g54 = doc["entities"].as_array().and_then(|e| e.iter().find(|e| e["name"] == "G54")).expect("G54 row");
    assert_eq!(g54["flip"], json!({ "x": true, "z": true }), "only the flipped axes are written");
    let fresh = new_world();
    project::open(&fresh, &text).expect("opens");
    assert_eq!(setup_wcs_flip::of_setup(&fresh, None), [true, false, true], "round trip");

    let mut old = doc.clone();
    if let Some(rows) = old["entities"].as_array_mut() {
        for row in rows {
            if let Some(o) = row.as_object_mut() {
                o.remove("flip");
            }
        }
    }
    let fresh = new_world();
    project::open(&fresh, &serde_json::to_string(&old).expect("json")).expect("a file without the key opens");
    assert_eq!(setup_wcs_flip::of_setup(&fresh, None), [false; 3], "no key, no flip");

    let (plain, _h) = faced_block();
    let text = saved(&plain, "plain");
    assert!(!text.contains("\"flip\""), "no flip, no key");
}

#[test]
fn a_flip_on_a_row_that_is_not_a_wcs_is_refused_as_corrupt() {
    let (world, mut h) = faced_block();
    flip(&world, &mut h, None, [true, false, false]);
    let mut doc: Value = serde_json::from_str(&saved(&world, "corrupt")).expect("json");
    let rows = doc["entities"].as_array_mut().expect("entities");
    let other = rows.iter_mut().find(|e| e["name"] != "G54").expect("another row");
    let name = other["name"].as_str().unwrap_or_default().to_owned();
    other["flip"] = json!({ "y": true });
    let err = project::open(&new_world(), &serde_json::to_string(&doc).expect("json")).expect_err("corrupt");
    assert!(
        err.to_string().contains(&format!("'{name}' records a flipped axis but is not a WCS")),
        "named refusal, got: {err}"
    );
}

fn call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a response");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    (is_err, if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() })
}

fn job_flip(server: &mut Server, id: i64) -> Option<Value> {
    let (err, job) = call(server, id, "get_job", json!({}));
    assert!(!err, "{job}");
    job["setups"].as_array().and_then(|s| s.iter().find_map(|s| s["wcs"].get("flip").cloned()))
}

#[test]
fn the_mcp_tool_sets_the_flags_and_get_job_shows_them() {
    let mut s = Server::new();
    let init = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } });
    s.handle(&init).expect("initialize");
    assert_eq!(job_flip(&mut s, 1), Some(json!({ "x": false, "y": false, "z": false })), "no flip at first");
    let (err, body) = call(&mut s, 2, "set_wcs_flip", json!({ "y": true }));
    assert!(!err, "{body}");
    assert_eq!(body["flip"], json!({ "x": false, "y": true, "z": false }));
    assert!(body["refused"].as_str().is_some_and(|r| r.contains("Y axis flipped")), "{body}");
    let (err, body) = call(&mut s, 3, "set_wcs_flip", json!({ "x": true }));
    assert!(!err, "{body}");
    assert_eq!(body["flip"], json!({ "x": true, "y": true, "z": false }), "an axis not given keeps its flag");
    assert_eq!(job_flip(&mut s, 4), Some(json!({ "x": true, "y": true, "z": false })), "get_job shows it");
    for (id, args, want) in [
        (5, json!({}), "give at least one of"),
        (6, json!({ "x": 1 }), "must be true or false"),
        (7, json!({ "x": true, "setup": "Nope" }), "no setup named 'Nope'"),
    ] {
        let (err, body) = call(&mut s, id, "set_wcs_flip", args);
        assert!(err && body.as_str().is_some_and(|b| b.contains(want)), "{want}: {body}");
    }
    let (err, body) = call(&mut s, 8, "undo", json!({}));
    assert!(!err, "undo: {body}");
    assert_eq!(job_flip(&mut s, 9), Some(json!({ "x": false, "y": true, "z": false })), "undo reverts one step");
}
