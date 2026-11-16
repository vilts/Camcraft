//! #902 end to end: the document's unit is the program's unit. Over the
//! MCP tools (the same undoable commands the GUI queues): a face job
//! posts G21 in mm; `set_units` inch posts the same program as G20 in
//! inches; undo puts mm back; the unit is saved and reopened with the
//! project; an inch document on a post that cannot write inch is
//! refused by name.
//!
//! Provenance: our own tests over our own tools; G20 / G21 are RS-274 /
//! ISO 6983.
//!
//! Gate map (M4.4): covers app::mcp (units_tools, job get_job units),
//! app::commands (SetUnits), app::posting (program_options),
//! app::project (v60 units).

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-units-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A 40 x 40 x 15 box as ASCII STL, z from 0, wound outward.
fn write_box_stl(path: &Path) {
    let (x, y, z) = (20.0f64, 20.0f64, 15.0f64);
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
    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
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

    fn post(&mut self, path: &Path, post: &str) -> String {
        self.ok("post", json!({ "output": path.display().to_string(), "post": post }));
        std::fs::read_to_string(path).expect("program written")
    }
}

fn has_word(program: &str, word: &str) -> bool {
    program.lines().any(|l| l.split_whitespace().any(|w| w == word))
}

/// Every X word of the program, in order.
fn xs(program: &str) -> Vec<f64> {
    program
        .split_whitespace()
        .filter_map(|w| w.strip_prefix('X').and_then(|v| v.parse().ok()))
        .collect()
}

fn face_job(dir: &Path) -> Client {
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" }));
    let g = c.ok("generate", json!({}));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
    c
}

#[test]
fn the_document_unit_is_the_posted_unit_through_undo_and_save() {
    let dir = tmp_dir("job");
    let mut c = face_job(&dir);
    assert_eq!(c.ok("get_job", json!({}))["units"], json!("mm"));
    let mm = c.post(&dir.join("mm.nc"), "Fanuc");
    assert!(has_word(&mm, "G21") && !has_word(&mm, "G20"), "{mm}");

    assert_eq!(c.ok("set_units", json!({ "unit": "inch" }))["unit"], json!("inch"));
    assert_eq!(c.ok("get_job", json!({}))["units"], json!("inch"));
    let inch = c.post(&dir.join("inch.nc"), "Fanuc");
    assert!(has_word(&inch, "G20") && !has_word(&inch, "G21"), "{inch}");
    let (a, b) = (xs(&mm), xs(&inch));
    assert_eq!(a.len(), b.len());
    assert!(!a.is_empty());
    for (m, i) in a.iter().zip(&b) {
        assert!((m / 25.4 - i).abs() < 1e-4, "X{m} mm posted as X{i} in");
    }

    // An inch document on a post that cannot write inch: refused by name.
    let why = c.refused("post", json!({ "output": dir.join("carvera.nc").display().to_string(), "post": "Makera Carvera" }));
    assert!(why.contains("Makera Carvera") && why.contains("inch"), "{why}");

    // Saved and reopened, the document is still inch and posts inch.
    let proj = dir.join("job.camproj");
    c.ok("save_project", json!({ "path": proj.display().to_string() }));
    c.ok("set_units", json!({ "unit": "mm" }));
    c.ok("open_project", json!({ "path": proj.display().to_string() }));
    assert_eq!(c.ok("get_job", json!({}))["units"], json!("inch"));

    // Undo puts mm back - the history is the one truth.
    let mut c = face_job(&dir);
    c.ok("set_units", json!({ "unit": "inch" }));
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["units"], json!("mm"));
    assert_eq!(c.post(&dir.join("undone.nc"), "Fanuc"), mm);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn set_units_refuses_nonsense_and_no_ops() {
    let mut c = Client { server: Server::new(), next: 1 };
    assert!(c.refused("set_units", json!({ "unit": "mm" })).contains("already"));
    assert!(c.refused("set_units", json!({ "unit": "in" })).contains("\"mm\" or \"inch\""));
    assert!(c.refused("set_units", json!({})).contains("unit"));
}
