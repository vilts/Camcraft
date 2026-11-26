//! #3153 MCP and project pins for the 2D Chamfer tip offset.
//!
//! Gate map (#3153): covers app::mcp (list_operation_types, add_operation,
//! edit_operation, get_job, project tools) and app::project::ParamsDto.

use camcraft_app::mcp::Server;
use serde_json::{Value, json};

struct Client {
    server: Server,
    id: i64,
}

impl Client {
    fn new() -> Self {
        Self {
            server: Server::new(),
            id: 1,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.id;
        self.id += 1;
        let reply = self
            .server
            .handle(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            }))
            .expect("MCP response");
        assert_eq!(reply["id"], json!(id));
        reply
    }

    fn call(&mut self, tool: &str, args: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": tool, "arguments": args }));
        let result = &reply["result"];
        let is_error = result["isError"] == json!(true);
        let body = if is_error {
            result["content"][0]["text"].clone()
        } else {
            result["structuredContent"].clone()
        };
        (is_error, body)
    }

    fn ok(&mut self, tool: &str, args: Value) -> Value {
        let (error, body) = self.call(tool, args);
        assert!(!error, "{tool} refused: {body}");
        body
    }
}

#[test]
fn chamfer_radius_is_listed_patched_saved_and_reset_to_zero() {
    let mut client = Client::new();
    client.request(
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }),
    );
    client.ok(
        "set_stock",
        json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }),
    );
    client.ok("add_tool", json!({ "catalog": "CM-0600-90" }));

    let kinds = client.ok("list_operation_types", json!({}));
    let chamfer = kinds["kinds"]
        .as_array()
        .expect("operation kinds")
        .iter()
        .find(|kind| kind["key"] == json!("chamfer2_d"))
        .expect("2D Chamfer kind");
    assert!(
        chamfer["fields"]
            .as_array()
            .is_some_and(|fields| fields.iter().any(|field| {
                field["name"] == json!("minimum_cone_radius")
                    && field["type"] == json!("number")
                    && field["default"] == json!(0.0)
            })),
        "the zero default remains visible even when omitted from saved DTOs: {chamfer}"
    );

    let added = client.ok(
        "add_operation",
        json!({
            "kind": "2D Chamfer",
            "tool": "CM-0600-90",
            "params": { "minimum_cone_radius": 2.0 }
        }),
    );
    let name = added["operation"].as_str().expect("operation name");
    let job = client.ok("get_job", json!({}));
    assert_eq!(
        job["operations"][0]["params"]["chamfer2_d"]["minimum_cone_radius"],
        json!(2.0)
    );

    let dir = std::env::temp_dir().join(format!("camcraft-3153-mcp-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let project = dir.join("chamfer.camproj");
    client.ok(
        "save_project",
        json!({ "path": project.display().to_string() }),
    );
    let opened = client.ok(
        "open_project",
        json!({ "path": project.display().to_string() }),
    );
    assert_eq!(
        opened["operations"][0]["key"],
        json!(name),
        "the project reopen keeps the operation"
    );
    let loaded = client.ok("get_job", json!({}));
    assert_eq!(
        loaded["operations"][0]["params"]["chamfer2_d"]["minimum_cone_radius"],
        json!(2.0),
        "the positive radius survives the project round trip"
    );

    let zero = client.ok(
        "edit_operation",
        json!({
            "name": name,
            "params": { "minimum_cone_radius": 0.0 }
        }),
    );
    assert!(
        zero["params"]["chamfer2_d"]
            .get("minimum_cone_radius")
            .is_none(),
        "zero edits are accepted and serialize to the absent default: {zero}"
    );
    let job = client.ok("get_job", json!({}));
    assert!(
        job["operations"][0]["params"]["chamfer2_d"]
            .get("minimum_cone_radius")
            .is_none(),
        "the zero round trip reads as the legacy default: {job}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
