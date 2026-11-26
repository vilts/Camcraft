//! #1889 through the operation layer, the project file and MCP: the
//! Deburr unreachable-edge policy. `SkipWithNote` (the default) skips an
//! edge steeper than the cone's flank and says so in a note; `Refuse`
//! refuses the operation by name. The setting round-trips in the
//! project file (absent = the default) and MCP selects the two modes.
//!
//! Provenance: our own tests of our own parameter model; the kernel law
//! is machine-tool handbook cone geometry (`finishing3d::deburr`).
//!
//! Gate map (#1889): covers app::operation (generate, the stamp),
//! app::project (params), app::mcp (edit, job).

use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::finishing3d::UnreachableEdges;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::project::{self, ProjectError};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

/// The 90-degree chamfer mill the deburr suite rides.
const CONE: &str = "CM-0600-90";

/// A 2-wide break, tips in z 6..8: on the 10 mm brick that is the top
/// rim and the top ends of the four vertical corner edges (out of reach).
fn deburr(unreachable: UnreachableEdges) -> OpParams {
    OpParams::Deburr {
        width: 2.0 * std::f64::consts::SQRT_2, // #1890: the chord of a 2.0 mm setback
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(8.0),
        bottom_z: Height::fixed(6.0),
        feed: 500.0,
        plunge_feed: 100.0,
        unreachable,
        exclude_edges: Vec::new(),
        min_edge_angle_deg: 0.0,
        exclude_holes: false,
        hole_min_diameter: 0.0,
        hole_max_diameter: 0.0,
        sizing: camcraft_app::finishing3d::DeburrBreak::default(),
        picks: Default::default(),
    }
}

/// A 40 x 20 x 10 brick that is its own stock, the cone, and a Deburr
/// "DB" of the brick with the given policy (not generated).
fn brick_world(unreachable: UnreachableEdges) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("brick", cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0))));
    h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "brick".into() }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == CONE).map(|(_, t)| t).expect("catalog cone");
    h.execute(&world, AddTool::new(&world, CONE, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "DB", CONE, SrcRef::Entity("brick".into()), deburr(unreachable)).expect("create"));
    (world, h)
}

fn status(world: &World) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, "DB").expect("op"))
}

fn policy(world: &World) -> UnreachableEdges {
    let e = model::find_by_name(world, "DB").expect("op");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Deburr { unreachable, .. } => unreachable,
        ref other => panic!("deburr params expected, got {other:?}"),
    }
}

#[test]
fn skip_with_note_generates_and_names_the_skipped_edges() {
    let (world, mut h) = brick_world(UnreachableEdges::SkipWithNote);
    h.execute(&world, GenerateOperation::new(&world, "DB").expect("the top rim is in reach"));
    assert_eq!(status(&world), OpStatus::Ok);
    let t = model::toolpath_of(&world, &model::find_by_name(&world, "DB").expect("op")).expect("cached");
    assert!(t.path.notes.iter().any(|n| n.contains("4 edges climb steeper") && n.contains("skipped")), "{:?}", t.path.notes);
}

#[test]
fn refuse_refuses_the_operation_by_name_and_leaves_no_path() {
    let (world, _h) = brick_world(UnreachableEdges::Refuse);
    let Err(CommandError::Machining(msg)) = GenerateOperation::new(&world, "DB") else {
        panic!("four corner edges are out of reach: expected a machining refusal");
    };
    assert!(msg.contains("4 edges climb steeper"), "{msg}");
    assert!(msg.contains("Unreachable edges is set to Refuse"), "{msg}");
    assert_eq!(status(&world), OpStatus::Never, "the refusal leaves no path behind");
}

#[test]
fn a_policy_edit_makes_the_generated_path_stale_and_undo_restores_it() {
    let (world, mut h) = brick_world(UnreachableEdges::SkipWithNote);
    h.execute(&world, GenerateOperation::new(&world, "DB").expect("generate"));
    assert_eq!(status(&world), OpStatus::Ok);
    h.execute(&world, EditOperationParams::new(&world, "DB", deburr(UnreachableEdges::Refuse)).expect("edit"));
    assert_eq!(policy(&world), UnreachableEdges::Refuse);
    assert_eq!(status(&world), OpStatus::Stale, "the policy is part of the stamp: a skip-policy path must not pass for a refuse-policy one");
    h.undo(&world);
    assert_eq!(policy(&world), UnreachableEdges::SkipWithNote, "undo restores the previous policy");
    assert_eq!(status(&world), OpStatus::Ok);
}

#[test]
fn a_policy_changed_outside_the_edit_command_makes_the_cached_path_stale() {
    // The policy is part of the stamp: a file whose policy word changed
    // after its path was cached must not show that path as current.
    let (world, mut h) = brick_world(UnreachableEdges::SkipWithNote);
    h.execute(&world, GenerateOperation::new(&world, "DB").expect("generate"));
    let saved = project::save_text(&world).expect("save");
    let same = new_world();
    project::open(&same, &saved).expect("reload");
    assert_eq!(status(&same), OpStatus::Ok, "the unchanged file keeps its path current");
    let flipped = saved.replace("\"unreachable\": \"skip_with_note\"", "\"unreachable\": \"refuse\"");
    assert_ne!(flipped, saved);
    let back = new_world();
    project::open(&back, &flipped).expect("reload");
    assert_eq!(policy(&back), UnreachableEdges::Refuse);
    assert_eq!(status(&back), OpStatus::Stale, "a skip-policy path must not pass for a refuse-policy operation");
}

#[test]
fn the_policy_round_trips_in_the_project_file() {
    // A Deburr file from before #1889 has no key: it reads as skip with
    // a note. The save writes the word.
    let (world, mut h) = brick_world(UnreachableEdges::SkipWithNote);
    let plain = project::save_text(&world).expect("save");
    let word = "\"unreachable\": \"skip_with_note\"";
    assert!(plain.contains("\"deburr\"") && plain.contains(word), "{plain}");
    // #1890: the break keys follow `unreachable`, so its line goes with
    // its comma and the object stays well formed.
    let old: String = plain.lines().filter(|l| !l.contains("\"unreachable\"")).collect::<Vec<_>>().join("\n");
    assert!(!old.contains("unreachable"), "{old}");
    let back = new_world();
    project::open(&back, &old).expect("a file without the key reads");
    assert_eq!(policy(&back), UnreachableEdges::SkipWithNote);
    assert_eq!(project::save_text(&back).expect("resave"), plain, "the default word round-trips byte-identically");

    h.execute(&world, EditOperationParams::new(&world, "DB", deburr(UnreachableEdges::Refuse)).expect("edit"));
    let first = project::save_text(&world).expect("save");
    assert!(first.contains("\"unreachable\": \"refuse\""), "{first}");
    let back = new_world();
    project::open(&back, &first).expect("reload");
    assert_eq!(policy(&back), UnreachableEdges::Refuse);
    assert_eq!(project::save_text(&back).expect("resave"), first, "the refuse word round-trips byte-identically");

    // A word that is not a policy is refused, not read as the default.
    let bad = first.replace("\"unreachable\": \"refuse\"", "\"unreachable\": \"ignore\"");
    assert!(matches!(project::open(&new_world(), &bad), Err(ProjectError::Corrupt(_))), "an unknown policy word is refused");
}

fn call(server: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("request answered");
    reply["result"].clone()
}

fn ok(server: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let result = call(server, id, name, args);
    assert_ne!(result["isError"], json!(true), "{name}: {result}");
    result["structuredContent"].clone()
}

#[test]
fn mcp_selects_the_two_modes_and_lists_the_field() {
    let mut s = Server::new();
    ok(&mut s, 1, "set_stock", json!({ "box": { "dx": 40.0, "dy": 20.0, "dz": 10.0 } }));
    ok(&mut s, 2, "add_tool", json!({ "catalog": CONE }));
    ok(&mut s, 3, "add_operation", json!({ "kind": "Deburr", "name": "DB", "tool": CONE, "params": { "unreachable": "refuse" } }));
    let job = ok(&mut s, 4, "get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["deburr"]["unreachable"], json!("refuse"), "{job}");

    let edited = ok(&mut s, 5, "edit_operation", json!({ "name": "DB", "params": { "unreachable": "skip_with_note" } }));
    assert_eq!(edited["params"]["deburr"]["unreachable"], json!("skip_with_note"), "{edited}");
    let edited = ok(&mut s, 6, "edit_operation", json!({ "name": "DB", "params": { "unreachable": "refuse" } }));
    assert_eq!(edited["params"]["deburr"]["unreachable"], json!("refuse"), "{edited}");

    let bad = call(&mut s, 7, "edit_operation", json!({ "name": "DB", "params": { "unreachable": "ignore" } }));
    assert_eq!(bad["isError"], json!(true), "an unknown policy word is refused: {bad}");

    let kinds = ok(&mut s, 8, "list_operation_types", json!({}));
    let text = kinds.to_string();
    let deburr = kinds["kinds"].as_array().and_then(|l| l.iter().find(|k| k["key"] == "deburr")).unwrap_or_else(|| panic!("the deburr kind is listed: {text}"));
    let field = deburr["fields"].as_array().and_then(|f| f.iter().find(|f| f["name"] == "unreachable")).unwrap_or_else(|| panic!("the field is listed: {deburr}"));
    assert_eq!(field["default"], json!("skip_with_note"));
}
