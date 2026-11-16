//! The MCP cutting-data tools (#732), driven in-process over the server's
//! JSON-RPC handler: the workpiece material stated, the suggestion read
//! without a write, applied as one undo step (op feed and tool rpm), and
//! every refusal named.
//!
//! Gate map (M4.4): covers app::mcp::cutting_tools.

use camcraft_app::mcp::Server;

use serde_json::{Value, json};

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
fn cutting_data_is_stated_suggested_applied_and_undone_over_mcp() {
    let mut c = Client::new();
    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    for want in ["set_work_material", "suggest_cutting_data", "apply_cutting_data"] {
        assert!(names.iter().any(|n| n == want), "missing tool {want}");
    }
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" }));
    assert_eq!(c.ok("get_job", json!({}))["work_material"], Value::Null);

    // Refusals by name, nothing written.
    let why = c.refused("suggest_cutting_data", json!({ "operation": "Face1" }));
    assert!(why.contains("no workpiece material is stated"), "{why}");
    let why = c.refused("apply_cutting_data", json!({ "operation": "Face1" }));
    assert!(why.contains("no workpiece material is stated"), "{why}");
    let why = c.refused("set_work_material", json!({ "material": "unobtainium" }));
    assert!(why.contains("unknown workpiece material 'unobtainium'") && why.contains("brass"), "{why}");
    let why = c.refused("set_work_material", json!({ "material": null }));
    assert!(why.contains("already stated"), "{why}");
    c.refused("set_work_material", json!({}));

    let set = c.ok("set_work_material", json!({ "material": "aluminium_6061" }));
    assert_eq!(set["work_material"], json!("aluminium_6061"));
    assert_eq!(c.ok("get_job", json!({}))["work_material"], json!("aluminium_6061"));
    let why = c.refused("set_work_material", json!({ "material": "aluminium_6061" }));
    assert!(why.contains("already stated"), "{why}");

    let before = c.ok("get_job", json!({}));
    let sug = c.ok("suggest_cutting_data", json!({ "operation": "Face1" }));
    assert_eq!(sug["material"], json!("aluminium_6061"));
    let (rpm, feed) = (sug["rpm"].as_f64().unwrap_or(0.0), sug["feed"].as_f64().unwrap_or(0.0));
    assert!(rpm > 0.0 && feed > 0.0, "{sug}");
    assert_eq!(c.ok("get_job", json!({})), before, "a suggestion writes nothing");
    c.refused("suggest_cutting_data", json!({ "operation": "Nope" }));

    let applied = c.ok("apply_cutting_data", json!({ "operation": "Face1" }));
    assert_eq!((applied["rpm"].as_f64(), applied["feed"].as_f64()), (Some(rpm), Some(feed)));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["face"]["feed"], json!(feed));
    assert_eq!(job["tools"][0]["definition"]["rpm"], json!(rpm), "{}", job["tools"][0]);
    let why = c.refused("apply_cutting_data", json!({ "operation": "Face1" }));
    assert!(why.contains("already runs the suggestion"), "{why}");

    // One undo step takes both numbers back; the next takes the material.
    c.ok("undo", json!({}));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["face"]["feed"], before["operations"][0]["params"]["face"]["feed"]);
    assert_eq!(job["tools"], before["tools"]);
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["work_material"], Value::Null);
    c.ok("redo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["work_material"], json!("aluminium_6061"));
}
