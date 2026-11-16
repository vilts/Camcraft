//! #93 slice 2 acceptance: a milling tool's own cutter, neck and shank
//! drawn as a spun outline lands through one undoable command
//! (`SetToolCutter`), is refused by name when it disagrees with the
//! tool's numbers, moves an operation's stamp only by a neck the stack
//! laws read (#852),
//! survives delete/undo, saves and loads (an older file loads with no
//! drawn cutter), holds a number edit that would break it, and is
//! driven the same way over MCP.
//!
//! Gate map (M4.4): covers app::commands::tool_cutter, app::tool_cutter, app::project, app::mcp::cutter_tools, app::schema.

use camcraft_app::commands::{AddTool, ChangeTool, CommandError, CreateOperation, DeleteTool, GenerateOperation, History, SetStock, SetToolCutter};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::spun::SpunPoint;
use camcraft_app::stock::StockShape;
use camcraft_app::tool_cutter;
use camcraft_app::tools::{catalog, Tool, ToolKind};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{json, Value};

const TOOL: &str = "EM-1000-4FL";

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

fn pt(z: f64, r: f64) -> SpunPoint {
    SpunPoint::line(z, r)
}

/// EM-1000-4FL (Ø10 x 22 flutes, Ø10 x 40 shank) with a Ø8 neck drawn
/// from 22 to 30 and a convex radius into the shank.
fn necked() -> Vec<SpunPoint> {
    vec![pt(0.0, 0.0), pt(0.0, 5.0), pt(22.0, 5.0), pt(22.0, 4.0), SpunPoint { z: 30.0, r: 4.0, arc: 0.0 }, pt(30.0, 5.0), pt(62.0, 5.0), pt(62.0, 0.0)]
}

fn world_with_op() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
    history.execute(&world, AddTool::new(&world, TOOL, catalog_tool(TOOL)).expect("tool"));
    let face = OpParams::Face {
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
    history.execute(&world, CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face).expect("face op"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, history)
}

fn drawn(world: &World) -> Option<usize> {
    model::find_by_name(world, TOOL).and_then(|e| tool_cutter::cutter_of(world, &e)).map(|s| s.points().len())
}

fn stamp(world: &World) -> Option<u64> {
    let op = model::operation_of(world, &model::find_by_name(world, "Face1").expect("Face1")).expect("an op");
    operation::current_stamp(world, &op.tool, &op.src, &op.params, "Face1", op.setup.as_deref()).map(|g| g.0)
}

fn refusal(r: Result<SetToolCutter, CommandError>) -> String {
    match r {
        Err(CommandError::Invalid(m)) => m,
        Err(e) => panic!("wrong refusal {e}"),
        Ok(_) => panic!("accepted an outline that should be refused"),
    }
}

#[test]
fn a_drawn_cutter_lands_undoes_and_moves_the_stamp_only_by_its_neck() {
    let (world, mut history) = world_with_op();
    let before = stamp(&world);
    assert!(before.is_some());
    history.execute(&world, SetToolCutter::new(&world, TOOL, Some(necked())).expect("drawn"));
    assert_eq!(drawn(&world), Some(8));
    // #852: the stack laws read a drawn neck, so its operations go stale.
    assert_ne!(stamp(&world), before, "a drawn neck is a moved input of the reach laws");
    let mut plain = necked();
    plain.drain(3..5);
    history.execute(&world, SetToolCutter::new(&world, TOOL, Some(plain)).expect("plain"));
    assert_eq!(drawn(&world), Some(6));
    assert_eq!(stamp(&world), before, "a drawn cutter with no neck moves no stamp");
    history.execute(&world, SetToolCutter::new(&world, TOOL, None).expect("clear"));
    assert_eq!(drawn(&world), None);
    assert!(history.undo(&world));
    assert_eq!(drawn(&world), Some(6));
    assert!(history.undo(&world));
    assert_eq!(drawn(&world), Some(8));
    assert!(history.undo(&world));
    assert_eq!(drawn(&world), None);
    assert!(history.redo(&world));
    assert_eq!(drawn(&world), Some(8));
}

#[test]
fn a_cutter_that_disagrees_with_the_numbers_is_refused_by_name() {
    let (world, _history) = world_with_op();
    let set = |pts: Vec<SpunPoint>| refusal(SetToolCutter::new(&world, TOOL, Some(pts)));
    assert!(set(Vec::new()).contains("cutter profile: the profile needs at least 3 points"));
    let mut wide = necked();
    wide[1].r = 6.0;
    wide[2].r = 6.0;
    let m = set(wide);
    assert!(m.starts_with(&format!("tool '{TOOL}': cutter profile:")) && m.contains("Ø 12.000") && m.contains("Ø 10.000"), "{m}");
    let mut long = necked();
    long[6].z = 70.0;
    long[7].z = 70.0;
    assert!(set(long).contains("flute + shank = 62.000"));
    let mut neg = necked();
    neg[3].r = -1.0;
    assert!(set(neg).contains("point 4 has a negative radius"));
    let mut nan = necked();
    nan[5].r = f64::INFINITY;
    assert!(set(nan).contains("point 6 is not a finite number"));
    let fold = vec![pt(0.0, 0.0), pt(0.0, 5.0), pt(22.0, 5.0), pt(22.0, 4.0), pt(22.0, 4.5), pt(62.0, 4.5), pt(62.0, 0.0)];
    assert!(set(fold).contains("folds back over itself at point 4"));
    let mut collar = necked();
    collar[5].r = 7.0;
    collar[6].r = 7.0;
    assert!(set(collar).contains("point 6 is wider"));
    assert!(matches!(SetToolCutter::new(&world, "nope", Some(necked())), Err(CommandError::NotFound(_))));
    // Turning tools are not spun profiles (owner, #93).
    let (lname, lathe) = catalog().into_iter().find(|(_, t)| t.kind == ToolKind::Turning).expect("an insert");
    let mut h = History::default();
    h.execute(&world, AddTool::new(&world, lname, lathe).expect("insert"));
    assert!(refusal(SetToolCutter::new(&world, lname, Some(necked()))).contains("turning tool"));
}

#[test]
fn a_number_edit_that_breaks_the_drawn_cutter_is_refused_and_delete_restores_it() {
    let (world, mut history) = world_with_op();
    history.execute(&world, SetToolCutter::new(&world, TOOL, Some(necked())).expect("drawn"));
    let fatter = Tool { diameter: 12.0, ..catalog_tool(TOOL) };
    match ChangeTool::new(&world, TOOL, fatter.clone()) {
        Err(CommandError::Invalid(m)) => assert!(m.contains("drawn cutter profile no longer fits") && m.contains("Parametric"), "{m}"),
        _ => panic!("a diameter edit under a drawn Ø10 cutter must be refused"),
    }
    // A number the outline does not state (the rpm) edits freely.
    history.execute(&world, ChangeTool::new(&world, TOOL, Tool { rpm: 5000.0, ..catalog_tool(TOOL) }).expect("rpm edit"));
    // Cleared, the same edit goes through.
    history.execute(&world, SetToolCutter::new(&world, TOOL, None).expect("clear"));
    history.execute(&world, ChangeTool::new(&world, TOOL, fatter).expect("now free"));
    history.undo(&world);
    history.undo(&world);
    assert_eq!(drawn(&world), Some(8));
    // DeleteTool snapshots the drawn cutter with the tool.
    history.execute(&world, camcraft_app::commands::DeleteOperation::new(&world, "Face1").expect("op gone"));
    history.execute(&world, DeleteTool::new(&world, TOOL).expect("delete"));
    assert_eq!(drawn(&world), None);
    assert!(history.undo(&world));
    assert_eq!(drawn(&world), Some(8));
}

#[test]
fn the_drawn_cutter_round_trips_and_older_files_load_without_one() {
    let (world, mut history) = world_with_op();
    let path = std::env::temp_dir().join(format!("camcraft_cutter_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save plain");
    let plain = std::fs::read_to_string(&path).expect("read");
    assert!(!plain.contains("\"cutter\""), "no drawn cutter writes no key: an older file's bytes");
    history.execute(&world, SetToolCutter::new(&world, TOOL, Some(necked())).expect("drawn"));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert!(first.contains("\"cutter\""));
    assert_eq!(first, second, "save-load-save is byte-identical");
    assert_eq!(drawn(&world2), Some(8));
    // An older file (no key) loads with the outline its numbers draw.
    let world3 = new_world();
    project::open(&world3, &plain).expect("older file");
    assert_eq!(drawn(&world3), None);
    // A file whose drawn cutter disagrees with its tool is refused by name.
    let mut v: Value = serde_json::from_str(&first).expect("json");
    for e in v["entities"].as_array_mut().expect("entities") {
        if let Some(t) = e.get_mut("tool").and_then(Value::as_object_mut)
            && t.contains_key("cutter")
        {
            t.insert("diameter".into(), json!(12.0));
        }
    }
    let err = project::open(&new_world(), &serde_json::to_string(&v).expect("json")).expect_err("disagreeing cutter");
    assert!(err.to_string().contains("cutter profile"), "{err}");
}

#[test]
fn the_cutter_is_drawn_and_undone_over_mcp() {
    let mut server = Server::new();
    let mut id = 0;
    let mut call = |name: &str, args: Value| {
        id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = server.handle(&msg).expect("a response");
        let r = reply["result"].clone();
        if r["isError"] == json!(true) { Err(r["content"][0]["text"].as_str().unwrap_or_default().to_owned()) } else { Ok(r["structuredContent"].clone()) }
    };
    call("add_tool", json!({ "catalog": TOOL })).expect("add tool");
    let got = call("get_tool_cutter_profile", json!({ "tool": TOOL })).expect("get");
    assert_eq!(got["drawn"], json!(false));
    assert_eq!(got["points"][0], json!([0.0, 0.0, 0.0]));
    let pts: Vec<Value> = necked().iter().map(|p| json!([p.z, p.r, p.arc])).collect();
    let set = call("set_tool_cutter_profile", json!({ "tool": TOOL, "points": pts })).expect("set");
    assert_eq!(set["drawn"], json!(true));
    assert_eq!(set["points"].as_array().map(Vec::len), Some(8));
    let err = call("set_tool_cutter_profile", json!({ "tool": TOOL, "points": [[0.0, 0.0], [0.0, 7.0], [62.0, 7.0], [62.0, 0.0]] })).expect_err("too wide");
    assert!(err.contains("cutter profile") && err.contains("Ø 14.000"), "{err}");
    assert!(call("set_tool_cutter_profile", json!({ "tool": TOOL, "points": [[0.0, "x"]] })).expect_err("junk").contains("point 1 must be"));
    call("undo", json!({})).expect("undo");
    assert_eq!(call("get_tool_cutter_profile", json!({ "tool": TOOL })).expect("get")["drawn"], json!(false));
    call("redo", json!({})).expect("redo");
    let cleared = call("set_tool_cutter_profile", json!({ "tool": TOOL, "points": null })).expect("clear");
    assert_eq!(cleared["drawn"], json!(false));
}
