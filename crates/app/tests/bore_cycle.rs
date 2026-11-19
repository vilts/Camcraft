//! #1445 slice 2: the boring cycle (G86) as a drilling operation's
//! parameter - the undoable command with its dwell, the dwell's door at
//! command creation, and the MCP `edit_operation` door that drives the
//! same command headlessly.
//!
//! Gate map: covers app::commands (CreateOperation, EditOperationParams,
//! History), app::operation (OpParams::dwell_refusal, drill generation),
//! app::mcp (edit_operation).
//!
//! Provenance: RS-274/NGC's G86 (NIST IR 6556; LinuxCNC's G-code
//! reference) for the cycle's shape - feed in, spindle stop, rapid out;
//! our own command laws for the rest. No vendor material.

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::cl::{CycleKind, MoveKind};
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::cube;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef, MAX_DWELL_S};
use camcraft_app::posting;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOP: f64 = 20.0;
const OP: &str = "Bore1";
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
        break_through: None,
        break_through_from: Default::default(),
    }
}

fn dwell_of(world: &World) -> (CycleChoice, f64) {
    let e = model::find_by_name(world, OP).expect("operation entity");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Drill { cycle, dwell, .. } => (cycle, dwell),
        _ => panic!("a drill operation stays one"),
    }
}

#[test]
fn refused_expanded_bores_write_no_nc_file() {
    for stem in ["heidenhain", "tinyg", "carvera"] {
        let script = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../assets/posts/{stem}.cps")),
        )
        .expect("shipped post");
        for dwell in [0.0, 0.5] {
            let (world, mut history) = world_with_tool();
            history.execute(
                &world,
                CreateOperation::new(&world, OP, TOOL, SrcRef::Entity("block".into()), params(CycleChoice::Bore, dwell))
                    .expect("create bore"),
            );
            history.execute(&world, GenerateOperation::new(&world, OP).expect("generate bore"));
            let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock").as_nanos();
            let path = std::env::temp_dir().join(format!("camcraft-bore-{stem}-{}-{nonce}.nc", std::process::id()));
            assert!(!path.exists(), "fresh output path");
            let err = posting::post_script(&world, &path, &script).expect_err("unsafe expanded bore must not post");
            assert!(
                err.contains("bore (G86) requires a spindle stop before rapid retract") && err.contains("refused, not posted"),
                "{stem}, dwell {dwell}: named safety refusal: {err}"
            );
            assert!(!path.exists(), "{stem}, dwell {dwell}: refusal writes no cutting output");
        }
    }
}

#[test]
fn a_bore_with_a_dwell_is_undoable_and_generates_feed_in_rapid_out() {
    let (world, mut history) = world_with_tool();
    let create = CreateOperation::new(&world, OP, TOOL, SrcRef::Entity("block".into()), params(CycleChoice::Drill, 0.0))
        .expect("create drill operation");
    history.execute(&world, create);
    let edit = EditOperationParams::new(&world, OP, params(CycleChoice::Bore, 0.5)).expect("edit to bore");
    history.execute(&world, edit);
    assert_eq!(dwell_of(&world), (CycleChoice::Bore, 0.5));
    history.execute(&world, GenerateOperation::new(&world, OP).expect("generate bore"));

    let e = model::find_by_name(&world, OP).expect("operation entity");
    let t = model::toolpath_of(&world, &e).expect("generated toolpath").path;
    assert_eq!(t.cycles.len(), 1);
    let c = t.cycles[0];
    assert_eq!((c.kind, c.dwell), (CycleKind::Bore, 0.5), "the record carries the dwell to the post");
    let g = &t.moves[c.first_move..];
    assert_eq!((g[0].kind, g[0].feed), (MoveKind::Plunge, Some(90.0)), "feed in");
    assert_eq!(g[1].kind, MoveKind::Rapid, "rapid out");
    assert!(g[1].to.z > g[0].to.z, "the rapid is the retract");

    assert!(history.undo(&world), "undo generation");
    assert!(history.undo(&world), "undo the bore edit");
    assert_eq!(dwell_of(&world), (CycleChoice::Drill, 0.0), "undo restores the plain drill");
    assert!(history.redo(&world), "redo the bore edit");
    assert_eq!(dwell_of(&world), (CycleChoice::Bore, 0.5));
}

#[test]
fn an_insane_dwell_is_refused_by_name_at_command_creation() {
    let (world, mut history) = world_with_tool();
    for bad in [-1.0, MAX_DWELL_S + 1.0, f64::NAN, f64::INFINITY] {
        let err = CreateOperation::new(&world, OP, TOOL, SrcRef::Entity("block".into()), params(CycleChoice::Bore, bad))
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains("drilling dwell") && err.contains("seconds"), "dwell {bad}: {err}");
    }
    let create = CreateOperation::new(&world, OP, TOOL, SrcRef::Entity("block".into()), params(CycleChoice::Bore, MAX_DWELL_S))
        .expect("the bound itself is a usable dwell");
    history.execute(&world, create);
    let err = EditOperationParams::new(&world, OP, params(CycleChoice::Bore, MAX_DWELL_S * 2.0))
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(err.contains("drilling dwell"), "an edit is refused by name too: {err}");
    assert_eq!(dwell_of(&world), (CycleChoice::Bore, MAX_DWELL_S), "the refused edit changed nothing");
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
fn mcp_edit_operation_sets_a_bore_dwell_and_refuses_an_insane_one() {
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
    let (err, body) =
        mcp_call(&mut s, 10, "edit_operation", json!({ "name": "OP", "params": { "cycle": "bore", "dwell": 0.5 } }));
    assert!(!err, "edit_operation refused a bore: {body}");
    assert_eq!(body["params"]["drill"]["cycle"], json!("bore"), "{body}");
    assert_eq!(body["params"]["drill"]["dwell"], json!(0.5), "{body}");
    let (err, body) = mcp_call(&mut s, 11, "edit_operation", json!({ "name": "OP", "params": { "dwell": 99999.0 } }));
    assert!(err, "an insane dwell is refused: {body}");
    assert!(body.as_str().is_some_and(|t| t.contains("drilling dwell")), "by name: {body}");
}
