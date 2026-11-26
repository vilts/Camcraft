//! #3448: Detect holes on a machine with no turret keeps the holes the
//! drill reaches along the setup's Z and names each radial (cross) hole
//! in `skipped`. It never fills the hidden `cross` list, so the op
//! generates instead of refusing "Z holes AND cross holes".
//!
//! Gate map: covers camcraft_app::operation::cross_reach,
//! camcraft_app::operation::material (detect_holes) and
//! camcraft_app::mcp::edit (detect_holes).
//!
//! Provenance: the issue's MCP reproduction rebuilt on our own synthetic
//! part (a plate with two D6 Z holes beside a bar with one blind D6
//! radial hole); no vendor material consulted.

mod punched_bar;

use std::path::{Path, PathBuf};

use camcraft_app::drilling::CrossHole;
use camcraft_app::geometry::Point3;
use camcraft_app::mcp::Server;
use serde_json::{json, Value};

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
}

type P = [f64; 3];

/// Half a plate cell, the plate's top and its X offset from the bar.
const H: f64 = 20.0;
const TOP: f64 = 10.0;
const AT: f64 = 200.0;
const N: usize = 64;

/// A 10 mm plate of `cells` 40 x 40 cells along X, each drilled through
/// at its centre with a D6 hole; the hole vertices sit on the cell's
/// square (N a multiple of 8), so neighbouring cells share their edges.
fn plate(cells: usize) -> Vec<[P; 3]> {
    let mut tris: Vec<[P; 3]> = Vec::new();
    let mut quad = |a: P, b: P, c: P, d: P, want: P| {
        for t in [[a, b, c], [a, c, d]] {
            let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
            let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let dot = n[0] * want[0] + n[1] * want[1] + n[2] * want[2];
            tris.push(if dot >= 0.0 { t } else { [t[0], t[2], t[1]] });
        }
    };
    let r = 3.0;
    for k in 0..cells {
        let cx = AT + (2.0 * k as f64 + 1.0) * H - cells as f64 * H;
        let ang = |i: usize| std::f64::consts::TAU * (i % N) as f64 / N as f64;
        let bp = |i: usize, z: f64| [cx + r * ang(i).cos(), r * ang(i).sin(), z];
        let sq = |i: usize, z: f64| {
            let (x, y) = (ang(i).cos(), ang(i).sin());
            let s = x.abs().max(y.abs());
            [cx + x * H / s, y * H / s, z]
        };
        for i in 0..N {
            let j = i + 1;
            quad(bp(i, TOP), bp(j, TOP), sq(j, TOP), sq(i, TOP), [0.0, 0.0, 1.0]);
            quad(bp(i, 0.0), bp(j, 0.0), sq(j, 0.0), sq(i, 0.0), [0.0, 0.0, -1.0]);
            let mid = ang(i) + std::f64::consts::PI / N as f64;
            quad(bp(i, TOP), bp(j, TOP), bp(j, 0.0), bp(i, 0.0), [-mid.cos(), -mid.sin(), 0.0]);
            let (a, b) = (sq(i, TOP), sq(j, TOP));
            let shared_left = k > 0 && (a[0] - (cx - H)).abs() < 1e-9 && (b[0] - (cx - H)).abs() < 1e-9;
            let shared_right = k + 1 < cells && (a[0] - (cx + H)).abs() < 1e-9 && (b[0] - (cx + H)).abs() < 1e-9;
            if !(shared_left || shared_right) {
                let out = [(a[0] + b[0]) * 0.5 - cx, (a[1] + b[1]) * 0.5, 0.0];
                quad(a, b, sq(j, 0.0), sq(i, 0.0), out);
            }
        }
    }
    tris
}

/// The bar of radius 25 about world Z with one blind D6 radial hole.
fn bar() -> Vec<[P; 3]> {
    let hole = CrossHole {
        origin: Point3::new(25.0, 0.0, 0.0),
        dir: Point3::new(-1.0, 0.0, 0.0),
        radius: 3.0,
        entry_d: 0.0,
        floor_d: -8.0,
        through: false,
    };
    punched_bar::punched_bar(25.0, -40.0, 20.0, &[hole])
        .facets
        .iter()
        .map(|t| [[t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]])
        .collect()
}

fn write_stl(dir: &Path, name: &str, tris: &[[P; 3]]) -> PathBuf {
    let path = dir.join(format!("{name}.stl"));
    let mut s = format!("solid {name}\n");
    for t in tris {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in t {
            s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str(&format!("endsolid {name}\n"));
    std::fs::write(&path, s).expect("write stl");
    path
}

/// One model on a 3-axis mill (no turret), its own stock, the D6
/// catalog drill and a Drill op: the issue's MCP sequence up to
/// `detect_holes`, whose answer is returned.
fn detect(tag: &str, tris: &[[P; 3]]) -> (Client, bool, Value) {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-detect-mill-radial-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = write_stl(&dir, "part", tris);
    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("import_model", json!({ "path": stl.display().to_string(), "name": "part" }));
    c.ok("set_stock", json!({ "from_model": "part" }));
    c.ok("add_tool", json!({ "catalog": "DR-0600" }));
    c.ok("add_operation", json!({ "kind": "drill", "tool": "DR-0600", "source": "part", "name": "D" }));
    let (err, got) = c.call("detect_holes", json!({ "name": "D" }));
    (c, err, got)
}

#[test]
fn a_mill_skips_radial_holes_by_name_and_the_z_holes_generate() {
    let mut tris = plate(2);
    tris.extend(bar());
    let (mut c, err, got) = detect("mixed", &tris);
    assert!(!err, "detect_holes refused: {got}");
    assert_eq!(got["holes"], json!(2), "the two Z holes stay: {got}");
    assert_eq!(got["radial_holes"], json!(0), "a mill has no turret for cross holes: {got}");
    let skipped = got["skipped"].as_array().expect("skipped list");
    assert_eq!(skipped.len(), 1, "the radial hole is named: {got}");
    let note = skipped[0].as_str().unwrap_or_default();
    assert!(note.contains("radial") && note.contains("6.000 mm"), "{note}");
    let g = c.ok("generate", json!({ "name": "D" }));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
}

#[test]
fn a_mill_with_only_radial_holes_refuses_detection_by_name() {
    let (_, err, got) = detect("radial-only", &bar());
    assert!(err, "nothing a mill can drill must not be stored: {got}");
    let text = got.as_str().unwrap_or_default();
    assert!(text.contains("radial") && text.contains("turret"), "{text}");
}
