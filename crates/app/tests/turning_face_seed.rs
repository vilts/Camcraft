//! #1409: a NEW lathe Facing faces the stock allowance down to the
//! part's top - never the part itself. The seed used to take the mill
//! fold's Top of Model / Bottom of Model, so the facing plane stood on
//! the part's floor and the ladder climbed through the whole part.
//! Driven over the MCP server (`add_operation` creates from
//! `operation::seed_params`, the same retype the GUI quick-add runs),
//! with the issue's own repro: an r 10 x 48 bar in 20 x 50 cylinder
//! stock, 2 mm of allowance above the part.
//!
//! Gate map (M4.4): covers app::operation::kinds (seed_facing),
//! app::operation::seed, app::operation::heights_sel (depth_sane, Facing
//! arm), app::mcp::job (add_operation/edit_operation/generate/verify).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::mcp::Server;
use camcraft_app::operation::{seed_params, HeightReference, OpParams, OpType};

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-face-seed-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

fn repo(rel: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel).display().to_string()
}

/// A plain bar r `r`, z 0..`h`, as ASCII STL: the meridian
/// (0,0) (r,0) (r,h) (0,h) revolved in 64 segments.
fn write_bar_stl(path: &Path, r: f64, h: f64) {
    let n = 64usize;
    let p = |j: usize, rr: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / n as f64;
        [rr * th.cos(), rr * th.sin(), z]
    };
    let mut tris: Vec<[[f64; 3]; 3]> = Vec::new();
    for j in 0..n {
        // Bottom cap (normal -z), wall, top cap (normal +z).
        tris.push([[0.0, 0.0, 0.0], p(j + 1, r, 0.0), p(j, r, 0.0)]);
        tris.push([p(j, r, 0.0), p(j + 1, r, 0.0), p(j + 1, r, h)]);
        tris.push([p(j, r, 0.0), p(j + 1, r, h), p(j, r, h)]);
        tris.push([[0.0, 0.0, h], p(j, r, h), p(j + 1, r, h)]);
    }
    let mut s = String::from("solid bar\n");
    for t in tris {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for v in t {
            s.push_str(&format!("   vertex {} {} {}\n", v[0], v[1], v[2]));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid bar\n");
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
}

/// The issue's document up to the seeded Facing `F1`: lathe, bar r 10
/// x `part_h`, cylinder stock 20 x `stock_h`, the house insert.
fn facing_job(dir: &Path, part_h: f64, stock_h: f64) -> Client {
    let stl = dir.join("cyl.stl");
    write_bar_stl(&stl, 10.0, part_h);
    let mut c = Client::new();
    c.ok("load_machine", json!({ "path": repo("assets/machines/lathe.mch") }));
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "cylinder": { "diameter": 20.0, "height": stock_h } }));
    c.ok("add_tool", json!({ "catalog": "TN-1600-0R8" }));
    c.ok("add_operation", json!({ "kind": "facing", "tool": "TN-1600-0R8", "name": "F1", "source": "cyl" }));
    c
}

#[test]
fn a_seeded_facing_faces_the_allowance_not_the_part() {
    let dir = tmp_dir("allowance");
    let mut c = facing_job(&dir, 48.0, 50.0);
    let generated = c.ok("generate", json!({ "name": "F1" }));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");

    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["findings"], json!([]), "no gouge, no rapid in stock: {ver}");
    assert_eq!(ver["ok"], json!(true), "{ver}");

    let nc = dir.join("f1.nc");
    c.ok("post", json!({ "post": repo("assets/posts/fanuc-lathe.cps"), "output": nc.display().to_string() }));
    let text = std::fs::read_to_string(&nc).expect("posted");
    // The passes cross the axis (X negative); every Z the program cuts
    // at stands on the 48..50 ladder, never inside the part.
    let z_of = |l: &str| {
        l.split_whitespace().find_map(|w| w.strip_prefix('Z').and_then(|v| v.parse::<f64>().ok()))
    };
    // A pass is the feed that crosses the axis: `G01 X-1.6 Z..`.
    let passes: Vec<f64> = text
        .lines()
        .filter(|l| l.starts_with("G01") && l.contains(" X-"))
        .map(|l| z_of(l).expect("a pass names its Z"))
        .collect();
    assert_eq!(passes, vec![50.0, 49.5, 49.0, 48.5, 48.0], "five passes Z50 down to Z48:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_seeded_facing_on_a_part_at_length_reports_nothing_to_face() {
    let dir = tmp_dir("at-length");
    let mut c = facing_job(&dir, 48.0, 48.0);
    let (err, body) = c.call("generate", json!({ "name": "F1" }));
    let said = body.to_string();
    assert!(said.contains("nothing to face"), "the at-length part says so (err={err}): {said}");
    assert!(err, "no path stands for an at-length part: {said}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_facing_seed_runs_from_the_stock_top_to_the_part_top() {
    // seed_params is the MCP add_operation's seed; the GUI quick-add
    // retypes the same Face draft (pinned in crates/ui operations.rs).
    let OpParams::Facing { top_z, bottom_z, .. } = seed_params(OpType::Facing.ordinal()) else {
        panic!("the Facing row seeds a Facing");
    };
    assert_eq!((top_z.reference, top_z.offset), (HeightReference::TopOfStock, 0.0));
    assert_eq!((bottom_z.reference, bottom_z.offset), (HeightReference::TopOfModel, 0.0));
}

/// Edit `F1`'s window to Fixed `top` / `bottom` over the MCP
/// `edit_operation` door (the same `EditOperationParams` +
/// `heights_gate` the GUI property panel runs).
fn edit_window(c: &mut Client, top: f64, bottom: f64) -> (bool, String) {
    let (err, body) = c.call(
        "edit_operation",
        json!({ "name": "F1", "params": {
            "top_z": { "reference": "fixed", "offset": top },
            "bottom_z": { "reference": "fixed", "offset": bottom },
        } }),
    );
    (err, body.to_string())
}

#[test]
fn an_inverted_facing_window_is_refused_at_the_door() {
    // #1409 relaxed the Facing arm of `depth_sane` to `t >= b` so an
    // at-length part reaches generate's "nothing to face"; a top
    // below the facing plane must still refuse by name, before any
    // geometry runs.
    let dir = tmp_dir("inverted");
    let mut c = facing_job(&dir, 48.0, 50.0);
    let (err, said) = edit_window(&mut c, 47.0, 48.0);
    assert!(err, "a top below the facing plane is no window: {said}");
    assert!(said.contains("must cut downward"), "{said}");

    // The create door says the same: stock 46 under a 48 part seeds
    // top 46 (Top of Stock) over a plane at 48 (Top of Model).
    let short = tmp_dir("inverted-create");
    let stl = short.join("cyl.stl");
    write_bar_stl(&stl, 10.0, 48.0);
    let mut s = Client::new();
    s.ok("load_machine", json!({ "path": repo("assets/machines/lathe.mch") }));
    s.ok("import_model", json!({ "path": stl.display().to_string() }));
    s.ok("set_stock", json!({ "cylinder": { "diameter": 20.0, "height": 46.0 } }));
    s.ok("add_tool", json!({ "catalog": "TN-1600-0R8" }));
    let (err, body) = s.call(
        "add_operation",
        json!({ "kind": "facing", "tool": "TN-1600-0R8", "name": "F1", "source": "cyl" }),
    );
    let said = body.to_string();
    assert!(err, "stock shorter than the part has no window: {said}");
    assert!(said.contains("must cut downward"), "{said}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&short);
}

#[test]
fn a_level_facing_window_passes_the_door_and_the_slightest_inversion_does_not() {
    // The Facing arm of `depth_sane` (operation/heights_sel.rs) is an
    // exact `t >= b` - it carries NO tolerance. So:
    // - 48 / 48 (level, the at-length part) passes the door; generate
    //   then refuses it as "nothing to face" (pinned above);
    // - 48 / 48 + 1e-9 is a top below the plane by a nanometre and
    //   already refuses. 1e-9 is far below any machine resolution yet
    //   exactly representable as a difference near 48 (f64 spacing
    //   there is ~7e-15), so a guard that grew any epsilon above 1e-9
    //   (or went back to "always true") turns this red.
    let dir = tmp_dir("level");
    let mut c = facing_job(&dir, 48.0, 50.0);
    let (err, said) = edit_window(&mut c, 48.0, 48.0);
    assert!(!err, "a level Facing window is legal at the door: {said}");

    let (err, said) = edit_window(&mut c, 48.0, 48.0 + 1e-9);
    assert!(err, "a top 1e-9 below the facing plane is inverted: {said}");
    assert!(said.contains("must cut downward"), "{said}");
    let _ = std::fs::remove_dir_all(&dir);
}
