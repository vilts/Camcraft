//! #3447: setups keep the order they were created in. A setup added
//! second (`Front_A90`, datum from `Setup1`) is listed, saved, reopened
//! and posted after `Setup1`, although its name sorts first. The setup
//! twin of #3022 (operations).
//! Provenance: our own fixture over our own commands and tools; no vendor
//! material.
//!
//! Gate map (#3447): covers app::setup (setups, names, operation_order),
//! app::commands::setup (CreateSetup, DeleteSetup), app::project (setup
//! order on save and load), app::mcp (add_setup, get_job, post).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::commands::{AddTool, CreateOperation, CreateSetup, DeleteSetup, History, LoadMachine, SetStock, SetWcsOrigin};
use camcraft_app::geometry::Point3;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, SetupDef};
use camcraft_app::setup;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::World;
use serde_json::{json, Value};
use std::path::PathBuf;

const TOOL: &str = "EM-0400-2FL";

fn tmp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-3447-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// `Setup1` faces with `FaceFirst`; `Front_A90` (created second, datum
/// from `Setup1`) faces with `FaceSecond`.
fn two_setups(world: &World, h: &mut History) {
    h.execute(world, LoadMachine::builtin(world, "VMC 3-axis").expect("machine"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(world, AddTool::new(world, TOOL, camcraft_app::tools::Tool { rpm: 8000.0, ..tool }).expect("tool"));
    for (name, from, op) in [("Setup1", None, "FaceFirst"), ("Front_A90", Some("Setup1"), "FaceSecond")] {
        h.execute(world, CreateSetup::new(world, Some(name), from).expect("setup"));
        let stock = SetStock::new(world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
        h.execute(world, stock.expect("box"));
        let top = Location { pos: Point3::new(0.0, 0.0, 20.0), ..Location::default() };
        h.execute(world, SetWcsOrigin::new(world, top).expect("wcs"));
        h.execute(world, CreateOperation::new(world, op, TOOL, SrcRef::Stock, face()).expect("face"));
    }
}

fn op_names(world: &World) -> Vec<String> {
    setup::operation_order(world).into_iter().map(|(n, _)| n).collect()
}

#[test]
fn a_setup_created_second_stays_second_whatever_its_name() {
    let world = new_world();
    let mut h = History::default();
    two_setups(&world, &mut h);
    assert_eq!(setup::names(&world), ["Setup1", "Front_A90"]);
    assert_eq!(setup::first(&world).as_deref(), Some("Setup1"));
    assert_eq!(op_names(&world), ["FaceFirst", "FaceSecond"]);

    // A third setup goes last; deleting and undoing a setup puts it back
    // in its place.
    h.execute(&world, CreateSetup::new(&world, Some("A_Third"), None).expect("third"));
    assert_eq!(setup::names(&world), ["Setup1", "Front_A90", "A_Third"]);
    h.execute(&world, DeleteSetup::new(&world, "A_Third").expect("delete"));
    assert_eq!(setup::names(&world), ["Setup1", "Front_A90"]);
    h.undo(&world);
    assert_eq!(setup::names(&world), ["Setup1", "Front_A90", "A_Third"]);
}

#[test]
fn the_creation_order_survives_save_and_load_and_reaches_the_post() {
    let dir = tmp_dir("post");
    let world = new_world();
    let mut h = History::default();
    two_setups(&world, &mut h);
    let path = dir.join("order.ccam");
    camcraft_app::project::save(&world, &path).expect("save");

    let mut server = Server::new();
    let mut call = |name: &str, args: Value| -> Value {
        let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = server.handle(&msg).expect("a response");
        assert!(reply["result"]["isError"] != json!(true), "{name} refused: {reply}");
        reply["result"]["structuredContent"].clone()
    };
    call("open_project", json!({ "path": path.display().to_string() }));
    let job = call("get_job", json!({}));
    let listed: Vec<Value> = job["setups"].as_array().cloned().unwrap_or_default().iter().map(|s| s["name"].clone()).collect();
    assert_eq!(listed, [json!("Setup1"), json!("Front_A90")], "{job}");
    let added = call("add_setup", json!({ "name": "A_Third" }));
    assert_eq!(added["setups"], json!(["Setup1", "Front_A90", "A_Third"]), "{added}");
    call("undo", json!({}));

    call("generate", json!({}));
    let out = dir.join("job.nc");
    let r = call("post", json!({ "output": out.display().to_string() }));
    let text: String = r["files"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|f| std::fs::read_to_string(f.as_str().expect("a path")).expect("written"))
        .collect();
    let first = text.find("FaceFirst").expect("Setup1's operation is posted");
    let second = text.find("FaceSecond").expect("Front_A90's operation is posted");
    assert!(first < second, "Setup1 posts before Front_A90:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A v76 file has no setup `order`. It opens unchanged and keeps the name
/// order it posted with; the saved order key needs v77, so a reader that
/// does not know the key refuses the newer file.
#[test]
fn a_v76_file_keeps_name_order_and_a_newer_file_is_refused() {
    const { assert!(camcraft_app::project::PROJECT_VERSION >= 77, "setup order needs project v77") };
    let world = new_world();
    let mut h = History::default();
    two_setups(&world, &mut h);
    let saved: Value = serde_json::from_str(&camcraft_app::project::save_text(&world).expect("save")).expect("json");
    assert_eq!(saved["version"], json!(camcraft_app::project::PROJECT_VERSION), "{saved}");

    let mut v76 = saved.clone();
    v76["version"] = json!(76);
    for row in v76["entities"].as_array_mut().expect("entities") {
        if let Some(def) = row.get_mut("setup").and_then(Value::as_object_mut) {
            assert!(def.remove("order").is_some(), "a new setup states its order: {def:?}");
        }
    }
    let old = new_world();
    camcraft_app::project::open(&old, &v76.to_string()).expect("a v76 file opens");
    assert_eq!(setup::names(&old), ["Front_A90", "Setup1"], "v76 files post setups in name order");
    assert_eq!(op_names(&old), ["FaceSecond", "FaceFirst"]);
    assert!(setup::names(&old).iter().all(|n| setup::def_of(&old, n).expect("setup").order.is_none()));
    let resaved: Value = serde_json::from_str(&camcraft_app::project::save_text(&old).expect("resave")).expect("json");
    let mut expected = v76.clone();
    expected["version"] = json!(camcraft_app::project::PROJECT_VERSION);
    assert_eq!(resaved, expected, "a v76 file steps up with no other change");

    let mut newer = saved;
    let future = camcraft_app::project::NEWEST_PROJECT_VERSION + 1;
    newer["version"] = json!(future);
    match camcraft_app::project::open(&new_world(), &newer.to_string()) {
        Err(camcraft_app::project::ProjectError::UnsupportedVersion(v)) if v == future => {}
        other => panic!("a newer file must be refused, got {other:?}"),
    }
}

#[test]
fn exhausted_setup_order_refuses_creation_without_changing_the_program() {
    let world = new_world();
    let mut h = History::default();
    two_setups(&world, &mut h);
    let source = camcraft_app::model::find_by_name(&world, "Front_A90").expect("source");
    source.entity_view(&world).get::<&mut SetupDef>(|d| d.order = Some(u32::MAX - 1));
    h.execute(&world, CreateSetup::new(&world, Some("Last"), Some("Front_A90")).expect("last order fits"));
    assert_eq!(setup::def_of(&world, "Last").expect("last").order, Some(u32::MAX));
    let before = camcraft_app::project::save_text(&world).expect("snapshot");
    // A project can carry this boundary value; loading it must keep the refusal.
    let world = new_world();
    camcraft_app::project::open(&world, &before).expect("load maximum order");
    let before = camcraft_app::project::save_text(&world).expect("loaded snapshot");
    let error = match CreateSetup::new(&world, Some("A_Overflow"), Some("Last")) {
        Ok(_) => panic!("exhausted order must refuse creation"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("setup order is exhausted"), "{error}");
    assert_eq!(camcraft_app::project::save_text(&world).expect("snapshot"), before);
    assert_eq!(setup::names(&world), ["Setup1", "Front_A90", "Last"]);
    assert_eq!(op_names(&world), ["FaceFirst", "FaceSecond"]);
}
