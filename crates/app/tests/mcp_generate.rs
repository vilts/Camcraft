//! MCP generation freshness regression (#2439).
//! Provenance: our own MCP contract and program-order dependency rules.
//! Gate map: covers app::script::api, app::mcp::tools.

use camcraft_app::mcp::Server;
use serde_json::{json, Value};

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        Self { server: Server::new(), next: 1 }
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let msg = json!({
            "jsonrpc": "2.0", "id": self.next, "method": "tools/call",
            "params": { "name": name, "arguments": args }
        });
        self.next += 1;
        let reply = self.server.handle(&msg).expect("request answered");
        let result = &reply["result"];
        assert!(result.is_object(), "{reply}");
        assert_ne!(result["isError"], json!(true), "{name}: {result}");
        result["structuredContent"].clone()
    }

    fn statuses(&mut self) -> Vec<String> {
        self.ok("get_job", json!({}))["operations"]
            .as_array().expect("operations")
            .iter().map(|op| op["status"].as_str().expect("status").to_owned()).collect()
    }
}

fn three_faces() -> Client {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 20.0, "dy": 20.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    for name in ["First", "Second", "Third"] {
        c.ok("add_operation", json!({ "kind": "Face", "name": name, "tool": "EM-0400-2FL" }));
    }
    c.ok("generate", json!({}));
    assert_eq!(c.statuses(), ["ok", "ok", "ok"]);
    c
}

#[test]
fn unnamed_generate_rechecks_downstream_freshness_in_program_order() {
    let mut c = three_faces();
    c.ok("edit_operation", json!({ "name": "First", "params": { "stepover": 2.0 } }));
    assert_eq!(c.statuses(), ["stale", "ok", "ok"], "downstream paths initially stay current");
    let generated = c.ok("generate", json!({}));
    assert_eq!(c.statuses(), ["ok", "ok", "ok"], "one call must leave every operation current: {generated}");
    let names: Vec<&str> = generated["generated"].as_array().expect("generated")
        .iter().map(|op| op["key"].as_str().expect("key")).collect();
    assert_eq!(names, ["First", "Second", "Third"]);
    assert_eq!(c.ok("generate", json!({}))["generated"], json!([]));
}

#[test]
fn named_generate_always_regenerates_only_the_named_operation() {
    let mut c = three_faces();
    let generated = c.ok("generate", json!({ "name": "First" }));
    assert_eq!(generated["generated"].as_array().expect("generated").len(), 1);
    assert_eq!(generated["generated"][0]["key"], json!("First"));
    c.ok("edit_operation", json!({ "name": "First", "params": { "stepover": 2.0 } }));
    c.ok("generate", json!({ "name": "First" }));
    assert_eq!(c.statuses(), ["ok", "stale", "stale"]);
}
