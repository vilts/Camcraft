//! #1445: the reaming cycle as a drilling operation's parameter - the
//! undoable command, the generated chain and cycle record, the project
//! file, the intermediate stream, and the MCP `edit_operation` door that
//! drives the same command headlessly.
//!
//! Gate map: covers app::commands (EditOperationParams + History),
//! app::operation (drill generation), app::project (params + cached cycle
//! DTOs), app::posting (stream dump), app::mcp (edit_operation).
//!
//! Provenance: RS-274/NGC's G85 (NIST IR 6556; LinuxCNC's G-code
//! reference) for the cycle's shape - feed in, feed out, no dwell; our own
//! command and project-file laws for the rest. No vendor material.

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::cl::{CycleKind, MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::cube;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::post::cnc;
use camcraft_app::project;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOP: f64 = 20.0;
const OP: &str = "Ream1";
const TOOL: &str = "DR-0600";

fn world_with_tool() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let tool = catalog()
        .into_iter()
        .find(|(name, _)| *name == TOOL)
        .map(|(_, t)| t)
        .expect("catalog DR-0600");
    history.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let block = cube(
        camcraft_app::geometry::Point3::new(-20.0, -20.0, -10.0),
        camcraft_app::geometry::Point3::new(20.0, 20.0, TOP),
    );
    history.execute(&world, ImportModel::from_mesh("block", block));
    (world, history)
}

fn params(cycle: CycleChoice, dwell: f64) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell,
        feed: 300.0,
        plunge_feed: 90.0,
        holes: vec![Hole {
            xy: Point2::new(0.0, 0.0),
            radius: 3.0,
            entry_z: TOP,
            floor_z: TOP - 12.0,
            through: false,
            stepped: false,
        }],
        cross: vec![],
    }
}

fn cycle_of(world: &World) -> CycleChoice {
    let e = model::find_by_name(world, OP).expect("operation entity");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Drill { cycle, .. } => cycle,
        _ => panic!("a drill operation stays one"),
    }
}

fn path(world: &World) -> Toolpath {
    let e = model::find_by_name(world, OP).expect("operation entity");
    model::toolpath_of(world, &e).expect("generated toolpath").path
}

fn temp_project(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-ream-{tag}-{}.camproj", std::process::id()))
}

#[test]
fn ream_is_an_undoable_drill_parameter_that_generates_feed_out_and_round_trips() {
    let (world, mut history) = world_with_tool();
    let create = CreateOperation::new(&world, OP, TOOL, SrcRef::Entity("block".into()), params(CycleChoice::Drill, 0.0))
        .expect("create drill operation");
    history.execute(&world, create);

    // A dwell asked for beside a ream is not the cycle's word: G85 has no
    // dwell, so the record carries none and the post writes no P.
    let edit = EditOperationParams::new(&world, OP, params(CycleChoice::Ream, 1.0)).expect("edit to ream");
    history.execute(&world, edit);
    assert_eq!(cycle_of(&world), CycleChoice::Ream);
    history.execute(&world, GenerateOperation::new(&world, OP).expect("generate ream"));

    let t = path(&world);
    assert_eq!(t.cycles.len(), 1);
    let c = t.cycles[0];
    assert_eq!(c.kind, CycleKind::Ream);
    assert_eq!(c.dwell, 0.0);
    let g = &t.moves[c.first_move..];
    assert_eq!((g[0].kind, g[0].feed), (MoveKind::Plunge, Some(90.0)), "feed in");
    assert_eq!((g[1].kind, g[1].feed), (MoveKind::Feed, Some(90.0)), "feed out at the same feed");
    assert!(g[1].to.z > g[0].to.z, "the feed move is the retract");

    let records = posting::stream_records(&world).expect("dumps to records");
    let stream = cnc::serialize(&records);
    assert!(stream.contains("CYCLE REAM "), "{stream}");

    let file = temp_project("roundtrip");
    project::save(&world, &file).expect("save");
    let saved = std::fs::read_to_string(&file).expect("read saved project");
    assert!(saved.contains("\"cycle\": \"ream\""), "params spelling: {saved}");
    assert!(saved.contains("\"kind\": \"ream\""), "cached cycle spelling: {saved}");
    let loaded = new_world();
    project::load(&loaded, &file).expect("load");
    let _ = std::fs::remove_file(&file);
    assert_eq!(cycle_of(&loaded), CycleChoice::Ream);
    assert_eq!(path(&loaded).cycles[0].kind, CycleKind::Ream);

    assert!(history.undo(&world), "undo generation");
    assert!(history.undo(&world), "undo the ream edit");
    assert_eq!(cycle_of(&world), CycleChoice::Drill, "undo restores the plain drill");
    assert!(history.redo(&world), "redo the ream edit");
    assert_eq!(cycle_of(&world), CycleChoice::Ream);
}

#[test]
fn a_ream_with_a_tool_wider_than_the_hole_is_refused_by_name() {
    let (world, mut history) = world_with_tool();
    let mut p = params(CycleChoice::Ream, 0.0);
    if let OpParams::Drill { holes, .. } = &mut p {
        holes[0].radius = 2.0; // a dia. 4 hole, a dia. 6 tool
    }
    let create = CreateOperation::new(&world, OP, TOOL, SrcRef::Entity("block".into()), p).expect("create");
    history.execute(&world, create);
    let err = match GenerateOperation::new(&world, OP) {
        Err(e) => format!("{e:?}"),
        Ok(cmd) => {
            history.execute(&world, cmd);
            let e = model::find_by_name(&world, OP).expect("operation entity");
            format!("{:?}", model::operation_of(&world, &e).expect("operation"))
        }
    };
    assert!(err.contains("ream tool dia. 6 will not enter"), "{err}");
}

fn mcp_call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
    (is_err, body)
}

#[test]
fn mcp_edit_operation_sets_and_refuses_the_cycle_by_its_project_spelling() {
    let model = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/Milling-test.step")
        .to_string_lossy()
        .into_owned();
    let mut s = Server::new();
    let steps = [
        ("import_model", json!({ "path": model, "name": "part" })),
        ("add_tool", json!({ "catalog": TOOL })),
        ("add_operation", json!({ "kind": "drill", "tool": TOOL, "source": "part", "name": "OP" })),
    ];
    for (i, (name, args)) in steps.into_iter().enumerate() {
        let (err, body) = mcp_call(&mut s, i as i64 + 1, name, args);
        assert!(!err, "{name} refused: {body}");
    }
    let (err, body) = mcp_call(&mut s, 10, "edit_operation", json!({ "name": "OP", "params": { "cycle": "ream" } }));
    assert!(!err, "edit_operation refused a ream: {body}");
    assert_eq!(body["params"]["drill"]["cycle"], json!("ream"), "{body}");
    let (err, body) = mcp_call(&mut s, 11, "edit_operation", json!({ "name": "OP", "params": { "cycle": "reamer" } }));
    assert!(err, "an unknown cycle word is refused, not read as another cycle: {body}");
    let (err, body) = mcp_call(&mut s, 12, "undo", json!({}));
    assert!(!err, "undo refused: {body}");
}
