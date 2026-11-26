//! `verify`'s `ok` is the post's whole verdict (#722): it reads false
//! while an operation's own warnings hold the post, even when every op
//! is fresh and the program gate passed - and the batch CLI's exit code,
//! which reads the same flag, follows. Driven in-process over the MCP
//! server's JSON-RPC handler, plus the real `camcraft-cli process` lane.
//!
//! The fixture is a real collision, not a staged one: `EM-0400-2FL` is a
//! Ø4 cutter with 12 mm of flute on a Ø6 shank, so contouring a 30 mm
//! wall rubs the shank on the part below the flutes (`shank_hit`, which
//! blocks the post). A 10 mm wall stays inside the flutes and is clean.
//!
//! Gate map (M4.4): covers app::script::api, app::mcp::job, app::cli.

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Wall heights either side of the tool's 12 mm flute length.
const WALL_PAST_THE_FLUTES_MM: f64 = 30.0;
const WALL_INSIDE_THE_FLUTES_MM: f64 = 10.0;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-verify-{tag}-{}", std::process::id()));
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

/// A 40 x 40 box `wall` mm tall in matching box stock, contoured all the
/// way down with `EM-0400-2FL` as op `C1`, generated.
fn contour_job(dir: &Path, wall: f64) -> Client {
    let part = dir.join("part.stl");
    write_box_stl(&part, (20.0, 20.0, wall));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": wall } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_operation", json!({ "kind": "contour", "tool": "EM-0400-2FL", "name": "C1", "source": "part" }));
    let generated = c.ok("generate", json!({}));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    c
}

/// The #722 case: fresh op, passing gate, but every finding blocks the
/// post - `ok` must say so and name the op, because the post refuses.
#[test]
fn verify_is_not_ok_while_an_ops_own_warnings_hold_the_post() {
    let dir = tmp_dir("held");
    let mut c = contour_job(&dir, WALL_PAST_THE_FLUTES_MM);

    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["gate"]["ok"], json!(true), "the program gate itself passes: {ver}");
    assert_eq!(ver["operations"][0]["status"], json!("ok"), "{ver}");
    let findings = ver["findings"].as_array().cloned().unwrap_or_default();
    assert!(!findings.is_empty(), "the shank rubs the wall below the flutes: {ver}");
    // #3525: the cause first - the cut is deeper than the flute (advisory);
    // every finding after it is a shank hit that holds the post.
    assert_eq!(findings[0]["kind"], json!("flute_short"), "{ver}");
    assert_eq!(findings[0]["blocks_post"], json!(false), "{ver}");
    assert!(findings.len() > 1, "{ver}");
    assert!(findings[1..].iter().all(|f| f["blocks_post"] == json!(true) && f["kind"] == json!("shank_hit")), "{ver}");
    assert_eq!(ver["ok"], json!(false), "a program the post refuses is not ok: {ver}");
    assert_eq!(ver["held_by"], json!(["C1"]), "the reply names the op holding the post: {ver}");
    // Asking about the op by name gives the same answer.
    let named = c.ok("verify", json!({ "name": "C1" }));
    assert_eq!((named["ok"].clone(), named["held_by"].clone()), (json!(false), json!(["C1"])), "{named}");

    let why = c.refused("post", json!({ "post": "Fanuc", "output": dir.join("held.nc").display().to_string() }));
    assert!(why.contains("warning(s)"), "the post refuses on the same warnings: {why}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The neighbour that must not move: the same contour on a wall inside
/// the flutes has no findings, verifies ok and posts.
#[test]
fn a_contour_inside_the_flutes_still_verifies_ok_and_posts() {
    let dir = tmp_dir("clean");
    let mut c = contour_job(&dir, WALL_INSIDE_THE_FLUTES_MM);

    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["findings"], json!([]), "{ver}");
    assert_eq!(ver["ok"], json!(true), "{ver}");
    assert_eq!(ver["held_by"], json!([]), "{ver}");
    let nc = dir.join("clean.nc");
    let posted = c.ok("post", json!({ "post": "Fanuc", "output": nc.display().to_string() }));
    assert!(posted["lines"].as_u64().is_some_and(|n| n > 0), "{posted}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The batch lane reads the same flag: `camcraft-cli process` without
/// `-o` exits 1 for the held job (it exited 0, "clean", before #722) and
/// still exits 0 for the clean one.
#[test]
fn the_cli_process_lane_exits_one_for_a_job_its_warnings_hold() {
    for (tag, wall, want) in [("cli-held", WALL_PAST_THE_FLUTES_MM, 1), ("cli-clean", WALL_INSIDE_THE_FLUTES_MM, 0)] {
        let dir = tmp_dir(tag);
        let mut c = contour_job(&dir, wall);
        let proj = dir.join("job.camproj");
        c.ok("save_project", json!({ "path": proj.display().to_string() }));

        let out = Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
            .args(["process", &proj.display().to_string()])
            .output()
            .expect("the CLI binary runs");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(want), "{tag}: {stderr}");
        if want == 1 {
            assert!(stderr.contains("not clean"), "{tag}: the CLI says why: {stderr}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
