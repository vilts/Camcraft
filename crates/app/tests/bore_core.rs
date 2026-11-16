//! #1566: wide bores clear concentric rings. Detect holes keeps them,
//! hand-entered holes generate, and verification accepts the paths.
//!
//! Gate map: covers app::operation::bore_reach, app::operation::material
//! (detect_holes), app::mcp::edit (detect_holes) and the Bore arm of
//! app::operation::generate, over the motion kernel's bore door; and
//! (#1560) app::operation::hole_digest through save_project/open_project.
//!
//! Provenance: the issue's own MCP reproduction on a synthetic plate
//! (a Ø74.3 through bore beside a Ø6 hole, a 4 mm end mill); no vendor
//! material consulted.

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

const BIG_R: f64 = 37.15;
const SMALL_R: f64 = 3.0;
const H: f64 = 50.0;
const TOP: f64 = 10.0;
const N: usize = 64;

type P = [f64; 3];

/// A plate of 100 x 100 cells along X, 10 mm thick, each cell bored
/// through at its centre with the given radius. Each bore's vertices are
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

fn write_stl(tag: &str, radii: &[f64]) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-bore-core-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
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

/// The plate imported, its own stock, a 4 mm end mill and a Bore op.
/// The 0.1 mm wall allowance keeps the nominal circle inside the faceted STL wall.
fn setup(tag: &str, radii: &[f64]) -> Client {
    let stl = write_stl(tag, radii);
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": stl.display().to_string(), "name": "plate" }));
    c.ok("set_stock", json!({ "from_model": "plate" }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL", "name": "EM4" }));
    c.ok("add_operation", json!({ "kind": "bore", "tool": "EM4", "source": "plate", "name": "B", "params": { "stepdown": 2.0, "stock_to_leave": 0.1 } }));
    c
}

fn holes(c: &mut Client) -> Vec<Value> {
    let job = c.ok("get_job", json!({}));
    job["operations"][0]["params"]["bore"]["holes"].as_array().cloned().unwrap_or_default()
}

#[test]
fn detect_holes_keeps_wide_bores_and_the_generated_paths_verify() {
    let mut c = setup("detect", &[BIG_R, SMALL_R]);
    let got = c.ok("detect_holes", json!({ "name": "B" }));
    assert_eq!(got["holes"], json!(2), "both holes clear with rings: {got}");
    assert_eq!(got["skipped"], json!([]), "{got}");
    assert_eq!(holes(&mut c).len(), 2);
    c.ok("generate", json!({ "name": "B" }));
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(true), "the wide bore verifies: {v}");
}

#[test]
fn detect_holes_with_only_a_wide_bore_keeps_it_and_leaves_drill_detection_unchanged() {
    let mut c = setup("only", &[BIG_R]);
    let got = c.ok("detect_holes", json!({ "name": "B" }));
    assert_eq!(got["holes"], json!(1), "{got}");
    assert_eq!(got["skipped"], json!([]), "{got}");
    c.ok("add_tool", json!({ "catalog": "DR-0400", "name": "DR4" }));
    c.ok("add_operation", json!({ "kind": "drill", "tool": "DR4", "source": "plate", "name": "D" }));
    let got = c.ok("detect_holes", json!({ "name": "D" }));
    assert_eq!(got["holes"], json!(1), "{got}");
    assert_eq!(got["skipped"], json!([]), "{got}");
}

#[test]
fn a_bore_op_accepts_a_wide_hole_entered_by_hand() {
    let mut c = setup("byhand", &[BIG_R, SMALL_R]);
    c.ok("detect_holes", json!({ "name": "B" }));
    let mut list = holes(&mut c);
    list.retain(|h| h["radius"].as_f64().is_some_and(|r| r < 4.0));
    let mut wide = list[0].clone();
    wide["radius"] = json!(BIG_R);
    wide["xy"] = json!([-50.0, 0.0]);
    list.push(wide);
    c.ok("edit_operation", json!({ "name": "B", "params": { "holes": list } }));
    assert_eq!(holes(&mut c).len(), 2);
    let g = c.ok("generate", json!({ "name": "B" }));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(true), "the hand-entered wide bore verifies: {v}");
}

#[test]
fn a_wide_bull_nose_through_bore_detects_generates_and_verifies() {
    let stl = write_stl("bull", &[17.75, 8.0]);
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": stl.display().to_string(), "name": "plate" }));
    c.ok("set_stock", json!({ "from_model": "plate" }));
    c.ok("add_tool", json!({ "name": "BN12", "definition": {
        "kind": "bull_nose", "diameter": 12.0, "corner_r": 2.0, "flute_len": 30.0, "shank_d": 12.0,
        "shank_len": 40.0, "holder_d": 44.0, "holder_len": 35.0, "material_group": "general",
        "rpm": 8000.0, "feed": 800.0 } }));
    c.ok("add_operation", json!({ "kind": "bore", "tool": "BN12", "source": "plate", "name": "B", "params": { "stepdown": 2.0 } }));
    let got = c.ok("detect_holes", json!({ "name": "B" }));
    assert_eq!(got["holes"], json!(2), "{got}");
    assert_eq!(got["skipped"], json!([]), "{got}");
    c.ok("generate", json!({ "name": "B" }));
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(true), "{v}");
}

/// #1560: a hole floor typed off the 1 um lattice (`0.190 in - 0.040 in`
/// = 3.8099999999999996 mm) is stored by the project file as 3.81. The
/// op generated Ok before the save reopens Ok - the stamp digests the
/// hole in the form the file keeps - and posts the same program.
#[test]
fn an_edited_hole_floor_off_the_lattice_reopens_ok_and_posts_the_same() {
    let mut c = setup("reopen", &[SMALL_R]);
    c.ok("detect_holes", json!({ "name": "B" }));
    let typed: f64 = 0.190 * 25.4 - 0.040 * 25.4;
    assert_ne!(typed.to_bits(), 3.81f64.to_bits(), "the probe must be off the lattice");
    let mut list = holes(&mut c);
    assert_eq!(list.len(), 1, "{list:?}");
    list[0]["floor_z"] = json!(typed);
    list[0]["through"] = json!(false);
    c.ok("edit_operation", json!({ "name": "B", "params": { "holes": list } }));
    let g = c.ok("generate", json!({ "name": "B" }));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");

    let dir = std::env::temp_dir().join(format!("camcraft-bore-core-reopen-{}", std::process::id()));
    let nc = |c: &mut Client, tag: &str| {
        let out = dir.join(format!("{tag}.nc"));
        c.ok("post", json!({ "output": out.display().to_string(), "post": "Fanuc" }));
        std::fs::read_to_string(&out).unwrap_or_default()
    };
    let before = nc(&mut c, "before");
    assert!(!before.is_empty(), "the bore posts");

    let proj = dir.join("p.camproj");
    c.ok("save_project", json!({ "path": proj.display().to_string() }));
    let mut fresh = Client::new();
    fresh.ok("open_project", json!({ "path": proj.display().to_string() }));
    let job = fresh.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["status"], json!("ok"), "saved Ok, reopened Ok: {job}");
    let after = nc(&mut fresh, "after");
    assert_eq!(before, after, "the reopened job posts the program it saved");
    let _ = std::fs::remove_dir_all(&dir);
}
