//! The MCP setup and fixture tools (#181), driven in-process over the
//! server's JSON-RPC handler: a work offset set off the stock corner, a
//! machine mounted, a fixture marked and placed, and the verifier's
//! fixture findings read back as data - every step undoable, every bad
//! input a tool error.
//!
//! Gate map (M4.4): covers app::mcp::setup_tools (set_program, #258), app::mcp::fixture_tools,
//! app::mcp::align_tools (#557, #555), app::commands::stock_source_only
//! (#1094),
//! app::posting (fixture_findings).

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-setup-{tag}-{}", std::process::id()));
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

#[test]
fn g54_off_the_stock_corner_a_machine_and_fixture_findings_as_data() {
    let dir = tmp_dir("job");
    let part = dir.join("block.stl");
    let vise = dir.join("vise.stl");
    write_box_stl(&part, (20.0, 20.0, 15.0));
    write_box_stl(&vise, (20.0, 20.0, 15.0));
    let mut c = Client::new();

    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    for want in ["add_setup", "activate_setup", "set_wcs", "set_work_offset", "set_program", "set_setup_post", "load_machine", "add_fixture", "remove_fixture", "move_fixture", "get_fixture_findings"] {
        assert!(names.iter().any(|n| n == want), "missing tool {want}: {names:?}");
    }

    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));

    // G54 on the stock's top front-left corner.
    let wcs = c.ok("set_wcs", json!({ "stock_point": { "x": "min", "y": "min", "z": "max" } }));
    assert_eq!(wcs["origin"], json!([-30.0, -30.0, 20.0]), "{wcs}");
    assert_eq!(wcs["word"], json!("G54"));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["setups"][0]["wcs"]["placed"], json!(true), "{job}");
    assert_eq!(job["setups"][0]["wcs"]["origin"], json!([-30.0, -30.0, 20.0]));
    assert_eq!(job["setups"][0]["wcs"]["word"], json!("G54"));
    // One History step: undo unplaces it, redo puts it back.
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["setups"][0]["wcs"]["placed"], json!(false));
    c.ok("redo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["setups"][0]["wcs"]["origin"], json!([-30.0, -30.0, 20.0]));
    // An offset rides the stock point; an absolute origin needs none.
    let moved = c.ok("set_wcs", json!({ "stock_point": { "x": "center", "y": "center", "z": "max" }, "offset": [0.0, 0.0, -1.0] }));
    assert_eq!(moved["origin"], json!([0.0, 0.0, 19.0]), "{moved}");
    c.ok("undo", json!({}));

    let m = c.ok("load_machine", json!({ "builtin": "VMC 3-axis" }));
    assert!(m["machine"].as_str().is_some_and(|s| !s.is_empty()), "{m}");
    assert!(m["table"].is_object(), "the VMC states its travels, so it has a table: {m}");

    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" }));
    c.ok("generate", json!({}));

    let clear = c.ok("get_fixture_findings", json!({}));
    assert_eq!(clear["judged"], json!(true), "{clear}");
    assert_eq!(clear["findings"], json!([]), "no fixture yet: {clear}");
    assert!(clear["machine"].is_string());

    // A vise sitting across the facing pass: marked, placed, judged live
    // (fixtures never stale a path - the walk re-runs on the cache).
    c.ok("import_model", json!({ "path": vise.display().to_string() }));
    let tagged = c.ok("add_fixture", json!({ "model": "vise" }));
    assert_eq!(tagged["fixture"], json!("vise"));
    let placed = c.ok("move_fixture", json!({ "name": "vise", "position": [0.0, 0.0, 10.0] }));
    assert_eq!(placed["origin"], json!([0.0, 0.0, 10.0]), "{placed}");

    let hit = c.ok("get_fixture_findings", json!({}));
    assert_eq!(hit["judged"], json!(true), "{hit}");
    assert_eq!(hit["ok"], json!(false), "{hit}");
    assert_eq!(hit["workholding"][0]["fixtures"][0]["name"], json!("vise"), "{hit}");
    assert_eq!(hit["workholding"][0]["fixtures"][0]["bounds"]["max"][2], json!(25.0), "{hit}");
    let findings = hit["findings"].as_array().cloned().unwrap_or_default();
    assert!(!findings.is_empty(), "{hit}");
    for f in &findings {
        assert_eq!(f["kind"], json!("fixture_hit"), "{f}");
        assert_eq!(f["op"], json!("Face1"), "{f}");
        assert_eq!(f["key"], json!("Face1"), "{f}");
        assert_eq!(f["blocks_post"], json!(true), "{f}");
        assert!(f["move"].as_u64().is_some(), "{f}");
        assert!(f["part"].as_str().is_some_and(|p| ["shank", "holder", "spindle", "rapid", "flute"].contains(&p)), "{f}");
        assert!(f["message"].as_str().is_some_and(|m| m.contains("strikes a fixture")), "{f}");
    }
    // The post gate reads the same walk.
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["gate"]["ok"], json!(false), "{ver}");

    // Undo the move: the vise goes back to the origin, z 0..15, below the pass.
    c.ok("undo", json!({}));
    let back = c.ok("get_fixture_findings", json!({}));
    assert_eq!(back["workholding"][0]["fixtures"][0]["bounds"]["max"][2], json!(15.0), "{back}");
    // Unmarked, it is a plain model again: nothing to strike.
    c.ok("move_fixture", json!({ "name": "vise", "position": [0.0, 0.0, 10.0] }));
    c.ok("remove_fixture", json!({ "name": "vise" }));
    let gone = c.ok("get_fixture_findings", json!({}));
    assert_eq!(gone["findings"], json!([]), "{gone}");
    assert_eq!(gone["workholding"][0]["fixtures"], json!([]), "{gone}");
    let job = c.ok("get_job", json!({}));
    assert!(job["models"].as_array().is_some_and(|m| m.iter().any(|m| m["name"] == json!("vise") && m["fixture"] == json!(false))), "{job}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn setups_declare_activate_and_post_their_own_word() {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 50.0, "dy": 40.0, "dz": 10.0 } }));
    c.ok("set_wcs", json!({ "origin": [1.0, 2.0, 3.0] }));
    // No setup declared: the word is not ours to pick yet.
    let e = c.refused("set_work_offset", json!({ "word": "G55" }));
    assert!(e.contains("add_setup"), "{e}");

    let first = c.ok("add_setup", json!({}));
    assert_eq!(first["setup"], json!("Setup1"));
    let second = c.ok("add_setup", json!({ "datum_from": "Setup1" }));
    assert_eq!(second["active"], json!("Setup2"));
    let word = c.ok("set_work_offset", json!({ "word": "g55" }));
    assert_eq!(word["word"], json!("G55"), "{word}");
    assert_eq!(word["setup"], json!("Setup2"));

    let job = c.ok("get_job", json!({}));
    let setups = job["setups"].as_array().cloned().unwrap_or_default();
    assert_eq!(setups.len(), 2, "{job}");
    assert_eq!(setups[1]["name"], json!("Setup2"));
    assert_eq!(setups[1]["wcs"]["word"], json!("G55"));
    assert_eq!(setups[1]["wcs"]["origin"], json!([1.0, 2.0, 3.0]), "the datum transfer copies the WCS: {job}");
    assert_eq!(setups[0]["wcs"]["word"], json!("G54"));

    // The WCS edit lands on the active setup only.
    c.ok("set_wcs", json!({ "origin": [5.0, 5.0, 5.0] }));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["setups"][1]["wcs"]["origin"], json!([5.0, 5.0, 5.0]));
    assert_eq!(job["setups"][0]["wcs"]["origin"], json!([1.0, 2.0, 3.0]));

    let act = c.ok("activate_setup", json!({ "name": "Setup1" }));
    assert_eq!(act["active"], json!("Setup1"));
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["active_setup"], json!("Setup2"));

    // A setup's own machine, then back to following the document's.
    let own = c.ok("load_machine", json!({ "builtin": "VMC 3-axis", "setup": "Setup2" }));
    assert_eq!(own["setup"], json!("Setup2"));
    assert!(own["machine"].is_string(), "{own}");
    let follow = c.ok("load_machine", json!({ "clear": true, "setup": "Setup2" }));
    assert_eq!(follow["machine"], Value::Null, "the document mounts nothing: {follow}");
}

#[test]
fn bad_setup_input_is_a_tool_error() {
    let mut c = Client::new();
    let e = c.refused("set_wcs", json!({ "stock_point": { "x": "min", "y": "min", "z": "max" } }));
    assert!(e.contains("no stock"), "{e}");
    c.ok("set_stock", json!({ "box": { "dx": 10.0, "dy": 10.0, "dz": 10.0 } }));
    let e = c.refused("set_wcs", json!({ "origin": [0.0, 0.0, 0.0], "stock_point": { "x": "min", "y": "min", "z": "min" } }));
    assert!(e.contains("exactly one"), "{e}");
    let e = c.refused("set_wcs", json!({}));
    assert!(e.contains("exactly one"), "{e}");
    let e = c.refused("set_wcs", json!({ "stock_point": { "x": "left", "y": "min", "z": "min" } }));
    assert!(e.contains("stock_point.x"), "{e}");
    let e = c.refused("set_wcs", json!({ "origin": [0.0, 0.0] }));
    assert!(e.contains("3 finite numbers"), "{e}");
    let e = c.refused("set_wcs", json!({ "origin": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 0.0] }));
    assert!(e.contains("degenerate"), "{e}");
    let e = c.refused("set_wcs", json!({ "origin": [0.0, 0.0, 0.0], "offset": [1.0, 0.0, 0.0] }));
    assert!(e.contains("offset"), "{e}");

    let e = c.refused("load_machine", json!({ "builtin": "Imaginary 9-axis" }));
    assert!(e.contains("VMC 3-axis"), "the refusal lists the names: {e}");
    let e = c.refused("load_machine", json!({}));
    assert!(e.contains("exactly one"), "{e}");
    let e = c.refused("load_machine", json!({ "clear": true }));
    assert!(e.contains("no machine"), "{e}");
    let e = c.refused("load_machine", json!({ "path": "/nonexistent/machine.mch" }));
    assert!(e.contains("machine file"), "{e}");
    let e = c.refused("load_machine", json!({ "builtin": "VMC 3-axis", "setup": "Nope" }));
    assert!(e.contains("no setup named"), "{e}");

    let e = c.refused("activate_setup", json!({ "name": "Nope" }));
    assert!(e.contains("Nope"), "{e}");
    c.ok("add_setup", json!({}));
    let e = c.refused("set_work_offset", json!({ "word": "G60" }));
    assert!(e.contains("G54"), "{e}");
    let e = c.refused("add_setup", json!({ "name": "Setup1" }));
    assert!(e.contains("Setup1"), "{e}");

    let e = c.refused("add_fixture", json!({ "model": "ghost" }));
    assert!(e.contains("ghost"), "{e}");
    let e = c.refused("remove_fixture", json!({ "name": "ghost" }));
    assert!(e.contains("ghost"), "{e}");
    let e = c.refused("move_fixture", json!({ "name": "ghost", "position": [0.0, 0.0, 0.0] }));
    assert!(e.contains("ghost"), "{e}");
    let e = c.refused("move_fixture", json!({ "name": "ghost" }));
    assert!(e.contains("position"), "{e}");
    let e = c.refused("set_wcs", json!({ "origin": [0.0, 0.0, 0.0], "typo": 1 }));
    assert!(e.contains("unknown argument"), "{e}");
}

/// #258: the program number and comment, one undo step; an omitted key
/// keeps its value, null clears it, a bad value is the command's refusal.
#[test]
fn set_program_states_undoes_and_refuses_by_name() {
    let mut c = Client::new();
    let p = c.ok("set_program", json!({ "number": 1234, "comment": "Bracket A" }));
    assert_eq!(p, json!({ "number": 1234, "comment": "Bracket A" }));
    let p = c.ok("set_program", json!({ "number": null }));
    assert_eq!(p, json!({ "number": null, "comment": "Bracket A" }), "the comment is kept");
    c.ok("undo", json!({}));
    let e = c.refused("set_program", json!({ "number": 0 }));
    assert!(e.contains("outside 1 to 9999"), "{e}");
    let e = c.refused("set_program", json!({ "comment": "A (B)" }));
    assert!(e.contains("'('"), "{e}");
    let e = c.refused("set_program", json!({ "number": "12" }));
    assert!(e.contains("whole number"), "{e}");
    let e = c.refused("set_program", json!({ "number": 1234 }));
    assert!(e.contains("already set"), "undo restored 1234: {e}");
}

/// #561: `locate_part` mates a part face onto a fixture face by
/// `list_features` ids (a fixture's features list too), one undo step;
/// an edge is refused by name.
#[test]
fn locate_part_seats_the_part_on_the_fixture_by_feature_ids() {
    let dir = tmp_dir("locate");
    let part = dir.join("block.stl");
    let vise = dir.join("vise.stl");
    write_box_stl(&part, (10.0, 5.0, 8.0));
    write_box_stl(&vise, (40.0, 30.0, 15.0));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("import_model", json!({ "path": vise.display().to_string() }));
    c.ok("add_fixture", json!({ "model": "vise" }));
    c.ok("move_fixture", json!({ "name": "vise", "position": [100.0, 0.0, -20.0] }));
    // The flat face lying at world Z `z`.
    let face = |c: &mut Client, model: &str, z: f64| -> String {
        let f = c.ok("list_features", json!({ "model": model, "kinds": ["faces"] }));
        f["faces"]
            .as_array()
            .and_then(|fs| {
                fs.iter().find(|fa| fa["bounds"]["min"][2] == json!(z) && fa["bounds"]["max"][2] == json!(z))
            })
            .and_then(|fa| fa["id"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| panic!("{model} has a face at Z {z}: {f}"))
    };
    let bottom = face(&mut c, "block", 0.0);
    let top = face(&mut c, "vise", -5.0);
    let placed = c.ok(
        "locate_part",
        json!({ "part": "block", "fixture": "vise", "mates": [{ "part": bottom, "fixture": top }], "offset": [0.0, 0.0, 2.0] }),
    );
    assert_eq!(placed["moved"], json!("block"), "{placed}");
    assert_eq!(placed["origin"][2], json!(-3.0), "the bottom 2 mm above the vise top at Z -5: {placed}");
    assert_eq!(placed["warning"], Value::Null, "{placed}");
    c.ok("undo", json!({}));
    let job = c.ok("get_job", json!({}));
    assert!(job.to_string().contains("block"), "{job}");

    let chain = c.ok("list_features", json!({ "model": "block", "kinds": ["chains"] }))["chains"][0]["id"]
        .as_str()
        .map(str::to_owned)
        .expect("a chain");
    let e = c.refused("locate_part", json!({ "part": "block", "fixture": "vise", "mates": [{ "part": chain, "fixture": top }] }));
    assert!(e.contains("is an edge"), "{e}");
    let e = c.refused("locate_part", json!({ "part": "block", "fixture": "vise", "mates": [{ "part": bottom, "fixture": top }], "move": "both" }));
    assert!(e.contains("\"move\""), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #557: `set_part_axes` turns the part in the setup's work plane from an
/// edge chain or two vertices, one undo step, refusals by name.
#[test]
fn set_part_axes_turns_the_part_from_an_edge_or_two_points() {
    let dir = tmp_dir("axes");
    let part = dir.join("block.stl");
    write_box_stl(&part, (20.0, 20.0, 15.0));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    let refused = c.refused("set_part_axes", json!({ "y": [] }));
    assert!(refused.contains("add_setup first") || refused.contains("at least one pick"), "{refused}");
    c.ok("add_setup", json!({}));
    let f = c.ok("list_features", json!({ "model": "block", "kinds": ["chains", "vertices"] }));
    let chain = |f: &Value, want: &dyn Fn(&Value, &Value) -> bool| -> String {
        f["chains"]
            .as_array()
            .and_then(|cs| cs.iter().find(|c| want(&c["start"], &c["end"])))
            .and_then(|c| c["id"].as_str())
            .expect("a chain")
            .to_owned()
    };
    let along_x = chain(&f, &|a, b| a[1] == b[1] && a[2] == b[2] && a[0] != b[0]);
    let upright = chain(&f, &|a, b| a[0] == b[0] && a[1] == b[1]);
    let vertex = |at: [f64; 3]| -> String {
        f["vertices"]
            .as_array()
            .and_then(|vs| vs.iter().find(|v| v["at"] == json!(at)))
            .and_then(|v| v["id"].as_str())
            .expect("a vertex")
            .to_owned()
    };
    let pick = |id: &str| json!({ "model": "block", "id": id });

    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    assert!(names.iter().any(|n| n == "set_part_axes"), "{names:?}");

    let turned = c.ok("set_part_axes", json!({ "y": [pick(&along_x)] }));
    assert_eq!(turned["turn_deg"].as_f64().map(f64::abs), Some(90.0), "{turned}");
    assert!(turned["alignment"].is_object(), "{turned}");
    let again = c.refused("set_part_axes", json!({ "y": [pick(&along_x)] }));
    assert!(again.contains("already holds the part that way"), "{again}");
    c.ok("undo", json!({}));
    let pts = [pick(&vertex([-20.0, -20.0, 0.0])), pick(&vertex([20.0, 20.0, 0.0]))];
    let diag = c.ok("set_part_axes", json!({ "x": pts, "primary": "x" }));
    assert!((diag["turn_deg"].as_f64().unwrap_or(0.0) + 45.0).abs() < 1e-9, "{diag}");
    c.ok("undo", json!({}));
    let msg = c.refused("set_part_axes", json!({ "y": [pick(&upright)] }));
    assert!(msg.contains("along the work plane's normal"), "{msg}");
    let msg = c.refused("set_part_axes", json!({ "y": [pick(&along_x)], "primary": "z" }));
    assert!(msg.contains("\"primary\""), "{msg}");
}

/// #555: `lay_part_on_points` lays the part on a plane through three
/// vertices - flip rests them on the table - one undo step, refusals by
/// name.
#[test]
fn lay_part_on_points_lays_the_plane_through_three_vertices() {
    let dir = tmp_dir("points");
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
    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    assert!(names.iter().any(|n| n == "lay_part_on_points"), "{names:?}");

    // The front wall (y = -20): its outside is -Y; flip lays it on the table.
    let wall = [pick([-20.0, -20.0, 0.0]), pick([20.0, -20.0, 0.0]), pick([20.0, -20.0, 15.0])];
    let laid = c.ok("lay_part_on_points", json!({ "points": wall, "flip": true }));
    assert!(laid["alignment"].is_object(), "{laid}");
    assert!(laid["outside"].as_str().is_some_and(|s| s.contains("-Y")), "{laid}");
    let again = c.refused("lay_part_on_points", json!({ "points": wall, "flip": true }));
    assert!(again.contains("already holds the part that way"), "{again}");
    c.ok("undo", json!({}));
    let framed = c.ok("lay_part_on_points", json!({ "points": wall, "flip": true, "frame": true, "plane": "g17" }));
    assert!(framed["alignment"].is_object(), "{framed}");
    c.ok("undo", json!({}));
    let same = [pick([-20.0, -20.0, 0.0]), pick([-20.0, -20.0, 0.0]), pick([20.0, -20.0, 15.0])];
    let msg = c.refused("lay_part_on_points", json!({ "points": same }));
    assert!(msg.contains("coincide"), "{msg}");
    let msg = c.refused("lay_part_on_points", json!({ "points": [pick([-20.0, -20.0, 0.0])] }));
    assert!(msg.contains("pick three points - 1 picked"), "{msg}");
    let msg = c.refused("lay_part_on_points", json!({ "points": wall, "plane": "g20" }));
    assert!(msg.contains("\"plane\""), "{msg}");
}

/// #1094: the stock page's "raw stock only" checkbox over MCP - the same
/// command, one undo step, refused on a stock not made from a model.
#[test]
fn set_raw_stock_only_marks_the_stock_source_through_the_command() {
    let dir = tmp_dir("raw");
    let casting = dir.join("casting.stl");
    write_box_stl(&casting, (30.0, 30.0, 20.0));
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 10.0, "dy": 10.0, "dz": 10.0 } }));
    let e = c.refused("set_raw_stock_only", json!({ "on": true }));
    assert!(e.contains("not made from a model"), "{e}");
    c.ok("import_model", json!({ "path": casting.display().to_string() }));
    c.ok("set_stock", json!({ "from_model": "casting" }));
    let e = c.refused("set_raw_stock_only", json!({}));
    assert!(e.contains("\"on\""), "{e}");
    let on = c.ok("set_raw_stock_only", json!({ "on": true }));
    assert_eq!(on["raw_stock_only"], json!(true), "{on}");
    // No operation and no other model: the mark holds; nothing else to judge.
    assert_eq!(on["judged_out"], json!("casting"), "{on}");
    c.ok("undo", json!({}));
    let off = c.ok("set_raw_stock_only", json!({ "on": false }));
    assert_eq!(off["judged_out"], Value::Null, "undo walked the mark back: {off}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #1233: a setup's own post through the undoable command: set, read
/// back in get_job, undo/redo, clear, and the refusals by name.
#[test]
fn set_setup_post_sets_clears_undoes_and_refuses() {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 50.0, "dy": 40.0, "dz": 10.0 } }));
    let e = c.refused("set_setup_post", json!({ "post": "Haas" }));
    assert!(e.contains("add_setup"), "{e}");
    c.ok("add_setup", json!({}));
    c.ok("add_setup", json!({}));
    let own = |c: &mut Client, i: usize| c.ok("get_job", json!({}))["setups"][i]["post"].clone();
    assert_eq!(own(&mut c, 0), Value::Null);

    let r = c.ok("set_setup_post", json!({ "setup": "Setup1", "post": "Haas" }));
    assert_eq!(r["own_post"], json!(true), "{r}");
    assert!(r["post"].as_str().unwrap_or_default().contains("Haas"), "{r}");
    assert!(own(&mut c, 0).as_str().unwrap_or_default().contains("Haas"));
    assert_eq!(own(&mut c, 1), Value::Null, "the other setup is untouched");

    c.ok("undo", json!({}));
    assert_eq!(own(&mut c, 0), Value::Null);
    c.ok("redo", json!({}));
    assert!(own(&mut c, 0).as_str().unwrap_or_default().contains("Haas"));

    let cleared = c.ok("set_setup_post", json!({ "setup": "Setup1", "post": null }));
    assert_eq!(cleared["own_post"], json!(false), "{cleared}");
    assert_eq!(own(&mut c, 0), Value::Null);
    c.ok("undo", json!({}));
    assert!(own(&mut c, 0).as_str().unwrap_or_default().contains("Haas"), "undo restores the cleared post");

    let e = c.refused("set_setup_post", json!({ "setup": "Setup1", "post": "No Such Post" }));
    assert!(e.contains("No Such Post"), "{e}");
    let e = c.refused("set_setup_post", json!({ "setup": "Nope", "post": "Haas" }));
    assert!(e.contains("Nope"), "{e}");
    let e = c.refused("set_setup_post", json!({ "setup": "Setup1" }));
    assert!(e.contains("post"), "{e}");
    let e = c.refused("set_setup_post", json!({ "setup": "Setup1", "post": 3 }));
    assert!(e.contains("string or null"), "{e}");
}
