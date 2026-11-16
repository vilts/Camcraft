//! `delete_tool` over MCP (#734): the undoable `DeleteTool`,
//! in-process over the JSON-RPC handler.
//!
//! Gate map (M4.4): covers app::mcp::tool_delete, app::commands (DeleteTool).

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

fn has_tool(c: &mut Client, name: &str) -> bool {
    !c.tool(name).is_null()
}

fn setup() -> Client {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c
}

#[test]
fn delete_tool_removes_it_and_undo_redo_walk_it() {
    let mut c = setup();
    let before = c.tool("EM-0400-2FL");
    assert!(!before.is_null());
    let r = c.ok("delete_tool", json!({ "name": "EM-0400-2FL" }));
    assert_eq!(r["deleted"], json!("EM-0400-2FL"));
    assert!(!has_tool(&mut c, "EM-0400-2FL"), "gone from get_job");
    c.ok("undo", json!({}));
    assert_eq!(c.tool("EM-0400-2FL"), before, "one undo restores the tool exactly");
    c.ok("redo", json!({}));
    assert!(!has_tool(&mut c, "EM-0400-2FL"), "redo deletes it again");
}

#[test]
fn delete_tool_refuses_a_tool_an_operation_uses() {
    let mut c = setup();
    c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL", "name": "F1" }));
    let before = c.tool("EM-0400-2FL");
    let e = c.refused("delete_tool", json!({ "name": "EM-0400-2FL" }));
    assert!(e.contains("EM-0400-2FL") && e.contains("F1"), "names tool and operation: {e}");
    assert_eq!(c.tool("EM-0400-2FL"), before, "nothing changed");
    // Nothing was pushed to history: undo reverts the add_operation, not a delete.
    c.ok("undo", json!({}));
    assert!(has_tool(&mut c, "EM-0400-2FL"));
}

#[test]
fn delete_tool_refuses_an_unknown_name_and_a_non_tool() {
    let mut c = setup();
    let e = c.refused("delete_tool", json!({ "name": "nope" }));
    assert!(e.contains("nope"), "{e}");
    c.refused("delete_tool", json!({}));
    assert!(has_tool(&mut c, "EM-0400-2FL"));
}
