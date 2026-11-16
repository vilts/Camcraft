//! The MCP operation name and order tools (#45), driven in-process over
//! the server's JSON-RPC handler: `rename_operation` changes only the
//! display name (the key stays, so the other tools still find it by
//! either), `reorder_operation` moves an operation within its setup and
//! refuses another setup, each is one undo step, and both survive a
//! save and reopen.
//!
//! Gate map (M4.4): covers app::mcp::order_tools,
//! app::commands::operation::meta (RenameOperation, ReorderOperation).

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::PathBuf;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-order-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
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

    /// The document's operations in program order: (key, display name, setup).
    fn ops(&mut self) -> Vec<(String, String, String)> {
        let job = self.ok("get_job", json!({}));
        job["operations"]
            .as_array()
            .map(|ops| {
                ops.iter()
                    .map(|o| {
                        let s = |k: &str| o[k].as_str().unwrap_or_default().to_owned();
                        (s("key"), s("name"), s("setup"))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn keys(&mut self) -> Vec<String> {
        self.ops().into_iter().map(|(k, _, _)| k).collect()
    }
}

/// Two setups: Face1..Face3 in Setup1, Face4 in Setup2.
fn two_setup_job() -> Client {
    let mut c = Client::new();
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_setup", json!({ "name": "Setup1" }));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    for _ in 0..3 {
        c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" }));
    }
    c.ok("add_setup", json!({ "name": "Setup2", "datum_from": "Setup1" }));
    c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" }));
    c
}

#[test]
fn rename_and_reorder_are_listed_with_their_schemas() {
    let mut c = Client::new();
    let list = c.request("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().cloned().unwrap_or_default();
    let find = |n: &str| tools.iter().find(|t| t["name"] == json!(n)).cloned();
    let rename = find("rename_operation").expect("rename_operation listed");
    assert_eq!(rename["inputSchema"]["required"], json!(["name", "new_name"]), "{rename}");
    let reorder = find("reorder_operation").expect("reorder_operation listed");
    assert_eq!(reorder["inputSchema"]["required"], json!(["name", "target"]), "{reorder}");
    assert_eq!(reorder["inputSchema"]["properties"]["place"]["enum"], json!(["before", "after"]));
}

#[test]
fn reorder_moves_within_its_setup_one_undo_step_and_refuses_another_setup() {
    let mut c = two_setup_job();
    assert_eq!(c.keys(), ["Face1", "Face2", "Face3", "Face4"]);

    let moved = c.ok("reorder_operation", json!({ "name": "Face3", "target": "Face1" }));
    assert_eq!(moved["setup"], json!("Setup1"), "{moved}");
    let order: Vec<&str> = moved["order"].as_array().map(|o| o.iter().filter_map(|e| e["key"].as_str()).collect()).unwrap_or_default();
    assert_eq!(order, ["Face3", "Face1", "Face2"], "the answer is the setup's new order: {moved}");
    // Setup ranks still lead: Setup2's operation stays after all of Setup1's.
    assert_eq!(c.keys(), ["Face3", "Face1", "Face2", "Face4"]);

    c.ok("reorder_operation", json!({ "name": "Face1", "target": "Face2", "place": "after" }));
    assert_eq!(c.keys(), ["Face3", "Face2", "Face1", "Face4"]);

    // One undo step each.
    c.ok("undo", json!({}));
    assert_eq!(c.keys(), ["Face3", "Face1", "Face2", "Face4"]);
    c.ok("undo", json!({}));
    assert_eq!(c.keys(), ["Face1", "Face2", "Face3", "Face4"]);
    c.ok("redo", json!({}));
    assert_eq!(c.keys(), ["Face3", "Face1", "Face2", "Face4"]);

    // Refusals, each by name and with the order untouched.
    let cross = c.refused("reorder_operation", json!({ "name": "Face4", "target": "Face1" }));
    assert!(cross.contains("within one setup"), "{cross}");
    let same = c.refused("reorder_operation", json!({ "name": "Face3", "target": "Face1" }));
    assert!(same.contains("already in that position"), "{same}");
    let place = c.refused("reorder_operation", json!({ "name": "Face3", "target": "Face1", "place": "middle" }));
    assert!(place.contains("\"place\""), "{place}");
    let missing = c.refused("reorder_operation", json!({ "name": "Nope", "target": "Face1" }));
    assert!(missing.contains("Nope"), "{missing}");
    assert_eq!(c.keys(), ["Face3", "Face1", "Face2", "Face4"]);
}

#[test]
fn rename_keeps_the_key_and_the_order_and_round_trips() {
    let mut c = two_setup_job();
    let renamed = c.ok("rename_operation", json!({ "name": "Face2", "new_name": "Finish top" }));
    assert_eq!(renamed, json!({ "operation": "Face2", "name": "Finish top" }));
    let ops = c.ops();
    assert_eq!(ops[1], ("Face2".to_owned(), "Finish top".to_owned(), "Setup1".to_owned()), "{ops:?}");
    assert_eq!(c.keys(), ["Face1", "Face2", "Face3", "Face4"], "a rename never moves an operation");

    // Other tools find it by its display name as well as by its key.
    c.ok("reorder_operation", json!({ "name": "Finish top", "target": "Face1" }));
    assert_eq!(c.keys(), ["Face2", "Face1", "Face3", "Face4"]);

    // A taken name - another's label or another's key - is refused.
    let dup = c.refused("rename_operation", json!({ "name": "Face3", "new_name": "Finish top" }));
    assert!(dup.contains("Finish top"), "{dup}");
    let key = c.refused("rename_operation", json!({ "name": "Face3", "new_name": "Face4" }));
    assert!(key.contains("Face4"), "{key}");
    let blank = c.refused("rename_operation", json!({ "name": "Face3", "new_name": "   " }));
    assert!(blank.contains("printable"), "{blank}");

    // Save and reopen: the display name and the saved order come back.
    let dir = tmp_dir("roundtrip");
    let camproj = dir.join("order.camproj");
    c.ok("save_project", json!({ "path": camproj.display().to_string() }));
    let mut fresh = Client::new();
    fresh.ok("open_project", json!({ "path": camproj.display().to_string() }));
    let ops = fresh.ops();
    let keys: Vec<&str> = ops.iter().map(|(k, _, _)| k.as_str()).collect();
    assert_eq!(keys, ["Face2", "Face1", "Face3", "Face4"], "{ops:?}");
    assert_eq!(ops[0].1, "Finish top");

    // Undo walks back the reorder, then the rename.
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    assert_eq!(c.ops()[1].1, "Face2", "undoing the rename shows the key again");
    let _ = std::fs::remove_dir_all(&dir);
}
