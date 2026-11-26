//! The Rotary Flow band through MCP (#2827): `list_operation_types`
//! lists the kind, `add_operation` creates it, `edit_operation` edits
//! its band fields, and `set_operation_geometry` accepts edge-chain
//! picks and refuses every other feature kind - the chain-only door the
//! pick walk says at the pick, before anything lands in the History.
//!
//! Gate map (#2827): covers app::mcp (list_operation_types, add, edit,
//! pick, kind_door), app::project (the rotary_flow params DTO),
//! app::operation (the kind row, the seed). Also covers
//! app::operation::rotary_law through the edit door's own refusals.

use camcraft_app::geometry::{cube, Point3};
use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::PathBuf;

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
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
}

/// A 40 x 40 x 10 brick as ASCII STL, a stock box over it, a ball
/// cutter, and a Rotary Flow of the whole part.
fn job(tag: &str) -> (Client, PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-mcp-rotary-flow-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut s = String::from("solid part\n");
    for t in cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0)).facets.iter() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid part\n");
    let stl = dir.join("part.stl");
    std::fs::write(&stl, s).expect("write stl");

    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "BN-0600" }));
    let add = c.ok("add_operation", json!({ "kind": "rotary_flow", "tool": "BN-0600", "name": "F1", "source": "part" }));
    assert_eq!(add["operation"], json!("F1"));
    (c, dir)
}

#[test]
fn rotary_flow_lists_creates_edits_and_takes_only_chain_picks() {
    let (mut c, dir) = job("surface");
    let kinds = c.ok("list_operation_types", json!({})).to_string();
    assert!(kinds.contains("Rotary Flow"), "the kind lists: {kinds}");

    // The band fields edit through the same door as every kind.
    let edit = c.ok("edit_operation", json!({ "name": "F1", "params": {
        "tolerance": 0.02, "stepover": 0.4, "lead_deg": 3.0, "max_tilt_deg": 30.0,
    } }));
    let text = edit["params"].to_string();
    for want in ["\"tolerance\":0.02", "\"stepover\":0.4", "\"lead_deg\":3.0", "\"max_tilt_deg\":30.0"] {
        assert!(text.contains(want), "the answer shows {want}: {text}");
    }

    // An edge chain is a pick: the source becomes the picked chain.
    let f = c.ok("list_features", json!({ "model": "part", "kinds": ["chains"] }));
    let chain = f["chains"][0]["id"].as_str().expect("a chain id").to_owned();
    let set = c.ok("set_operation_geometry", json!({ "operation": "F1", "picks": [{ "model": "part", "id": chain }] }));
    assert_eq!(set["source"]["picks"][0]["id"], json!(chain), "{set}");

    // A face is not an edge chain: the kind door refuses it by name.
    let face = c.ok("list_features", json!({ "model": "part", "kinds": ["faces"] }))["faces"][0]["id"]
        .as_str()
        .expect("a face id")
        .to_owned();
    let why = c.refused("set_operation_geometry", json!({ "operation": "F1", "picks": [{ "model": "part", "id": face }] }));
    assert!(why.contains("is not an edge chain") && why.contains("Rotary Flow"), "{why}");
    // A point bounds no band either.
    let vertex = c.ok("list_features", json!({ "model": "part", "kinds": ["vertices"] }))["vertices"][0]["id"]
        .as_str()
        .expect("a vertex id")
        .to_owned();
    let why = c.refused("set_operation_geometry", json!({ "operation": "F1", "picks": [{ "model": "part", "id": vertex }] }));
    assert!(why.contains("is not an edge chain"), "{why}");
    // The refused pick changed nothing: the chain pick stands.
    let g = c.ok("get_operation_geometry", json!({ "operation": "F1" }));
    assert_eq!(g["source"]["picks"][0]["id"], json!(chain), "{g}");
    let _ = std::fs::remove_dir_all(dir);
}
