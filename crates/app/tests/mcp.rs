//! The MCP server (#128), driven in-process over its JSON-RPC handler
//! and over a byte pipe: the handshake, the tool list, a whole job
//! (import, stock, tool, operation, generate, verify, post, undo), and
//! the refusals - every bad input is a tool error or a JSON-RPC error,
//! never a panic. #355: the tool library tools - export -> import into a
//! fresh document, the duplicate skip, a malformed file refused, undo/redo.
//!
//! Gate map (M4.4): covers app::mcp, app::script, app::project::json,
//! app::cli, app::tool_library, app::commands (tool_import),
//! app::operation (detect_holes, #312), app::mcp::pockets (detect_pockets, #35).

mod common;
mod mcp_open;

use camcraft_app::mcp::{self, Server};

use serde_json::{json, Value};
use std::path::PathBuf;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A 40 x 40 x 15 box as ASCII STL, z from 0: twelve triangles, wound
/// outward. Written by the test so the fixture is the code.
fn write_box_stl(path: &std::path::Path) {
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
    fn new() -> Self {
        Self { server: Server::new(), next: 1 }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        assert_eq!(reply["id"], json!(id));
        assert_eq!(reply["jsonrpc"], json!("2.0"));
        reply
    }

    /// A tools/call: (isError, structuredContent-or-text).
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
fn handshake_and_tool_list() {
    let mut c = Client::new();
    let init = c.request("initialize", json!({
        "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" }
    }));
    assert_eq!(init["result"]["protocolVersion"], json!("2025-06-18"));
    assert_eq!(init["result"]["serverInfo"]["name"], json!("camcraft"));
    assert!(init["result"]["capabilities"]["tools"].is_object());
    // The initialized notification has no id and gets no answer.
    assert!(c.server.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());

    let list = c.request("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for want in [
        "open_project", "import_model", "save_project", "get_job", "list_operation_types", "add_operation",
        "edit_operation", "delete_operation", "set_tool", "undo", "redo", "generate", "verify", "post",
        "export_tool_library", "import_tool_library", "detect_holes", "detect_pockets",
    ] {
        assert!(names.contains(&want), "missing tool {want}: {names:?}");
    }
    // #355: both library tools take one required path.
    for lib in ["export_tool_library", "import_tool_library"] {
        let t = tools.iter().find(|t| t["name"] == json!(lib)).expect("library tool listed");
        assert_eq!(t["inputSchema"]["required"], json!(["path"]), "{t}");
        assert_eq!(t["inputSchema"]["properties"]["path"]["type"], json!("string"), "{t}");
    }
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], json!("object"), "{t}");
        assert!(t["description"].as_str().is_some_and(|d| !d.is_empty()));
    }
    // An unknown protocol version is answered with ours.
    let other = c.request("initialize", json!({ "protocolVersion": "1999-01-01" }));
    assert_eq!(other["result"]["protocolVersion"], json!(mcp::PROTOCOL_VERSIONS[0]));
}

#[test]
fn a_whole_job_over_the_tools() {
    let dir = tmp_dir("job");
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut c = Client::new();
    c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));

    let imp = c.ok("import_model", json!({ "path": stl.display().to_string() }));
    assert_eq!(imp["model"], json!("block"));
    assert_eq!(imp["facets"], json!(12));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));

    let kinds = c.ok("list_operation_types", json!({}));
    let face = kinds["kinds"].as_array().and_then(|k| k.iter().find(|k| k["label"] == json!("Face"))).cloned();
    let face = face.expect("Face kind listed");
    assert_eq!(face["key"], json!("face"));
    assert!(face["fields"].as_array().is_some_and(|f| f.iter().any(|f| f["name"] == json!("stepover") && f["type"] == json!("number"))));

    let add = c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL", "params": { "stepover": 2.5 } }));
    assert_eq!(add["operation"], json!("Face1"));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["status"], json!("never"));
    assert_eq!(job["operations"][0]["params"]["face"]["stepover"], json!(2.5));
    assert_eq!(job["tools"][0]["name"], json!("EM-0400-2FL"));
    assert!(job["setups"][0]["stock"]["box"].is_object(), "{job}");
    assert!(job["models"].as_array().is_some_and(|m| m.iter().any(|m| m["name"] == json!("block"))));

    let edit = c.ok("edit_operation", json!({ "name": "Face1", "params": { "feed": 1000.0 } }));
    assert_eq!(edit["params"]["face"]["feed"], json!(1000.0));

    let generated = c.ok("generate", json!({}));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(true), "{ver}");
    assert!(ver["findings"].is_array());

    let nc = dir.join("out.nc");
    let posted = c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert_eq!(posted["files"], json!([nc.display().to_string()]));
    assert!(posted["lines"].as_u64().is_some_and(|n| n > 0));
    assert!(std::fs::metadata(&nc).is_ok_and(|m| m.len() > 0), "the program was written");

    // Undo walks the History: the generation, then the edit.
    let u = c.ok("undo", json!({}));
    assert_eq!(u["undone"], json!(true));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["status"], json!("never"), "undoing the first generation drops its cache: {job}");
    c.ok("undo", json!({}));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["face"]["feed"], json!(1200.0));
    c.ok("redo", json!({}));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["face"]["feed"], json!(1000.0));

    // Save, delete, reopen: the saved file still holds the op.
    let camproj = dir.join("job.camproj");
    c.ok("save_project", json!({ "path": camproj.display().to_string() }));
    c.ok("delete_operation", json!({ "name": "Face1" }));
    assert_eq!(c.ok("get_job", json!({}))["operations"], json!([]));
    let opened = c.ok("open_project", json!({ "path": camproj.display().to_string() }));
    assert_eq!(opened["operations"][0]["key"], json!("Face1"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_input_is_a_tool_error_never_a_panic() {
    let mut c = Client::new();
    let msg = c.refused("no_such_tool", json!({}));
    assert!(msg.contains("unknown tool"), "{msg}");
    let msg = c.refused("import_model", json!({ "path": "Z:/definitely/missing/part.stl" }));
    assert!(msg.starts_with("import_model:"), "{msg}");
    let msg = c.refused("get_job", json!({ "bogus": 1 }));
    assert!(msg.contains("unknown argument"), "{msg}");
    c.refused("undo", json!({}));
    c.refused("redo", json!({}));
    c.refused("add_operation", json!({ "kind": "Face", "tool": "no-such-tool" }));
    c.refused("add_operation", json!({ "kind": "Nonsense", "tool": "x" }));
    c.refused("edit_operation", json!({ "name": "Face1", "params": { "feed": 1.0 } }));
    c.refused("post", json!({}));

    // A real op, then patches that must not land.
    c.ok("set_stock", json!({ "box": { "dx": 50.0, "dy": 50.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_operation", json!({ "kind": "face", "tool": "EM-0400-2FL" }));
    let msg = c.refused("edit_operation", json!({ "name": "Face1", "params": { "stepover": "wide" } }));
    assert!(msg.contains("bad params patch"), "{msg}");
    let msg = c.refused("edit_operation", json!({ "name": "Face1", "params": { "stepovr": 2.0 } }));
    assert!(msg.contains("no field \"stepovr\""), "{msg}");
    c.refused("edit_operation", json!({ "name": "Face1", "params": { "stepover": -3.0 } }));
    c.refused("edit_operation", json!({ "name": "Face1", "params": [1, 2] }));
    c.refused("set_stock", json!({ "box": { "dx": -1.0, "dy": 5.0, "dz": 5.0 } }));
    c.refused("add_tool", json!({ "definition": { "kind": "end_mill" } }));
    // None of the refusals reached the History: one undo per real change.
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["face"]["stepover"], json!(3.0));
    for _ in 0..3 {
        c.ok("undo", json!({}));
    }
    c.refused("undo", json!({}));

    // Protocol-level junk: JSON-RPC errors, the session lives on.
    let parse = c.server.handle_line("{ not json").expect("parse errors are answered");
    assert!(parse.contains("-32700"), "{parse}");
    let unknown = c.request("no/such/method", json!({}));
    assert_eq!(unknown["error"]["code"], json!(-32601));
    let bad_call = c.request("tools/call", json!({ "arguments": {} }));
    assert_eq!(bad_call["error"]["code"], json!(-32602));
    assert!(c.server.handle(&json!([1, 2])).is_some_and(|r| r["error"]["code"] == json!(-32600)));
    assert!(c.server.handle_line("   ").is_none());
}

#[test]
fn the_stdio_loop_answers_line_for_line() {
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#, "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_job","arguments":{}}}"#, "\n",
    );
    let mut out = Vec::new();
    mcp::serve(input.as_bytes(), &mut out).expect("serve");
    let text = String::from_utf8(out).expect("utf8");
    let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).expect("one JSON object per line")).collect();
    assert_eq!(lines.len(), 2, "the notification is not answered: {text}");
    assert_eq!(lines[0]["result"]["protocolVersion"], json!("2025-03-26"));
    assert_eq!(lines[1]["result"]["isError"], json!(false));
    assert_eq!(lines[1]["result"]["structuredContent"]["operations"], json!([]));
}

/// The first two catalog names, read through the server itself.
fn two_catalog_tools(c: &mut Client) -> (String, String) {
    let cat = c.ok("list_tool_catalog", json!({}));
    let names: Vec<String> =
        cat["tools"].as_array().map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    assert!(names.len() >= 2, "the catalog has at least two tools: {cat}");
    (names[0].clone(), names[1].clone())
}

fn job_tool_names(c: &mut Client) -> Vec<String> {
    let job = c.ok("get_job", json!({}));
    job["tools"].as_array().map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

#[test]
fn tool_library_export_import_roundtrip_into_a_fresh_document() {
    let dir = tmp_dir("toollib-roundtrip");
    let lib = dir.join("shop.camtools");
    let mut src = Client::new();
    let (a, b) = two_catalog_tools(&mut src);
    src.ok("add_tool", json!({ "catalog": a }));
    src.ok("add_tool", json!({ "catalog": b }));
    let out = src.ok("export_tool_library", json!({ "path": lib.display().to_string() }));
    assert_eq!(out["tools"], json!(2), "{out}");
    let text = std::fs::read_to_string(&lib).expect("library written");
    assert!(text.contains("\"camcraft-tools\""), "the GUI's own envelope: {text}");

    let mut dst = Client::new();
    let imp = dst.ok("import_tool_library", json!({ "path": lib.display().to_string() }));
    assert_eq!(imp["added"].as_array().map(Vec::len), Some(2), "{imp}");
    assert_eq!(imp["skipped"], json!([]));
    let src_job = src.ok("get_job", json!({}));
    let dst_job = dst.ok("get_job", json!({}));
    assert_eq!(src_job["tools"], dst_job["tools"], "the definitions arrive exactly");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tool_library_import_skips_duplicates_and_renames_clashes() {
    let dir = tmp_dir("toollib-dup");
    let lib = dir.join("shop.camtools");
    let mut src = Client::new();
    let (a, b) = two_catalog_tools(&mut src);
    src.ok("add_tool", json!({ "catalog": a }));
    src.ok("add_tool", json!({ "catalog": b }));
    src.ok("export_tool_library", json!({ "path": lib.display().to_string() }));

    // The destination already holds `a` exactly, and a different tool
    // under `b`'s name: `a` is skipped, `b` arrives as "b (2)".
    let mut dst = Client::new();
    dst.ok("add_tool", json!({ "catalog": a }));
    dst.ok("add_tool", json!({ "catalog": a, "name": b }));
    let imp = dst.ok("import_tool_library", json!({ "path": lib.display().to_string() }));
    assert_eq!(imp["skipped"], json!([a]), "{imp}");
    assert_eq!(imp["renamed"], json!([{ "from": b, "to": format!("{b} (2)") }]), "{imp}");
    assert_eq!(imp["added"], json!([]));
    // Into the document that exported it, every tool is a duplicate:
    // nothing to import is refused, and no History entry is made.
    let msg = src.refused("import_tool_library", json!({ "path": lib.display().to_string() }));
    assert!(msg.starts_with("import_tool_library:") && msg.contains("nothing to import"), "{msg}");
    src.ok("undo", json!({}));
    src.ok("undo", json!({}));
    src.refused("undo", json!({}));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tool_library_import_modes_update_and_replace_over_mcp() {
    let dir = tmp_dir("toollib-modes");
    let lib = dir.join("shop.camtools");
    let mut src = Client::new();
    let (a, b) = two_catalog_tools(&mut src);
    src.ok("add_tool", json!({ "catalog": a }));
    src.ok("add_tool", json!({ "catalog": b }));
    src.ok("export_tool_library", json!({ "path": lib.display().to_string() }));

    // The destination holds `a` under the same name but a different tool,
    // plus `Extra`: update puts the file's `a` in place, replace also
    // drops `Extra` (no operation uses it).
    let mut dst = Client::new();
    dst.ok("add_tool", json!({ "catalog": b, "name": a }));
    dst.ok("add_tool", json!({ "catalog": a, "name": "Extra" }));
    let imp = dst.ok("import_tool_library", json!({ "path": lib.display().to_string(), "mode": "update" }));
    assert_eq!(imp["mode"], json!("update"), "{imp}");
    assert_eq!(imp["updated"], json!([a]), "{imp}");
    assert_eq!(imp["added"], json!([b]), "{imp}");
    assert_eq!(imp["renamed"], json!([]));
    let rep = dst.ok("import_tool_library", json!({ "path": lib.display().to_string(), "mode": "replace" }));
    assert_eq!(rep["removed"], json!(["Extra"]), "{rep}");
    let job = dst.ok("get_job", json!({}));
    assert_eq!(job["tools"], src.ok("get_job", json!({}))["tools"], "the document now holds the file's set");
    dst.ok("undo", json!({}));
    assert!(job_tool_names(&mut dst).contains(&"Extra".to_owned()), "undo brings Extra back");
    let msg = dst.refused("import_tool_library", json!({ "path": lib.display().to_string(), "mode": "merge" }));
    assert!(msg.contains("unknown import mode"), "{msg}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tool_library_malformed_files_are_refused_by_name() {
    let dir = tmp_dir("toollib-bad");
    let mut c = Client::new();
    let bad = dir.join("bad.camtools");
    std::fs::write(&bad, "{ not json").expect("write");
    let msg = c.refused("import_tool_library", json!({ "path": bad.display().to_string() }));
    assert!(msg.contains("not a tool library"), "{msg}");
    let insane = dir.join("insane.camtools");
    let mut src = Client::new();
    let (a, _) = two_catalog_tools(&mut src);
    src.ok("add_tool", json!({ "catalog": a }));
    let def = src.ok("get_job", json!({}))["tools"][0]["definition"].clone();
    let mut def = def;
    def["diameter"] = json!(-1.0);
    let text = json!({ "format": "camcraft-tools", "version": 1, "tools": [{ "name": "Broken", "definition": def }] });
    std::fs::write(&insane, text.to_string()).expect("write");
    let msg = c.refused("import_tool_library", json!({ "path": insane.display().to_string() }));
    assert!(msg.contains("Broken"), "the refusal names the tool: {msg}");
    let msg = c.refused("import_tool_library", json!({ "path": dir.join("missing.camtools").display().to_string() }));
    assert!(msg.contains("cannot read"), "{msg}");
    // Nothing landed; nothing to undo. An empty document has nothing to export,
    // and an unwritable path is refused.
    assert!(job_tool_names(&mut c).is_empty());
    c.refused("undo", json!({}));
    let msg = c.refused("export_tool_library", json!({ "path": dir.join("e.camtools").display().to_string() }));
    assert!(msg.contains("no tools"), "{msg}");
    let msg = src.refused("export_tool_library", json!({ "path": dir.join("no/such/dir/e.camtools").display().to_string() }));
    assert!(msg.contains("not written"), "{msg}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tool_library_import_is_one_undo_step() {
    let dir = tmp_dir("toollib-undo");
    let lib = dir.join("shop.camtools");
    let mut src = Client::new();
    let (a, b) = two_catalog_tools(&mut src);
    src.ok("add_tool", json!({ "catalog": a }));
    src.ok("add_tool", json!({ "catalog": b }));
    src.ok("export_tool_library", json!({ "path": lib.display().to_string() }));

    let mut c = Client::new();
    c.ok("import_tool_library", json!({ "path": lib.display().to_string() }));
    assert_eq!(job_tool_names(&mut c).len(), 2);
    c.ok("undo", json!({}));
    assert!(job_tool_names(&mut c).is_empty(), "one undo removes the whole import");
    c.refused("undo", json!({}));
    c.ok("redo", json!({}));
    let mut names = job_tool_names(&mut c);
    names.sort();
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(names, want, "redo brings both back");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A 20 mm disc, 10 tall, with a 6 mm blind bore 6 deep from its top,
/// written as ASCII STL (the bore suites' 16-sided revolve).
fn write_bored_disc_stl(path: &std::path::Path) {
    let m = common::bore::revolve(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (3.0, 10.0), (3.0, 4.0), (0.0, 4.0)]);
    let mut s = String::from("solid disc\n");
    for t in m.facets.iter() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid disc\n");
    std::fs::write(path, s).expect("write stl");
}

#[test]
fn detect_holes_fills_a_drill_from_its_model_and_undo_empties_it() {
    // #312: a drill machines only the holes in its parameters, which the
    // app fills from its Detect holes button - over MCP this is the tool.
    let dir = tmp_dir("holes");
    let stl = dir.join("disc.stl");
    write_bored_disc_stl(&stl);
    let mut c = Client::new();
    c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 30.0, "dy": 30.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "DR-0400" }));
    c.ok("add_operation", json!({ "kind": "drill", "tool": "DR-0400", "source": "disc", "name": "D1" }));
    let before = c.refused("generate", json!({ "name": "D1" }));
    assert!(before.contains("no holes"), "an undetected drill has nothing to machine: {before}");

    let got = c.ok("detect_holes", json!({ "name": "D1" }));
    assert_eq!((got["holes"].clone(), got["radial_holes"].clone()), (json!(1), json!(0)), "{got}");
    let holes = |c: &mut Client| c.ok("get_job", json!({}))["operations"][0]["params"]["drill"]["holes"].as_array().map(Vec::len);
    assert_eq!(holes(&mut c), Some(1), "the bore is stored in the drill's list");
    c.ok("generate", json!({ "name": "D1" }));

    // Undo walks back the generation, then the detection.
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    assert_eq!(holes(&mut c), Some(0), "undo empties the list again");

    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_operation", json!({ "kind": "face", "tool": "EM-0400-2FL", "name": "F1" }));
    let e = c.refused("detect_holes", json!({ "name": "F1" }));
    assert!(e.contains("not a drilling operation"), "{e}");
}

#[test]
fn a_nested_patch_typo_is_refused_by_its_path() {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 50.0, "dy": 50.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_operation", json!({ "kind": "face", "tool": "EM-0400-2FL" }));
    // One level down: `ofset` would have been dropped by serde and the
    // edit reported done while clearance stayed at +25.
    let msg = c.refused("edit_operation", json!({ "name": "Face1", "params": { "clearance_z": { "ofset": 50.0 } } }));
    assert!(msg.contains("unknown field clearance_z.ofset"), "{msg}");
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["face"]["clearance_z"]["offset"], json!(25.0), "{job}");
    // The same typo on add_operation, and the right spelling lands.
    let msg = c.refused("add_operation", json!({ "kind": "face", "tool": "EM-0400-2FL", "params": { "cut_z": { "referense": "fixed" } } }));
    assert!(msg.contains("unknown field cut_z.referense"), "{msg}");
    let edit = c.ok("edit_operation", json!({ "name": "Face1", "params": { "clearance_z": { "offset": 50.0 } } }));
    assert_eq!(edit["params"]["face"]["clearance_z"]["offset"], json!(50.0), "{edit}");
}

#[test]
fn a_null_id_is_an_invalid_request() {
    let mut s = Server::new();
    let reply = s.handle(&json!({ "jsonrpc": "2.0", "id": null, "method": "ping" })).expect("answered");
    assert_eq!(reply["error"]["code"], json!(-32600), "{reply}");
    assert_eq!(reply["id"], Value::Null);
}

fn serve_bytes(input: &[u8]) -> Vec<Value> {
    let mut out = Vec::new();
    mcp::serve(input, &mut out).expect("serve");
    let text = String::from_utf8(out).expect("utf8");
    text.lines().map(|l| serde_json::from_str(l).expect("one JSON object per line")).collect()
}

const PING: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n";

#[test]
fn a_non_utf8_line_is_a_parse_error_and_the_session_reads_on() {
    let mut input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"x\":\"\xff\xfe\"}\n".to_vec();
    input.extend_from_slice(PING);
    let lines = serve_bytes(&input);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0]["error"]["code"], json!(-32700));
    assert_eq!(lines[1]["id"], json!(7));
    assert_eq!(lines[1]["result"], json!({}));
}

#[test]
fn an_oversized_line_is_a_parse_error_and_is_skipped() {
    let mut input = vec![b'x'; mcp::MAX_LINE_BYTES + 4096];
    input.push(b'\n');
    input.extend_from_slice(PING);
    let lines = serve_bytes(&input);
    assert_eq!(lines.len(), 2, "one answer for the long line, one for the ping");
    assert_eq!(lines[0]["error"]["code"], json!(-32700));
    assert!(lines[0]["error"]["message"].as_str().is_some_and(|m| m.contains("longer than")), "{}", lines[0]);
    assert_eq!(lines[1]["id"], json!(7));
}

/// #894: thread milling is drivable with the tools that exist - a
/// thread mill from the catalog or by definition, the Thread
/// operation's pitch and hand as params, the door's refusals by name,
/// undo, and (#992) generate's refusal of a hole nothing drilled.
#[test]
fn thread_milling_over_the_tools() {
    let dir = tmp_dir("thread");
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut c = Client::new();
    c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "TMS-0600-P050-150" }));
    c.ok("add_tool", json!({ "catalog": "TAP-M6" }));
    let job = c.ok("get_job", json!({}));
    let single = job["tools"].as_array().and_then(|t| t.iter().find(|t| t["name"] == json!("TMS-0600-P050-150"))).cloned();
    let single = single.expect("the thread mill is in the document");
    assert_eq!(single["definition"]["kind"], json!("thread_mill_single"), "{single}");
    assert_eq!(single["definition"]["pitch_min"], json!(0.5));
    // A multi-form mill by definition: the catalog's shape with another pitch.
    let mut multi = single["definition"].clone();
    multi["kind"] = json!("thread_mill");
    multi["pitch"] = json!(1.0);
    multi["flute_len"] = json!(10.0);
    multi.as_object_mut().expect("an object").remove("pitch_min");
    c.ok("add_tool", json!({ "name": "TM-P100", "definition": multi }));

    // #1212: the mouth on the block's 15 mm top, where the default R plane (top of model) stands.
    let hole = json!({ "xy": [30.0, 30.0], "radius": 3.45875, "entry_z": 15.0, "floor_z": 8.0, "through": false });
    // A tap is not a thread mill; a single-form mill's pitch must sit inside its range.
    let msg = c.refused("add_operation", json!({ "kind": "Thread", "tool": "TAP-M6" }));
    assert!(msg.contains("thread mill"), "{msg}");
    let add = c.ok("add_operation", json!({
        "kind": "Thread", "tool": "TMS-0600-P050-150",
        "params": { "pitch": 1.0, "hand": "left", "holes": [hole] }
    }));
    let name = add["operation"].as_str().expect("the operation's name").to_owned();
    let msg = c.refused("edit_operation", json!({ "name": name, "params": { "pitch": 2.5 } }));
    assert!(msg.contains("outside this single-form thread mill's 0.5 to 1.5 mm range"), "{msg}");
    let edit = c.ok("edit_operation", json!({ "name": name, "params": { "pitch": 1.25, "hand": "right" } }));
    assert_eq!(edit["params"]["thread"]["pitch"], json!(1.25));
    assert_eq!(edit["params"]["thread"]["hand"], json!("right"));
    // The multi-form mill cuts only its own pitch: the swap is refused while the thread states 1.25.
    let msg = c.refused("set_tool", json!({ "operation": name, "tool": "TM-P100" }));
    assert!(msg.contains("only its own 1 mm pitch"), "{msg}");
    c.ok("undo", json!({}));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["thread"]["pitch"], json!(1.0), "undo restores the pitch");
    assert_eq!(job["operations"][0]["params"]["thread"]["hand"], json!("left"));
    // #992: nothing ahead opened the hole (the block is solid there) - a thread mill is not
    // centre-cutting, so generate refuses the thread by name.
    let msg = c.refused("generate", json!({ "name": name }));
    assert!(msg.contains("the hole at (30.000, 30.000) is not drilled yet"), "{msg}");
}

/// #985: an external thread is drivable with the tools that exist - the
/// `external` param on add/edit, `detect_holes` reading bosses for it,
/// the boss door's refusal by name, undo, and generate's refusal of a
/// boss standing in uncut stock.
#[test]
fn external_thread_milling_over_the_tools() {
    let dir = tmp_dir("thread_boss");
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut c = Client::new();
    c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "TM-0600-P125" }));
    // #1212: the top on the block's 15 mm top, where the default R plane (top of model) stands.
    let boss = json!({ "xy": [30.0, 30.0], "radius": 4.0, "entry_z": 15.0, "floor_z": 10.0, "through": false });
    // A pin thinner than the thread is deep is refused when the operation is made.
    let pin = json!({ "xy": [30.0, 30.0], "radius": 0.7, "entry_z": 20.0, "floor_z": 10.0, "through": false });
    let msg = c.refused("add_operation", json!({ "kind": "Thread", "tool": "TM-0600-P125", "params": { "external": true, "holes": [pin] } }));
    assert!(msg.contains("thinner than the 0.76675 mm thread is deep"), "{msg}");
    let add = c.ok("add_operation", json!({ "kind": "Thread", "tool": "TM-0600-P125", "params": { "holes": [] } }));
    let name = add["operation"].as_str().expect("the operation's name").to_owned();
    assert!(add["params"]["thread"].get("external").is_none(), "an internal thread writes no flag: {add}");
    let edit = c.ok("edit_operation", json!({ "name": name, "params": { "external": true, "holes": [boss] } }));
    assert_eq!(edit["params"]["thread"]["external"], json!(true));
    // The box has no round wall: detect says so for bosses, by name.
    let msg = c.refused("detect_holes", json!({ "name": name }));
    assert!(msg.contains("no upright round bosses"), "{msg}");
    // Nothing ahead machined round the boss: generate refuses the thread by name.
    let msg = c.refused("generate", json!({ "name": name }));
    assert!(msg.contains("the boss at (30.000, 30.000) is not cleared"), "{msg}");
    c.ok("undo", json!({}));
    let job = c.ok("get_job", json!({}));
    assert!(job["operations"][0]["params"]["thread"].get("external").is_none(), "undo restores the internal thread");
}

/// #988: a Thread operation's pass control rides the params door - the
/// flattened keys on add and edit, the door's refusals by name, undo,
/// and no key written while the ride is the #894 default.
#[test]
fn thread_pass_control_over_the_params_door() {
    let dir = tmp_dir("thread_passes");
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut c = Client::new();
    c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "TM-0600-P125" }));
    let hole = json!({ "xy": [30.0, 30.0], "radius": 3.3235, "entry_z": 20.0, "floor_z": 8.0, "through": false });
    let add = c.ok("add_operation", json!({ "kind": "Thread", "tool": "TM-0600-P125", "params": { "holes": [hole] } }));
    let name = add["operation"].as_str().expect("the operation's name").to_owned();
    let thread = &add["params"]["thread"];
    for key in ["pass_schedule", "finish_depth", "spring_passes", "depth_allowance", "spindle_reverse", "cutter_left_hand"] {
        assert!(thread.get(key).is_none(), "the default ride writes no {key}: {add}");
    }
    let msg = c.refused("edit_operation", json!({ "name": name, "params": { "spindle_reverse": true } }));
    assert!(msg.contains("right-hand-cut thread mill under an M4"), "{msg}");
    let msg = c.refused("edit_operation", json!({ "name": name, "params": { "depth_allowance": -0.1 } }));
    assert!(msg.contains("past the thread's standard depth"), "{msg}");
    let edit = c.ok("edit_operation", json!({ "name": name, "params": {
        "pass_schedule": "constant_area", "finish_depth": 0.05, "spring_passes": 1, "depth_allowance": 0.02,
        "spindle_reverse": true, "cutter_left_hand": true
    } }));
    let t = &edit["params"]["thread"];
    assert_eq!(t["pass_schedule"], json!("constant_area"), "{edit}");
    assert_eq!(t["spring_passes"], json!(1));
    assert_eq!(t["cutter_left_hand"], json!(true));
    c.ok("undo", json!({}));
    let job = c.ok("get_job", json!({}));
    assert!(job["operations"][0]["params"]["thread"].get("pass_schedule").is_none(), "undo restores the #894 ride");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #1196: the land width rides the MCP params door - one undoable edit,
/// a nonsense width refused at the door, and the generated path names
/// what stands as land.
#[test]
fn fill_openings_rides_the_params_door_and_undoes() {
    let dir = tmp_dir("land");
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut c = Client::new();
    c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "BN-0600" }));
    let add = c.ok("add_operation", json!({ "kind": "3D Finish", "tool": "BN-0600" }));
    let op = add["operation"].as_str().expect("op name").to_string();
    let job = c.ok("get_job", json!({}));
    assert!(job["operations"][0]["params"]["finish3d"].get("fill_openings").is_none(), "off by default: {job}");
    let edit = c.ok("edit_operation", json!({ "name": op, "params": { "fill_openings": 5.0 } }));
    assert_eq!(edit["params"]["finish3d"]["fill_openings"], json!(5.0));
    let bad = c.refused("edit_operation", json!({ "name": op, "params": { "fill_openings": 0.0 } }));
    assert!(!bad.is_empty());
    let generated = c.ok("generate", json!({}));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    let job = c.ok("get_job", json!({}));
    assert!(job["operations"][0]["params"]["finish3d"].get("fill_openings").is_none(), "undo restores off: {job}");
    let _ = std::fs::remove_dir_all(&dir);
}
