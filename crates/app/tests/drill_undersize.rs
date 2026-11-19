//! #2496: a Drill op on holes wider than its drill names each one - at
//! Detect holes (`undersize`) and on the toolpath (`drill_undersize`,
//! advisory: a pilot drill is valid, so the post writes). A hole at the
//! drill's own size names nothing, and a hole narrower than the drill is
//! a cut into the part wall that the gouge law holds.
//!
//! Gate map: covers app::operation::drill_size, app::operation::material
//! (detect_holes), app::mcp::edit (detect_holes), app::mcp::job
//! (findings) and the Drill arm of app::operation::generate, over
//! sim::verify (the DrillUndersize warning).
//!
//! Provenance: the issue's own MCP reproduction (D6, D8 and D12 through
//! holes, a 6 mm catalog drill) on a synthetic plate; no vendor material
//! consulted.

use std::path::PathBuf;

use camcraft_app::mcp::Server;
use serde_json::{json, Value};

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        Self { server: Server::new(), next: 1 }
    }

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

const H: f64 = 20.0;
const TOP: f64 = 10.0;
const N: usize = 64;

type P = [f64; 3];

/// A plate of 40 x 40 cells along X, 10 mm thick, each cell drilled
/// through at its centre with the given radius. Each hole's vertices are
/// pushed radially onto its cell's square (N a multiple of 8, so the
/// corners are hit), so neighbouring cells share their edge vertices and
/// the mesh closes. Facets are wound by their wanted normal.
fn plate(radii: &[f64]) -> Vec<[P; 3]> {
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
    let cells = radii.len();
    for (k, &r) in radii.iter().enumerate() {
        let cx = (2.0 * k as f64 + 1.0) * H - cells as f64 * H;
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
            // The outer wall, except where this cell meets its neighbour.
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

fn temp_dir(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-drill-undersize-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write_stl(dir: &std::path::Path, radii: &[f64]) -> PathBuf {
    let path = dir.join("plate.stl");
    let mut s = String::from("solid plate\n");
    for t in plate(radii) {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in t {
            s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid plate\n");
    std::fs::write(&path, s).expect("write stl");
    path
}

/// The plate imported, its own stock, the 6 mm catalog drill and a Drill
/// op with the holes detected: the issue's MCP sequence.
fn setup(dir: &std::path::Path, radii: &[f64]) -> (Client, Value) {
    let stl = write_stl(dir, radii);
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": stl.display().to_string(), "name": "plate" }));
    c.ok("set_stock", json!({ "from_model": "plate" }));
    c.ok("add_tool", json!({ "catalog": "DR-0600" }));
    c.ok("add_operation", json!({ "kind": "drill", "tool": "DR-0600", "source": "plate", "name": "D" }));
    let detected = c.ok("detect_holes", json!({ "name": "D" }));
    (c, detected)
}

fn findings(c: &mut Client, kind: &str) -> Vec<Value> {
    let v = c.ok("verify", json!({}));
    v["findings"].as_array().map(|f| f.iter().filter(|x| x["kind"] == json!(kind)).cloned().collect()).unwrap_or_default()
}

#[test]
fn holes_wider_than_the_drill_are_named_at_detect_and_on_the_path_and_the_post_writes() {
    let dir = temp_dir("issue");
    let (mut c, got) = setup(&dir, &[3.0, 4.0, 6.0]);
    assert_eq!(got["holes"], json!(3), "a pilot is valid: every hole stays in the list: {got}");
    assert_eq!(got["skipped"], json!([]), "{got}");
    let notes: Vec<String> = got["undersize"].as_array().expect("undersize list").iter().filter_map(|n| n.as_str().map(str::to_owned)).collect();
    assert_eq!(notes.len(), 2, "the D8 and the D12 hole: {notes:?}");
    for (note, want) in notes.iter().zip(["the 8.000 mm hole", "the 12.000 mm hole"]) {
        assert!(note.contains(want) && note.contains("6.000 mm drill"), "{note}");
    }
    let g = c.ok("generate", json!({ "name": "D" }));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
    let found = findings(&mut c, "drill_undersize");
    assert_eq!(found.len(), 2, "one warning per undersize hole: {found:?}");
    for f in &found {
        assert_eq!(f["blocks_post"], json!(false), "advisory: {f}");
        let m = f["message"].as_str().unwrap_or_default();
        assert!(m.contains("undersize hole") && m.contains("6.000 mm drill"), "{m}");
    }
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(true), "the advisory does not hold the post: {v}");
    let nc = dir.join("out.nc");
    c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(nc.exists(), "the program is written");
}

#[test]
fn a_hole_at_the_drills_own_size_names_nothing() {
    let dir = temp_dir("fit");
    let (mut c, got) = setup(&dir, &[3.0, 3.0]);
    assert_eq!(got["holes"], json!(2), "{got}");
    assert_eq!(got["undersize"], json!([]), "{got}");
    c.ok("generate", json!({ "name": "D" }));
    assert!(findings(&mut c, "drill_undersize").is_empty());
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(true), "{v}");
}

#[test]
fn a_hole_narrower_than_the_drill_is_held_as_a_gouge_not_named_undersize() {
    let dir = temp_dir("narrow");
    let (mut c, got) = setup(&dir, &[2.5]);
    assert_eq!(got["undersize"], json!([]), "{got}");
    c.ok("generate", json!({ "name": "D" }));
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(false), "a D6 drill in a D5 hole cuts the wall - the post is held: {v}");
    assert!(findings(&mut c, "gouge").iter().any(|f| f["blocks_post"] == json!(true)), "a blocking gouge: {v}");
    assert!(findings(&mut c, "drill_undersize").is_empty());
}
