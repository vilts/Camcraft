//! The MCP workflow uses the same selected chains and verifies the posted job.
//! Provenance: original DXF test fixture and CamCraft's MCP protocol.

use camcraft_app::mcp::Server;
use camcraft_app::project;
use serde_json::{Value, json};

use super::{DXF, KINDS, OP, PROFILE, TOOL, TOP, params};

fn ok(server: &mut Server, tool: &str, arguments: Value) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": tool, "arguments": arguments }
    });
    let reply = server.handle(&request).expect("MCP response");
    let result = &reply["result"];
    assert_eq!(result["isError"], json!(false), "{tool}: {reply}");
    result["structuredContent"].clone()
}

#[test]
fn mcp_imports_picks_generates_verifies_and_posts_dxf_marks() {
    for kind in KINDS {
        let dir = std::env::temp_dir().join(format!("camcraft-1869-{}-{kind:?}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test directory");
        let drawing = dir.join("mark.dxf");
        let nc = dir.join("mark.nc");
        std::fs::write(&drawing, DXF).expect("write drawing");

        let mut server = Server::new();
        ok(&mut server, "set_stock", json!({ "box": { "dx": 100.0, "dy": 120.0, "dz": TOP } }));
        ok(&mut server, "add_tool", json!({ "catalog": TOOL }));
        let profile = ok(&mut server, "import_dxf", json!({ "path": drawing }));
        assert_eq!(profile["chains"], json!({ "closed": 2, "open": 3, "rejected": 0 }));
        ok(
            &mut server,
            "move_dxf_profile",
            json!({ "name": PROFILE, "position": [16.0, 8.0], "angle": 90.0 }),
        );

        let features = ok(&mut server, "list_features", json!({ "model": PROFILE }));
        let open = features["chains"]
            .as_array()
            .expect("sketch chains")
            .iter()
            .find(|c| c["layer"] == "open")
            .expect("open stroke");
        assert_eq!(open["closed"], json!(false));
        let label = kind.info().label;
        let config = project::params_to_json(&params(kind, true, 0.5, 0.5));
        let fields = config.as_object().expect("parameters").values().next().expect("mark fields");
        ok(
            &mut server,
            "add_operation",
            json!({ "kind": label, "name": OP, "tool": TOOL, "source": PROFILE, "params": fields }),
        );
        ok(
            &mut server,
            "set_operation_geometry",
            json!({ "operation": OP, "picks": [
                { "model": PROFILE, "id": open["id"] },
                { "model": PROFILE, "id": "sketch_layer:closed" }
            ] }),
        );

        let generation = ok(&mut server, "generate", json!({ "name": OP }));
        assert_eq!(generation["generated"][0]["status"], json!("ok"), "{generation}");
        let report = ok(&mut server, "verify", json!({}));
        assert_eq!(report["ok"], json!(true), "{report}");
        assert_eq!(report["findings"], json!([]), "{report}");
        let posted = ok(&mut server, "post", json!({ "output": nc }));
        assert!(posted["files"].is_array(), "{posted}");
        let program = std::fs::read_to_string(&nc).expect("posted NC");
        assert!(
            program
                .lines()
                .any(|line| line.split_whitespace().any(|word| matches!(word, "G02" | "G2"))),
            "{program}"
        );

        std::fs::remove_dir_all(&dir).expect("clean up test files");
    }
}
