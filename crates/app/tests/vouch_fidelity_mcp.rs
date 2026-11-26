//! #1339: the MCP tool `vouch_model_fidelity`, driven in-process over the
//! server's JSON-RPC handler. A model marked "fidelity unknown" (a mesh of
//! a project saved before v59) refuses the post; a person vouches for the
//! mesh, the program posts, and undo refuses it again. One undo step, and
//! every bad input is a tool error that names it and enters no history.
//!
//! Gate map (#1339): covers app::mcp::vouch_tools,
//! app::commands::vouch_fidelity, app::model_vouch, app::mcp::job (get_job
//! `fidelity_vouched`), app::posting (the fidelity clause of the gate).
//! Lane shape (M4.4): every test here runs in seconds.
//!
//! Provenance: our own pins over our own laws; no vendor material.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Mesh, Point3};
use camcraft_app::mcp::Server;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model_fidelity, project};

use serde_json::{json, Value};
use std::path::PathBuf;

const MILL: &str = "EM-1000-4FL";
/// A chord that leaves a tolerance: the program still posts, tightened.
const TIGHT: f64 = 0.012;
const UNKNOWN: &str = "saved before CamCraft recorded mesh fidelity";

fn brick() -> Mesh {
    cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0))
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_radius: None,
        blend_turns: false,
        picks: Default::default(),
    }
}

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-1339-mcp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A project as an old file loads it: the part "bar" and the stock copied
/// from it are of unknown fidelity; "known" was imported at a recorded
/// coarse chord; a Face op finishes the flat top of "bar" exactly.
fn old_project(dir: &std::path::Path) -> PathBuf {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("bar", brick()).with_fidelity(Some(model_fidelity::UNKNOWN_MM)));
    h.execute(&world, ImportModel::from_mesh("known", brick()).with_fidelity(Some(TIGHT)));
    h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "bar".into() }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog mill");
    h.execute(&world, AddTool::new(&world, MILL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", MILL, SrcRef::Entity("bar".into()), face()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    let path = dir.join("old.camproj");
    project::save(&world, &path).expect("save");
    path
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        let mut c = Self { server: Server::new(), next: 1 };
        c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
        c
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.server.handle(&msg).expect("a request gets a response")
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        let is_err = result["isError"] == json!(true);
        let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
        (is_err, body)
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }

    fn model(&mut self, name: &str) -> Value {
        let job = self.ok("get_job", json!({}));
        let models = job["models"].as_array().cloned().unwrap_or_default();
        models.into_iter().find(|m| m["name"] == json!(name)).unwrap_or_else(|| panic!("no model {name}: {job}"))
    }

    /// The gate's problem lines, as `verify` reports them.
    fn gate(&mut self) -> Vec<String> {
        let report = self.ok("verify", json!({}));
        report["gate"]["problems"]
            .as_array()
            .map(|p| p.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect())
            .unwrap_or_else(|| panic!("verify reports the gate: {report}"))
    }
}

#[test]
fn a_vouched_model_posts_and_its_undo_is_refused_again() {
    let dir = tmp_dir("post");
    let project = old_project(&dir);
    let nc = dir.join("out.nc");
    let nc_arg = json!({ "output": nc.to_string_lossy() });
    let mut c = Client::new();
    c.ok("open_project", json!({ "path": project.to_string_lossy() }));

    // Before: refused by name at the gate and at the post.
    let before = c.gate();
    assert!(before.iter().any(|p| p.starts_with("OP: model 'bar' was") && p.contains(UNKNOWN)), "{before:?}");
    let said = c.refused("post", nc_arg.clone());
    assert!(said.contains(UNKNOWN), "{said}");
    assert!(!nc.exists(), "a refused post writes nothing");
    assert_eq!(c.model("bar")["reduced_fidelity"]["fidelity_unknown"], json!(true));

    // The tool is listed, and it vouches for the model and its stock copy.
    let names = c.request("tools/list", json!({}))["result"]["tools"].to_string();
    assert!(names.contains("\"vouch_model_fidelity\""), "the tool is listed");
    let out = c.ok("vouch_model_fidelity", json!({ "model": "bar", "by": "M. Kinney" }));
    assert_eq!(out["model"], json!("bar"));
    assert_eq!(out["by"], json!("M. Kinney"));
    assert_eq!(out["stock_copies"], json!(["Stock"]));

    // After: the gate passes, the model says who vouched, the post writes.
    assert_eq!(c.gate(), Vec::<String>::new());
    let bar = c.model("bar");
    assert!(bar.get("reduced_fidelity").is_none(), "{bar}");
    assert_eq!(bar["fidelity_vouched"]["by"], json!("M. Kinney"));
    c.ok("post", nc_arg.clone());
    let text = std::fs::read_to_string(&nc).expect("the posted file");
    assert!(text.lines().count() > 5, "a real program: {text}");
    std::fs::remove_file(&nc).expect("remove");

    // One undo step puts the mark back: refused again, nothing written.
    c.ok("undo", json!({}));
    let undone = c.gate();
    assert_eq!(undone, before, "undo restores the exact refusal");
    let said = c.refused("post", nc_arg.clone());
    assert!(said.contains(UNKNOWN), "{said}");
    assert!(!nc.exists(), "a refused post writes nothing");
    let bar = c.model("bar");
    assert_eq!(bar["reduced_fidelity"]["fidelity_unknown"], json!(true));
    assert!(bar.get("fidelity_vouched").is_none(), "{bar}");

    // Redo vouches again.
    c.ok("redo", json!({}));
    assert_eq!(c.gate(), Vec::<String>::new());
    assert_eq!(c.model("bar")["fidelity_vouched"]["by"], json!("M. Kinney"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_tool_refuses_by_name_and_enters_no_history() {
    let dir = tmp_dir("refuse");
    let project = old_project(&dir);
    let mut c = Client::new();
    c.ok("open_project", json!({ "path": project.to_string_lossy() }));

    let said = c.refused("vouch_model_fidelity", json!({ "model": "bar" }));
    assert!(said.contains("\"by\" is required"), "{said}");
    let said = c.refused("vouch_model_fidelity", json!({ "model": "bar", "by": "   " }));
    assert!(said.contains("who vouches"), "{said}");
    let said = c.refused("vouch_model_fidelity", json!({ "model": "nope", "by": "A" }));
    assert!(said.contains("nope"), "{said}");
    // A recorded coarse chord (#864) is known: it is never vouched away.
    let said = c.refused("vouch_model_fidelity", json!({ "model": "known", "by": "A" }));
    assert!(said.contains("'known'") && said.contains("recorded") && said.contains("re-import"), "{said}");
    // The stock copy is not a model: its source is what a person vouches for.
    let said = c.refused("vouch_model_fidelity", json!({ "model": "Stock", "by": "A" }));
    assert!(said.contains("'Stock' is not a model"), "{said}");

    // No refusal entered the history: there is nothing to undo, and the
    // mark is where it was.
    assert_eq!(c.model("bar")["reduced_fidelity"]["fidelity_unknown"], json!(true));
    let (err, body) = c.call("undo", json!({}));
    assert!(err || body["undone"] == json!(false), "nothing to undo: {body}");
    assert_eq!(c.model("known")["reduced_fidelity"]["approximation_mm"], json!(TIGHT));

    // A second vouch of a model that is no longer marked is refused too.
    c.ok("vouch_model_fidelity", json!({ "model": "bar", "by": "A" }));
    let said = c.refused("vouch_model_fidelity", json!({ "model": "bar", "by": "B" }));
    assert!(said.contains("'bar'") && said.contains("not marked fidelity unknown"), "{said}");
    let _ = std::fs::remove_dir_all(&dir);
}
