//! The Deburr edge filters through MCP (#1888): `edit_operation` sets
//! the minimum edge angle, the hole filter and the excluded edges, and
//! `set_operation_geometry` sets the edge chains to break.
//!
//! Gate map (#1888): covers app::mcp (pick, edit),
//! app::project (the Deburr params DTO), app::commands
//! (SetOperationGeometry, EditOperationParams), app::operation::tool_gate
//! (#3368: a flat or ball taper tip refused at generation).

use camcraft_app::geometry::{Point3, cube};
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

/// A 40 x 40 x 10 brick as ASCII STL, a stock box over it, the
/// 90-degree chamfer mill and a Deburr of the whole part.
fn deburr_job(tag: &str) -> (Client, PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-mcp-deburr-{tag}-{}", std::process::id()));
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
    let imp = c.ok("import_model", json!({ "path": stl.display().to_string() }));
    assert_eq!(imp["model"], json!("part"));
    c.ok("set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "CM-0600-90" }));
    let add = c.ok("add_operation", json!({ "kind": "deburr", "tool": "CM-0600-90", "name": "D1", "source": "part" }));
    assert_eq!(add["operation"], json!("D1"));
    (c, dir)
}

#[test]
fn deburr_takes_edge_chains_and_its_filters_through_mcp() {
    let (mut c, dir) = deburr_job("filters");
    let f = c.ok("list_features", json!({ "model": "part", "kinds": ["chains"] }));
    let chain = f["chains"][0]["id"].as_str().expect("a chain id").to_owned();

    // An edge chain is a pick: the source becomes the picked edge.
    let set = c.ok("set_operation_geometry", json!({ "operation": "D1", "picks": [{ "model": "part", "id": chain }] }));
    assert_eq!(set["source"]["picks"][0]["id"], json!(chain), "{set}");
    c.ok("undo", json!({}));

    // The filters are parameters: one edit, one undo step.
    let id = u64::from_str_radix(chain.trim_start_matches("chain:0x"), 16).expect("a hex chain id");
    let edit = c.ok("edit_operation", json!({ "name": "D1", "params": {
        "min_edge_angle_deg": 30.0,
        "exclude_holes": true,
        "hole_min_diameter": 2.0,
        "hole_max_diameter": 9.0,
        "exclude_edges": [{ "entity": "part", "feature": { "chain": { "id": id } } }],
    } }));
    let job = &edit["params"];
    let text = job.to_string();
    for want in ["\"min_edge_angle_deg\":30.0", "\"exclude_holes\":true", "\"hole_max_diameter\":9.0", "\"exclude_edges\""] {
        assert!(text.contains(want), "the answer shows {want}: {text}");
    }

    // The command door, through the same tool: a bad range is refused.
    let why = c.refused("edit_operation", json!({ "name": "D1", "params": { "hole_min_diameter": 20.0 } }));
    assert!(!why.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

/// #3368: a Deburr whose chamfer mill is edited to a flat or ball tip
/// after the operation exists is refused by name at generation - no
/// toolpath reads the tip as a sharp cone. Back to sharp, it generates.
#[test]
fn a_deburr_refuses_its_tool_after_a_flat_tip_edit() {
    let (mut c, dir) = deburr_job("tip-form");
    c.ok("edit_tool", json!({ "name": "CM-0600-90", "patch": { "tip_form": "flat", "tip_diameter": 1.0 } }));
    let (err, body) = c.call("generate", json!({ "name": "D1" }));
    let text = body.to_string();
    assert!(text.contains("'CM-0600-90' has a flat tip"), "generation names the tip (error {err}): {text}");
    let job = c.ok("get_job", json!({}));
    assert_ne!(job["operations"][0]["status"], json!("ok"), "no toolpath for a flat tip: {job}");
    c.ok("undo", json!({}));
    let (err, body) = c.call("generate", json!({ "name": "D1" }));
    assert!(!body.to_string().contains("has a flat tip"), "the sharp tip generates (error {err}): {body}");
    let _ = std::fs::remove_dir_all(dir);
}
