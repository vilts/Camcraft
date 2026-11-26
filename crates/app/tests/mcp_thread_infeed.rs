//! Gate map (M4.4): covers app::mcp::thread_infeed_tools, app::mcp::tools.
//! Provenance: our MCP contract over the existing threading command.

use camcraft_app::mcp::Server;
use serde_json::{Value, json};

struct Client {
    server: Server,
    next: u64,
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
        self.server.handle(&json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        })).expect("a request has a response")
    }

    fn call(&mut self, name: &str, arguments: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": arguments }));
        let result = &reply["result"];
        assert!(result.is_object(), "tool result: {reply}");
        let error = result["isError"] == true;
        let body = if error { result["content"][0]["text"].clone() }
            else { result["structuredContent"].clone() };
        (error, body)
    }

    fn ok(&mut self, name: &str, arguments: Value) -> Value {
        let (error, body) = self.call(name, arguments);
        assert!(!error, "{name}: {body}");
        body
    }

    fn refused(&mut self, name: &str, arguments: Value) -> String {
        let (error, body) = self.call(name, arguments);
        assert!(error, "{name} must refuse: {body}");
        body.as_str().expect("refusal text").to_owned()
    }

    fn thread() -> Self {
        let mut c = Self::new();
        c.ok("load_machine", json!({ "builtin": "Tormach 15L Slant-PRO" }));
        c.ok("set_stock", json!({ "cylinder": { "diameter": 12.0, "height": 56.0 } }));
        c.ok("add_tool", json!({ "catalog": "TD-1732-60", "holder": "none" }));
        c.ok("add_operation", json!({
            "name": "TH1", "kind": "threading", "tool": "TD-1732-60"
        }));
        c
    }

    fn plan(&mut self) -> Value {
        self.ok("get_thread_infeed", json!({ "name": "TH1" }))["infeed_plan"].clone()
    }
}

#[test]
fn thread_infeed_tools_are_listed() {
    let mut c = Client::new();
    let response = c.request("tools/list", json!({}));
    let tools = response["result"]["tools"].as_array().expect("tools array");
    for name in ["get_thread_infeed", "set_thread_infeed"] {
        assert!(tools.iter().any(|t| t["name"] == name), "missing tool {name}");
    }
}

#[test]
fn thread_infeed_edits_read_back_and_undo_redo_restores_the_plan() {
    let mut c = Client::thread();
    let before = c.plan();
    assert_eq!(before["method"], "radial");
    assert_eq!(before["repeat_each_depth"], false, "defaults remain visible");
    assert!(before["dips"].is_object(), "default dips remain visible");
    let changed = c.ok("set_thread_infeed", json!({ "name": "TH1", "infeed_plan": {
        "method": "alternating", "flank_angle": 29.5, "schedule": "constant_area",
        "min_depth": 0.02, "finish_allowance": 0.01,
        "spring_passes": 2, "repeat_each_depth": true
    } }))["infeed_plan"].clone();
    assert_eq!(changed["method"], "alternating");
    assert_eq!(changed["schedule"], "constant_area");
    assert_eq!(changed["flank_angle"], 29.5);
    assert_eq!(changed["min_depth"], 0.02);
    assert_eq!(changed["finish_allowance"], 0.01);
    assert_eq!(changed["spring_passes"], 2);
    assert_eq!(changed["repeat_each_depth"], true);
    assert_eq!(c.plan(), changed);
    c.ok("undo", json!({}));
    assert_eq!(c.plan(), before);
    c.ok("redo", json!({}));
    assert_eq!(c.plan(), changed);
    c.ok("set_thread_infeed", json!({ "name": "TH1", "infeed_plan": {
        "repeat_each_depth": false, "spring_passes": 0
    } }));
    let reset = c.plan();
    assert_eq!(reset["repeat_each_depth"], false);
    assert_eq!(reset["spring_passes"], 0);
    assert_eq!(reset["method"], "alternating", "a partial edit retains other fields");
    c.ok("set_thread_infeed", json!({ "name": "TH1", "infeed_plan": before }));
    assert_eq!(c.plan(), before, "the whole default plan can be restored");
}

#[test]
fn thread_infeed_refuses_bad_fields_and_values_without_changing_the_plan() {
    let mut c = Client::thread();
    let before = c.plan();
    for (patch, field) in [
        (json!({ "spring_passess": 2 }), "spring_passess"),
        (json!({ "spring_passes": -1 }), "spring_passes"),
        (json!({ "spring_passes": 1.5 }), "spring_passes"),
        (json!({ "repeat_each_depth": "yes" }), "repeat_each_depth"),
        (json!({ "method": "sideways" }), "method"),
        (json!({ "min_depth": -0.1 }), "depth"),
        (json!({ "dips": { "heigth": 0.1 } }), "heigth"),
        (json!({ "dips": { "heigth": null } }), "heigth"),
    ] {
        let why = c.refused("set_thread_infeed", json!({ "name": "TH1", "infeed_plan": patch }));
        assert!(why.contains(field), "refusal must name {field}: {why}");
        assert_eq!(c.plan(), before, "a refusal changes nothing");
    }
    for args in [
        json!({ "name": "TH1", "infeed_plan": [] }),
        json!({ "name": "TH1", "infeed_plan": null }),
        json!({ "name": "TH1" }),
    ] {
        let why = c.refused("set_thread_infeed", args);
        assert!(why.contains("infeed_plan"), "{why}");
        assert_eq!(c.plan(), before, "a malformed plan changes nothing");
    }
    let why = c.refused("set_thread_infeed", json!({
        "name": "TH1", "infeed_plan": {}, "extra": true
    }));
    assert!(why.contains("extra"), "{why}");
    assert_eq!(c.plan(), before, "an unknown argument changes nothing");
    let why = c.refused("get_thread_infeed", json!({ "name": "missing" }));
    assert!(why.contains("missing"), "{why}");
    c.ok("load_machine", json!({ "builtin": "Tormach 770MX + microARC 4" }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL", "name": "mill", "holder": "none" }));
    c.ok("add_operation", json!({ "name": "P1", "kind": "pocket", "tool": "mill" }));
    for tool in ["get_thread_infeed", "set_thread_infeed"] {
        let args = if tool == "get_thread_infeed" { json!({ "name": "P1" }) }
            else { json!({ "name": "P1", "infeed_plan": {} }) };
        let why = c.refused(tool, args);
        assert!(why.contains("P1") && why.to_ascii_lowercase().contains("thread"), "{why}");
    }
}
