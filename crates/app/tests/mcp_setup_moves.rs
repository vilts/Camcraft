//! `set_operation_setup` and `delete_setup` over MCP (#731), driven in-process
//! over the server's JSON-RPC handler: an operation moves between two
//! setups, the emptied setup is deleted, undo and redo walk both, and every
//! refusal is a tool error naming its cause.
//!
//! Gate map (#731): covers app::mcp::setup_tools,
//! app::commands (SetOperationSetup, DeleteSetup).

use camcraft_app::mcp::Server;

use serde_json::{json, Value};

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

    /// The setup names and each operation's declared setup, from get_job.
    fn shape(&mut self) -> (Vec<String>, Vec<(String, Value)>) {
        let job = self.ok("get_job", json!({}));
        let setups = job["setups"].as_array().map(|a| a.iter().filter_map(|s| s["name"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
        let ops = job["operations"].as_array().map(|a| a.iter().map(|o| (o["name"].as_str().unwrap_or_default().to_owned(), o["setup"].clone())).collect()).unwrap_or_default();
        (setups, ops)
    }
}

/// Two declared setups, "Front" and "Back", and a Face operation made in Front.
fn two_setups() -> (Client, String) {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_setup", json!({ "name": "Front" }));
    c.ok("add_setup", json!({ "name": "Back", "datum_from": "Front" }));
    c.ok("activate_setup", json!({ "name": "Front" }));
    let op = c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" }));
    let name = op["operation"].as_str().unwrap_or_default().to_owned();
    (c, name)
}

#[test]
fn move_an_operation_delete_the_emptied_setup_and_undo_redo_both() {
    let (mut c, op) = two_setups();
    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    for want in ["set_operation_setup", "delete_setup"] {
        assert!(names.iter().any(|n| n == want), "missing tool {want}");
    }
    assert_eq!(c.shape().1[0].1, json!("Front"));

    let moved = c.ok("set_operation_setup", json!({ "operation": op, "setup": "Back" }));
    assert_eq!(moved["setup"], json!("Back"), "{moved}");
    assert!(moved["status"].is_string(), "the stamp is kept and read back: {moved}");
    assert_eq!(c.shape().1[0].1, json!("Back"));

    // Front is empty now: delete it, one undo step.
    let del = c.ok("delete_setup", json!({ "name": "Front" }));
    assert_eq!(del["setups"], json!(["Back"]), "{del}");
    assert_eq!(c.shape().0, vec!["Back".to_owned()]);
    c.ok("undo", json!({}));
    // Setups list in name order, so undo puts Front back where it was (#1362).
    assert_eq!(c.shape().0, vec!["Back".to_owned(), "Front".to_owned()], "undo restores the setup in place");
    c.ok("redo", json!({}));
    assert_eq!(c.shape().0, vec!["Back".to_owned()]);
    c.ok("undo", json!({}));
    // One more undo reverts the move itself.
    c.ok("undo", json!({}));
    assert_eq!(c.shape().1[0].1, json!("Front"));
    c.ok("redo", json!({}));
    assert_eq!(c.shape().1[0].1, json!("Back"));
}

#[test]
fn refusals_name_their_cause_and_change_nothing() {
    let (mut c, op) = two_setups();
    let before = c.shape();

    assert!(c.refused("set_operation_setup", json!({ "operation": "Nope", "setup": "Back" })).contains("Nope"));
    assert!(c.refused("set_operation_setup", json!({ "operation": op, "setup": "Ghost" })).contains("Ghost"));
    assert!(c.refused("set_operation_setup", json!({ "operation": op, "setup": "Front" })).contains("already machines"));
    assert!(c.refused("set_operation_setup", json!({ "operation": op })).contains("setup"));
    assert!(c.refused("delete_setup", json!({ "name": "Ghost" })).contains("Ghost"));
    let busy = c.refused("delete_setup", json!({ "name": "Front" }));
    assert!(busy.contains("machines in 'Front'") && busy.contains("move or delete it first"), "{busy}");

    assert_eq!(c.shape(), before, "refusals leave the document alone");
}
