//! #743: MCP tests for `import_tool_holder_dxf`, `export_tool_holder_dxf`,
//! and `convert_tool_step_to_profile` (round trip, undo, and named refusals).
//! Gate map (M4.4): covers app::mcp, app::tool_profile_dxf, app::commands.
//! Provenance: original integration test over `camcraft_app::mcp::Server`.

use std::path::PathBuf;

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
        self.server.handle(&msg).expect("request receives response")
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with result: {reply}");
        let is_err = result["isError"] == json!(true);
        let body = if is_err {
            result["content"][0]["text"].clone()
        } else {
            result["structuredContent"].clone()
        };
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

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(name)
}

#[test]
fn mcp_holder_dxf_and_step_profile_tools_list_round_trip_undo_and_refuse() {
    let mut c = Client::new();
    let tools = c.request("tools/list", json!({}));
    let description = tools["result"]["tools"]
        .as_array()
        .expect("tools list")
        .iter()
        .find(|t| t["name"] == "import_tool_holder_dxf")
        .and_then(|t| t["description"].as_str())
        .expect("import description");
    for convention in [
        "parallel to Y",
        "DXF +Y is tool +Z",
        "z = 0 is the lower end of the centreline",
        "independent of draw direction",
    ] {
        assert!(
            description.contains(convention),
            "missing convention: {convention}"
        );
    }
    let listed: Vec<String> = tools["result"]["tools"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    for want in [
        "import_tool_holder_dxf",
        "export_tool_holder_dxf",
        "convert_tool_step_to_profile",
    ] {
        assert!(listed.iter().any(|n| n == want), "missing tool {want}");
    }

    let tool = "holder_mcp_tool";
    c.ok("add_tool", json!({ "name": tool, "catalog": "EM-0400-2FL", "holder": "none" }));

    // Exporting when no body is attached must refuse by tool name.
    let dxf_path = std::env::temp_dir().join(format!("camcraft_mcp_holder_{}.dxf", std::process::id()));
    let no_body_err = c.refused(
        "export_tool_holder_dxf",
        json!({ "tool": tool, "path": dxf_path }),
    );
    assert!(no_body_err.contains(tool), "{no_body_err}");

    // Unaligned STEP refuses by name without modifying the tool.
    let bad_step_err = c.refused(
        "convert_tool_step_to_profile",
        json!({ "tool": tool, "step_path": testdata("box.step") }),
    );
    assert!(!bad_step_err.is_empty());

    // Aligned ISO 13399 STEP converts to editable profile and writes DXF; undo clears it cleanly.
    let conv = c.ok(
        "convert_tool_step_to_profile",
        json!({
            "tool": tool,
            "step_path": testdata("tool_csys_box_iso13399.step"),
            "dxf_out_path": dxf_path,
        }),
    );
    assert!(conv["holder"]["points"].as_array().is_some_and(|p| p.len() >= 4));
    assert!(dxf_path.exists());

    c.ok("undo", json!({}));
    // Exporting again fails because undo restored no-holder state.
    c.refused("export_tool_holder_dxf", json!({ "tool": tool, "path": dxf_path }));

    // Re-import the exported DXF via import_tool_holder_dxf and verify undo/export round trip.
    let imp = c.ok(
        "import_tool_holder_dxf",
        json!({ "tool": tool, "path": dxf_path }),
    );
    assert!(imp["holder"]["points"].as_array().is_some_and(|p| p.len() >= 4));
    let dxf2 = std::env::temp_dir().join(format!("camcraft_mcp_holder_re_{}.dxf", std::process::id()));
    let exp = c.ok("export_tool_holder_dxf", json!({ "tool": tool, "path": dxf2 }));
    assert_eq!(exp["units"], "mm");
    assert!(exp["points"].as_u64().unwrap_or(0) >= 4);

    let _ = std::fs::remove_file(&dxf_path);
    let _ = std::fs::remove_file(&dxf2);
}
