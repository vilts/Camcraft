//! The MCP `set_part_axes` tool with both axes picked (#1329): the same
//! full 3D frame the Setups page's "Part axes" rows apply through
//! `AlignPartToSetup::align_from_axes` (#769) - a vertical edge as +Y
//! lays the part on its side, a left-handed pair lands it on its head
//! rather than being refused - driven in-process over the server's
//! JSON-RPC handler, one undo step each.
//!
//! Gate map (M4.4): covers app::mcp::align_tools (#1329).

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-align-axes-{tag}-{}", std::process::id()));
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

/// The part-local direction `d` as the returned alignment lays it:
/// rotated by its quaternion `[x, y, z, w]`.
fn laid(alignment: &Value, d: [f64; 3]) -> [f64; 3] {
    let q: Vec<f64> = alignment["rotation"].as_array().map(|r| r.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    assert_eq!(q.len(), 4, "a quaternion: {alignment}");
    let (v, w) = ([q[0], q[1], q[2]], q[3]);
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let t = cross(v, d).map(|c| 2.0 * c);
    let u = cross(v, t);
    [d[0] + w * t[0] + u[0], d[1] + w * t[1] + u[1], d[2] + w * t[2] + u[2]]
}

fn lands_on(alignment: &Value, local: [f64; 3], want: [f64; 3]) -> bool {
    laid(alignment, local).iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-9)
}

/// #1329: with both +Y and +X picked, `set_part_axes` takes the GUI's
/// full 3D path (`align_from_axes`, #769): a vertical edge as +Y lays
/// the part on its side, a left-handed pair is honoured literally (the
/// part lands on its head) rather than refused - each one undo step.
#[test]
fn set_part_axes_with_both_axes_aligns_the_full_3d_frame() {
    let dir = tmp_dir("axes3d");
    let part = dir.join("block.stl");
    write_box_stl(&part, (20.0, 20.0, 15.0));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("add_setup", json!({}));
    let f = c.ok("list_features", json!({ "model": "block", "kinds": ["vertices"] }));
    let pick = |at: [f64; 3]| -> Value {
        let id = f["vertices"]
            .as_array()
            .and_then(|vs| vs.iter().find(|v| v["at"] == json!(at)))
            .and_then(|v| v["id"].as_str())
            .expect("a vertex")
            .to_owned();
        json!({ "model": "block", "id": id })
    };
    let (a, b, d) = ([-20.0, -20.0, 0.0], [-20.0, -20.0, 15.0], [20.0, -20.0, 15.0]);
    let e = [-20.0, 20.0, 0.0];

    // +Y: the front-left upright, picked up; +X: the front top edge,
    // picked +X. Old +Z runs +Y, old +X stays +X, old -Y is the new top.
    let side = c.ok("set_part_axes", json!({ "y": [pick(a), pick(b)], "x": [pick(b), pick(d)] }));
    assert_eq!(side["frame"], json!("full_3d"), "{side}");
    assert!(side["turn_deg"].is_null(), "{side}");
    let al = &side["alignment"];
    assert!(lands_on(al, [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]), "old +Z runs +Y: {:?}", laid(al, [0.0, 0.0, 1.0]));
    assert!(lands_on(al, [1.0, 0.0, 0.0], [1.0, 0.0, 0.0]), "old +X runs +X: {:?}", laid(al, [1.0, 0.0, 0.0]));
    assert!(lands_on(al, [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]), "old -Y is the top: {:?}", laid(al, [0.0, -1.0, 0.0]));
    // One undo step back to as modelled.
    c.ok("undo", json!({}));
    let one = c.ok("set_part_axes", json!({ "y": [pick(a), pick(b)], "x": [pick(b), pick(d)] }));
    assert_eq!(one["alignment"], side["alignment"], "undo restored as modelled, so the same picks land the same");
    c.ok("undo", json!({}));

    // Left-handed (in-plane, clockwise): +Y along old +X, +X along old
    // +Y. The old in-plane tool refused it; now it lands on its head.
    let head = c.ok("set_part_axes", json!({ "y": [pick(b), pick(d)], "x": [pick(a), pick(e)] }));
    assert_eq!(head["frame"], json!("full_3d"), "{head}");
    let al = &head["alignment"];
    assert!(lands_on(al, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), "the +Y pick lands: {:?}", laid(al, [1.0, 0.0, 0.0]));
    assert!(lands_on(al, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]), "the +X pick lands: {:?}", laid(al, [0.0, 1.0, 0.0]));
    assert!(lands_on(al, [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]), "the old top faces down: {:?}", laid(al, [0.0, 0.0, 1.0]));
    c.ok("undo", json!({}));

    // One axis keeps the in-plane turn and reports its angle.
    let flat = c.ok("set_part_axes", json!({ "y": [pick(b), pick(d)] }));
    assert_eq!(flat["frame"], json!("in_plane"), "{flat}");
    assert_eq!(flat["turn_deg"].as_f64().map(f64::abs), Some(90.0), "{flat}");
    c.ok("undo", json!({}));

    // Picks running along each other leave no frame: refused by name.
    let msg = c.refused("set_part_axes", json!({ "y": [pick(b), pick(d)], "x": [pick(b), pick(d)] }));
    assert!(msg.contains("along each other"), "{msg}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #1347: `set_part_axes` with the same two picks again is no change -
/// refused by name ("already holds the part that way"), as the GUI's
/// Apply is, and no undo step is added: one undo is as modelled again.
#[test]
fn set_part_axes_again_with_the_same_picks_refuses_as_already_held() {
    let dir = tmp_dir("again");
    let part = dir.join("block.stl");
    write_box_stl(&part, (20.0, 20.0, 15.0));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("add_setup", json!({}));
    let f = c.ok("list_features", json!({ "model": "block", "kinds": ["vertices"] }));
    let pick = |at: [f64; 3]| -> Value {
        let id = f["vertices"]
            .as_array()
            .and_then(|vs| vs.iter().find(|v| v["at"] == json!(at)))
            .and_then(|v| v["id"].as_str())
            .expect("a vertex")
            .to_owned();
        json!({ "model": "block", "id": id })
    };
    let (a, b, d) = ([-20.0, -20.0, 0.0], [-20.0, -20.0, 15.0], [20.0, -20.0, 15.0]);
    let picks = json!({ "y": [pick(a), pick(b)], "x": [pick(b), pick(d)] });
    let side = c.ok("set_part_axes", picks.clone());
    assert!(side["alignment"].is_object(), "the first apply turns the part: {side}");
    let msg = c.refused("set_part_axes", picks);
    assert!(msg.contains("already holds the part that way"), "{msg}");
    c.ok("undo", json!({}));
    let held = c.ok("set_part_axes", json!({ "y": [pick(a), pick(b)], "x": [pick(b), pick(d)] }));
    assert_eq!(held["alignment"], side["alignment"], "one undo was as modelled: the picks turn the part again");
    let _ = std::fs::remove_dir_all(&dir);
}
