//! The MCP workholding-rpm tool (#1031), driven in-process over the
//! server's JSON-RPC handler: a setup states its chuck's rating, undo
//! walks it back, a cleared limit reads none, and bad input is a tool
//! error carrying the command's own words.
//!
//! Gate map (M4.4): covers app::mcp::workholding_tools.

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
}

#[test]
fn set_workholding_max_rpm_states_clears_and_undoes_the_chucks_rating() {
    let mut c = Client::new();
    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    assert!(names.iter().any(|n| n == "set_workholding_max_rpm"), "{names:?}");

    let why = c.refused("set_workholding_max_rpm", json!({ "max_rpm": 2000.0 }));
    assert!(why.contains("add_setup"), "no setup yet: {why}");
    c.ok("set_stock", json!({ "box": { "dx": 20.0, "dy": 20.0, "dz": 20.0 } }));
    c.ok("add_setup", json!({}));

    let set = c.ok("set_workholding_max_rpm", json!({ "max_rpm": 2000.0 }));
    assert_eq!(set, json!({ "setup": "Setup1", "max_rpm": 2000.0 }));
    assert_eq!(c.ok("get_job", json!({}))["setups"][0]["workholding_max_rpm"], json!(2000.0), "get_job reads it back");
    let again = c.refused("set_workholding_max_rpm", json!({ "setup": "Setup1", "max_rpm": 2000.0 }));
    assert!(again.contains("already states"), "{again}");
    for (bad, words) in [(json!(0.0), "at least 1 rpm"), (json!(1.0e6), "past any lathe"), (json!("fast"), "a number or null")] {
        let why = c.refused("set_workholding_max_rpm", json!({ "max_rpm": bad }));
        assert!(why.contains(words), "{bad}: {why}");
    }
    let why = c.refused("set_workholding_max_rpm", json!({ "setup": "Nope", "max_rpm": 2000.0 }));
    assert!(why.contains("no setup named 'Nope'"), "{why}");
    assert!(c.refused("set_workholding_max_rpm", json!({})).contains("required"));

    let cleared = c.ok("set_workholding_max_rpm", json!({ "max_rpm": null }));
    assert_eq!(cleared["max_rpm"], Value::Null);
    c.ok("undo", json!({}));
    let restored = c.refused("set_workholding_max_rpm", json!({ "max_rpm": 2000.0 }));
    assert!(restored.contains("already states a workholding maximum of 2000"), "undo put 2000 back: {restored}");
}
