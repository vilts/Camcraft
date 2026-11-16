//! `lay_part_on_face` (#729), driven in-process over the server's JSON-RPC
//! handler: a box laid on a picked side face through
//! `AlignPartToSetup::lay_on_plane`, one undo step, every refusal named.
//!
//! Gate map (M4.4): covers app::mcp::align_tools (#729),
//! app::commands (AlignPartToSetup::lay_on_plane), app::pick (plane).

use camcraft_app::mcp::Server;

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-layface-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A `2x` by `2y` by `z` box as ASCII STL, centered in XY, z from 0.
fn write_box_stl(path: &Path, (x, y, z): (f64, f64, f64)) {
    let v = |i: usize| -> [f64; 3] {
        [if i & 1 == 0 { -x } else { x }, if i & 2 == 0 { -y } else { y }, if i & 4 == 0 { 0.0 } else { z }]
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

/// The face id whose bounds lie on `axis` (0..3) at exactly `v`.
fn face_at(f: &Value, axis: usize, v: f64) -> String {
    f["faces"]
        .as_array()
        .and_then(|fs| fs.iter().find(|fa| fa["bounds"]["min"][axis] == json!(v) && fa["bounds"]["max"][axis] == json!(v)))
        .and_then(|fa| fa["id"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("no face at axis {axis} = {v}: {f}"))
}

#[test]
fn lay_part_on_face_lands_the_face_undoes_and_refuses_by_name() {
    let dir = tmp_dir("box");
    let part = dir.join("block.stl");
    let fixture = dir.join("vise.stl");
    write_box_stl(&part, (20.0, 20.0, 15.0));
    write_box_stl(&fixture, (40.0, 30.0, 15.0));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("import_model", json!({ "path": fixture.display().to_string() }));
    c.ok("add_fixture", json!({ "model": "vise" }));
    c.ok("add_setup", json!({}));
    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    assert!(names.iter().any(|n| n == "lay_part_on_face"), "{names:?}");

    let f = c.ok("list_features", json!({ "model": "block", "kinds": ["faces", "chains", "vertices", "patches"] }));
    let wall = json!({ "model": "block", "id": face_at(&f, 1, -20.0) });

    // The front wall (y = -20) has outside -Y; flip lays it on the table.
    let laid = c.ok("lay_part_on_face", json!({ "face": wall, "flip": true }));
    assert!(laid["alignment"].is_object(), "{laid}");
    assert!(laid["outside"].as_str().is_some_and(|s| s.contains("-Y")), "{laid}");
    assert!(laid["description"].as_str().is_some_and(|s| !s.is_empty()), "{laid}");
    // The front wall turns a quarter turn about X onto the table.
    let q = laid["alignment"]["rotation"].as_array().expect("rotation");
    let (qx, qw) = (q[0].as_f64().unwrap_or(0.0), q[3].as_f64().unwrap_or(0.0));
    let half = std::f64::consts::FRAC_1_SQRT_2;
    assert!((qx - half).abs() < 1e-9 && (qw - half).abs() < 1e-9, "{laid}");
    assert!(laid["description"].as_str().is_some_and(|d| d.contains("X 90")), "{laid}");

    // The same lay again is refused by name; the failed call adds no step.
    let again = c.refused("lay_part_on_face", json!({ "face": wall, "flip": true }));
    assert!(again.contains("already holds the part that way"), "{again}");

    // One undo step takes the lay back (the same lay is accepted again and
    // stores the same alignment); redo lands it, so it is held again.
    c.ok("undo", json!({}));
    let relaid = c.ok("lay_part_on_face", json!({ "face": wall, "flip": true }));
    assert_eq!(relaid["alignment"], laid["alignment"], "the same lay stores the same alignment");
    c.ok("undo", json!({}));
    c.ok("redo", json!({}));
    let held = c.refused("lay_part_on_face", json!({ "face": wall, "flip": true }));
    assert!(held.contains("already holds the part that way"), "redo lands the lay: {held}");
    c.ok("undo", json!({}));

    // g18 and place:false take the same options as lay_part_on_points.
    c.ok("lay_part_on_face", json!({ "face": wall, "plane": "g18", "place": false }));
    c.ok("undo", json!({}));

    // Refusals, by name.
    let edge = f["chains"][0]["id"].as_str().expect("an edge chain");
    let msg = c.refused("lay_part_on_face", json!({ "face": { "model": "block", "id": edge } }));
    assert!(msg.contains("is an edge"), "{msg}");
    let vertex = f["vertices"][0]["id"].as_str().expect("a vertex");
    let msg = c.refused("lay_part_on_face", json!({ "face": { "model": "block", "id": vertex } }));
    assert!(msg.contains("is a vertex"), "{msg}");
    let top = json!({ "model": "block", "id": face_at(&f, 2, 15.0) });
    let msg = c.refused("lay_part_on_face", json!({ "face": [wall, top] }));
    assert!(msg.contains("pick one flat face"), "{msg}");
    let fixture_faces = c.ok("list_features", json!({ "model": "vise", "kinds": ["faces"] }));
    let off = json!({ "model": "vise", "id": face_at(&fixture_faces, 2, 15.0) });
    let msg = c.refused("lay_part_on_face", json!({ "face": off }));
    assert!(msg.contains("is a fixture"), "{msg}");
    let msg = c.refused("lay_part_on_face", json!({ "face": wall, "plane": "g20" }));
    assert!(msg.contains("\"plane\""), "{msg}");
    let msg = c.refused("lay_part_on_face", json!({}));
    assert!(msg.contains("\"face\" is required"), "{msg}");
}
