//! `edit_tool` over MCP (#1062): a partial tool definition through the
//! undoable `ChangeTool`, in-process over the JSON-RPC handler.
//!
//! Gate map (M4.4): covers app::mcp::tool_edit, app::commands (ChangeTool).

use camcraft_app::mcp::Server;

use serde_json::{json, Value};

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        let mut c = Self { server: Server::new(), next: 1 };
        c.call_raw("initialize", json!({ "protocolVersion": "2025-06-18" }));
        c
    }

    fn call_raw(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.server.handle(&msg).expect("a request gets a response")
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let reply = self.call_raw("tools/call", json!({ "name": name, "arguments": args }));
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

    fn tool(&mut self, name: &str) -> Value {
        let job = self.ok("get_job", json!({}));
        job["tools"].as_array().and_then(|t| t.iter().find(|t| t["name"] == json!(name))).cloned().unwrap_or(Value::Null)
    }
}

fn setup() -> Client {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c
}

#[test]
fn edit_tool_changes_a_field_and_undo_redo_walk_it() {
    let mut c = setup();
    let before = c.tool("EM-0400-2FL")["definition"]["rpm"].clone();
    let r = c.ok("edit_tool", json!({ "name": "EM-0400-2FL", "patch": { "rpm": 4321.0, "vendor": "Acme" } }));
    assert_eq!(r["definition"]["rpm"], json!(4321.0));
    let t = c.tool("EM-0400-2FL");
    assert_eq!(t["definition"]["rpm"], json!(4321.0), "get_job reflects it: {t}");
    assert_eq!(t["definition"]["vendor"], json!("Acme"));
    c.ok("undo", json!({}));
    assert_eq!(c.tool("EM-0400-2FL")["definition"]["rpm"], before, "one undo reverts every patched field");
    assert!(c.tool("EM-0400-2FL")["definition"]["vendor"].is_null());
    c.ok("redo", json!({}));
    assert_eq!(c.tool("EM-0400-2FL")["definition"]["rpm"], json!(4321.0));
    assert_eq!(c.tool("EM-0400-2FL")["definition"]["vendor"], json!("Acme"));
}

#[test]
fn edit_tool_geometry_marks_operations_stale() {
    let mut c = setup();
    c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL", "name": "F1" }));
    let g = c.ok("generate", json!({}));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
    c.ok("edit_tool", json!({ "name": "EM-0400-2FL", "patch": { "diameter": 5.0, "flute_len": 12.0 } }));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["status"], json!("stale"), "{job}");
}

#[test]
fn edit_tool_refusals_name_the_problem() {
    let mut c = setup();
    let e = c.refused("edit_tool", json!({ "name": "nope", "patch": { "rpm": 1000.0 } }));
    assert!(e.contains("nope"), "{e}");
    let e = c.refused("edit_tool", json!({ "name": "EM-0400-2FL", "patch": { "rmp": 1000.0 } }));
    assert!(e.contains("rmp"), "an unknown key is named: {e}");
    let e = c.refused("edit_tool", json!({ "name": "EM-0400-2FL", "patch": { "number": 3 } }));
    assert!(e.contains("number"), "{e}");
    let e = c.refused("edit_tool", json!({ "name": "EM-0400-2FL", "patch": { "diameter": -1.0 } }));
    assert!(e.contains("positive") || e.contains("EM-0400-2FL"), "{e}");
    let e = c.refused("edit_tool", json!({ "name": "EM-0400-2FL", "patch": { "cutting_data": [
        { "material": "brass", "rpm": 100.0, "feed": 10.0 }, { "material": "brass", "rpm": 200.0, "feed": 20.0 } ] } }));
    assert!(e.contains("EM-0400-2FL"), "{e}");
    c.refused("edit_tool", json!({ "name": "EM-0400-2FL", "patch": "rpm" }));
    // A retype that would strand a Face operation is refused by name.
    c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL", "name": "F1" }));
    let e = c.refused("edit_tool", json!({ "name": "EM-0400-2FL", "patch": { "kind": "drill", "tip_angle": 118.0 } }));
    assert!(e.contains("F1"), "{e}");
    assert_eq!(c.tool("EM-0400-2FL")["definition"]["kind"], json!("end_mill"));
}
