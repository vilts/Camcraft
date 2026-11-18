//! #2174: the MCP `turn_fixture` tool, driven in-process over the
//! server's JSON-RPC handler - a quarter turn lands exact, undo walks it
//! back, the collision box reads the turned steel, and bad input is a
//! tool error in plain English.
//!
//! Gate map (M4.4): covers app::mcp::fixture_turn_tools,
//! app::commands::fixture_turn.

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-turn-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A box from the origin to `(x, y, z)` as ASCII STL.
fn write_box_stl(path: &Path, (x, y, z): (f64, f64, f64)) {
    let v = |i: usize| -> [f64; 3] {
        [if i & 1 == 0 { 0.0 } else { x }, if i & 2 == 0 { 0.0 } else { y }, if i & 4 == 0 { 0.0 } else { z }]
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let mut s = String::from("solid box\n");
    for q in quads {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            s.push_str(" facet normal 0 0 0\n  outer loop\n");
            for i in tri {
                let p = v(i);
                s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
            }
            s.push_str("  endloop\n endfacet\n");
        }
    }
    s.push_str("endsolid box\n");
    std::fs::write(path, s).expect("write stl");
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
}

fn bounds(c: &mut Client) -> Value {
    let f = c.ok("get_fixture_findings", json!({}));
    f["workholding"][0]["fixtures"][0]["bounds"].clone()
}

/// Each component of `v` lies within float dust of `want`.
fn near(v: &Value, want: [f64; 3], what: &str) {
    for (i, w) in want.iter().enumerate() {
        let got = v[i].as_f64().unwrap_or(f64::NAN);
        assert!((got - w).abs() < 1e-9, "{what}: {v} vs {want:?}");
    }
}

#[test]
fn turn_fixture_turns_by_a_quarter_undo_walks_it_back_and_bad_input_is_refused() {
    let dir = tmp_dir("quarter");
    let block = dir.join("block.stl");
    write_box_stl(&block, (10.0, 20.0, 30.0));
    let mut c = Client::new();
    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    assert!(names.iter().any(|n| n == "turn_fixture"), "{names:?}");

    c.ok("import_model", json!({ "path": block.display().to_string() }));
    c.ok("add_fixture", json!({ "model": "block" }));
    let before = bounds(&mut c);
    let turned = c.ok("turn_fixture", json!({ "name": "block", "axis": "z", "degrees": 90.0 }));
    assert_eq!(turned["rotation"], json!([0.0, 0.0, H, H]), "exact: {turned}");
    assert_eq!(turned["fixture"], json!("block"));
    let after = bounds(&mut c);
    near(&after["min"], [-20.0, 0.0, 0.0], "the box is the turned block");
    near(&after["max"], [0.0, 10.0, 30.0], "the box is the turned block");
    c.ok("undo", json!({}));
    assert_eq!(bounds(&mut c), before, "undo puts the block back");

    let e = c.refused("turn_fixture", json!({ "name": "block", "axis": "w", "degrees": 90.0 }));
    assert!(e.contains("\"axis\" is \"x\", \"y\" or \"z\""), "{e}");
    let e = c.refused("turn_fixture", json!({ "name": "block", "axis": "x", "degrees": 0.0 }));
    assert!(e.contains("leaves 'block' as it stands"), "{e}");
    let e = c.refused("turn_fixture", json!({ "name": "ghost", "axis": "x", "degrees": 90.0 }));
    assert!(e.contains("ghost"), "{e}");
}
