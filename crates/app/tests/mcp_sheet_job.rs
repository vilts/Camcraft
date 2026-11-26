//! The guided sheet job (#3070), driven in-process over the MCP server's
//! JSON-RPC handler: one `set_sheet_job` sets the stock, the Z zero and
//! the XY zero, and one undo puts them back; the post holds the program
//! to the stated through-cut allowance and clamp plane by name; the
//! posted Z words follow the stated Z zero; the record survives a save
//! and reload; bad answers are refused with the command's own words.
//!
//! The fixture: a 40 x 40 x 10 box part in 60 x 60 x 10 stock (the sheet),
//! contoured with `EM-0400-2FL` from the stock top to 0.5 mm below the
//! stock bottom - a through cut 0.5 mm into the spoilboard.
//!
//! Gate map (M4.4): covers app::sheet_job, app::commands::sheet_job,
//! app::mcp::sheet_job_tools.

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const SHEET_MM: f64 = 60.0;
const THICK_MM: f64 = 10.0;
const INTO_SPOILBOARD_MM: f64 = 0.5;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-sheet-job-{tag}-{}", std::process::id()));
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

    fn setup(&mut self) -> Value {
        self.ok("get_job", json!({}))["setups"][0].clone()
    }

    /// Generate what is stale (nothing, after a job that left the stock
    /// the same size) and check each generated op is ok.
    fn generate(&mut self) {
        let generated = self.ok("generate", json!({}));
        let ops = generated["generated"].as_array().cloned().unwrap_or_default();
        assert!(ops.iter().all(|o| o["status"] == json!("ok")), "{generated}");
    }
}

/// The part, the sheet stock, one declared setup and the through-cut
/// contour `C1` (its rapids across at `clearance` mm over the stock top).
fn sheet_doc(dir: &Path, clearance: f64) -> Client {
    let part = dir.join("part.stl");
    write_box_stl(&part, (20.0, 20.0, THICK_MM));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": SHEET_MM, "dy": SHEET_MM, "dz": THICK_MM } }));
    c.ok("add_setup", json!({}));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_operation", json!({
        "kind": "contour", "tool": "EM-0400-2FL", "name": "C1", "source": "part",
        "params": {
            "clearance_z": { "reference": "top_of_stock", "offset": clearance },
            "retract_z": { "reference": "top_of_stock", "offset": 2.0 },
            "top_z": { "reference": "top_of_stock", "offset": 0.0 },
            "bottom_z": { "reference": "bottom_of_stock", "offset": -INTO_SPOILBOARD_MM },
        },
    }));
    c
}

fn job(z_zero: &str, allowance: f64, clamp: f64) -> Value {
    json!({
        "length": SHEET_MM, "width": SHEET_MM, "thickness": THICK_MM,
        "z_zero": z_zero, "allowance": allowance, "clamp_clearance": clamp,
    })
}

/// The lowest Z word the program posts.
fn lowest_z(nc: &str) -> f64 {
    nc.split_whitespace()
        .filter_map(|w| w.strip_prefix('Z').and_then(|v| v.parse::<f64>().ok()))
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn one_set_sheet_job_places_stock_and_zero_and_one_undo_puts_them_back() {
    let dir = tmp_dir("undo");
    let mut c = sheet_doc(&dir, 5.0);
    let before = c.setup();
    assert!(before["sheet_job"].is_null(), "{before}");

    // A larger sheet, Z zero on the top, XY zero front left (the default).
    let set = c.ok("set_sheet_job", json!({
        "length": 100.0, "width": 80.0, "thickness": 12.0, "z_zero": "top", "allowance": 0.3,
    }));
    assert_eq!(set["sheet_job"]["allowance"], json!(0.3), "the allowance is always stated: {set}");
    assert_eq!(set["sheet_job"]["xy_zero"], json!("front_left"), "{set}");
    assert_eq!(set["sheet_job"]["clamp_clearance"], json!(0.0), "{set}");
    // The stock kept its XY centre (0, 0) and its bottom (Z 0).
    assert_eq!(set["wcs_origin"], json!([-50.0, -40.0, 12.0]), "front left, on the sheet top: {set}");
    let s = c.setup();
    assert_eq!(s["stock_bounds"]["min"], json!([-50.0, -40.0, 0.0]), "{s}");
    assert_eq!(s["stock_bounds"]["max"], json!([50.0, 40.0, 12.0]), "{s}");
    assert_eq!(s["sheet_job"]["thickness"], json!(12.0), "get_job shows the job: {s}");

    // The spoilboard reading moves only Z zero; a corner word moves XY.
    let spoil = c.ok("set_sheet_job", json!({
        "length": 100.0, "width": 80.0, "thickness": 12.0, "z_zero": "spoilboard", "xy_zero": "back_right", "allowance": 0.3,
    }));
    assert_eq!(spoil["wcs_origin"], json!([50.0, 40.0, 0.0]), "{spoil}");

    // One undo per checklist run: back to the top reading, then to before.
    c.ok("undo", json!({}));
    assert_eq!(c.setup()["sheet_job"]["z_zero"], json!("top"));
    c.ok("undo", json!({}));
    let after = c.setup();
    assert!(after["sheet_job"].is_null(), "{after}");
    assert_eq!(after["stock_bounds"], before["stock_bounds"], "one undo puts the stock back: {after}");
    assert_eq!(after["wcs"], before["wcs"], "one undo puts the WCS back: {after}");
    c.ok("redo", json!({}));
    assert_eq!(c.setup()["sheet_job"]["allowance"], json!(0.3), "redo replays it");

    // Clearing drops only the record.
    let cleared = c.ok("set_sheet_job", json!({ "clear": true }));
    assert!(cleared["sheet_job"].is_null(), "{cleared}");
    assert_eq!(c.setup()["stock_bounds"]["max"], json!([50.0, 40.0, 12.0]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_through_cut_within_the_allowance_posts_from_the_stated_zero_and_past_it_is_refused() {
    let dir = tmp_dir("allowance");
    let mut c = sheet_doc(&dir, 5.0);

    // Z zero on the top: the through cut posts at -(thickness + 0.5).
    c.ok("set_sheet_job", job("top", 1.0, 0.0));
    c.generate();
    let nc = dir.join("top.nc");
    c.ok("post", json!({ "post": "Fanuc", "output": nc.display().to_string() }));
    let text = std::fs::read_to_string(&nc).expect("posted");
    assert!((lowest_z(&text) - -(THICK_MM + INTO_SPOILBOARD_MM)).abs() < 1e-6, "lowest Z {}: {text}", lowest_z(&text));

    // Z zero on the spoilboard: the same cut posts at -0.5.
    c.ok("set_sheet_job", job("spoilboard", 1.0, 0.0));
    c.generate();
    let nc = dir.join("spoil.nc");
    c.ok("post", json!({ "post": "Fanuc", "output": nc.display().to_string() }));
    let text = std::fs::read_to_string(&nc).expect("posted");
    assert!((lowest_z(&text) - -INTO_SPOILBOARD_MM).abs() < 1e-6, "lowest Z {}: {text}", lowest_z(&text));

    // An allowance smaller than the cut: refused by name, nothing written.
    c.ok("set_sheet_job", job("spoilboard", 0.2, 0.0));
    c.generate();
    let held = dir.join("held.nc");
    let why = c.refused("post", json!({ "post": "Fanuc", "output": held.display().to_string() }));
    assert!(why.contains("C1") && why.contains("through-cut allowance of 0.200 mm"), "{why}");
    assert!(why.contains("Z-0.500"), "the depth prints in program Z: {why}");
    assert!(!held.exists(), "a refused post writes no file");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_rapid_across_below_the_clamp_plane_is_refused_by_name() {
    let dir = tmp_dir("clamp");
    // Rapids 5 mm over the sheet; clamps stand 8 mm tall.
    let mut c = sheet_doc(&dir, 5.0);
    c.ok("set_sheet_job", job("top", 1.0, 8.0));
    c.generate();
    let why = c.refused("post", json!({ "post": "Fanuc", "output": dir.join("held.nc").display().to_string() }));
    assert!(why.contains("C1") && why.contains("below setup 'Setup1''s clamp plane Z8.000"), "{why}");

    // Rapids at 10 mm clear them.
    let mut c = sheet_doc(&dir, 10.0);
    c.ok("set_sheet_job", job("top", 1.0, 8.0));
    c.generate();
    c.ok("post", json!({ "post": "Fanuc", "output": dir.join("clear.nc").display().to_string() }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_stock_or_zero_edited_after_the_checklist_holds_the_post() {
    let dir = tmp_dir("drift");
    let mut c = sheet_doc(&dir, 5.0);
    c.ok("set_sheet_job", job("top", 1.0, 0.0));
    c.ok("set_wcs", json!({ "origin": [0.0, 0.0, 0.0] }));
    c.generate();
    let why = c.refused("post", json!({ "post": "Fanuc", "output": dir.join("held.nc").display().to_string() }));
    assert!(why.contains("Z zero is no longer at the sheet top"), "{why}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_sheet_job_survives_a_save_and_reload() {
    let dir = tmp_dir("save");
    let mut c = sheet_doc(&dir, 5.0);
    c.ok("set_sheet_job", job("spoilboard", 0.4, 6.0));
    let proj = dir.join("job.camproj");
    c.ok("save_project", json!({ "path": proj.display().to_string() }));
    let mut d = Client::new();
    d.ok("open_project", json!({ "path": proj.display().to_string() }));
    let s = d.setup();
    assert_eq!(s["sheet_job"], json!({
        "length": SHEET_MM, "width": SHEET_MM, "thickness": THICK_MM, "z_zero": "spoilboard",
        "xy_zero": "front_left", "allowance": 0.4, "clamp_clearance": 6.0,
    }), "{s}");

    // A file whose allowance no command would accept is refused, never guessed.
    let text = std::fs::read_to_string(&proj).expect("saved");
    assert_eq!(text.matches("\"allowance\": 0.4").count(), 1, "{text}");
    let bad = dir.join("bad.camproj");
    std::fs::write(&bad, text.replace("\"allowance\": 0.4", "\"allowance\": 50.0")).expect("write");
    let why = d.refused("open_project", json!({ "path": bad.display().to_string() }));
    assert!(why.contains("through-cut allowance of 50 mm"), "{why}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_answers_are_refused_with_the_commands_words() {
    let mut c = Client::new();
    let why = c.refused("set_sheet_job", job("top", 0.3, 0.0));
    assert!(why.contains("add a setup first"), "{why}");
    c.ok("add_setup", json!({}));
    let why = c.refused("set_sheet_job", json!({ "length": 100.0, "width": 80.0, "thickness": 12.0, "z_zero": "top" }));
    assert!(why.contains("\"allowance\" is required"), "the allowance is never assumed: {why}");
    let why = c.refused("set_sheet_job", job("top", 6.0, 0.0));
    assert!(why.contains("at most 5 mm"), "{why}");
    let why = c.refused("set_sheet_job", job("middle", 0.3, 0.0));
    assert!(why.contains("\"z_zero\" 'middle'"), "{why}");
    let why = c.refused("set_sheet_job", json!({ "clear": true }));
    assert!(why.contains("states no sheet job"), "{why}");
    let mut bad = job("top", 0.3, 0.0);
    bad["thickness"] = json!(0.0);
    let why = c.refused("set_sheet_job", bad);
    assert!(why.contains("thickness must be a positive size"), "{why}");
}
