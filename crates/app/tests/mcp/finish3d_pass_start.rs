//! #3362: "Pass starts at" on a 3D Finish through the generic MCP
//! `edit_operation`: one undo step each way, `auto` never written, an
//! unknown word refused.
//! Provenance: the generic operation tools and the Camcraft project schema.

use super::{Client, tmp_dir, write_box_stl};
use serde_json::{Value, json};

fn finish3d(client: &mut Client) -> Value {
    client.ok("get_job", json!({}))["operations"][0].clone()
}

#[test]
fn finish3d_pass_start_uses_mcp_history_and_never_writes_auto() {
    let dir = tmp_dir("finish3d-pass-start");
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut client = Client::new();
    client.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
    client.ok("import_model", json!({ "path": stl }));
    client.ok("add_tool", json!({ "catalog": "BN-0600" }));
    let added = client.ok(
        "add_operation",
        json!({ "kind": "3D Finish", "tool": "BN-0600", "source": "block", "name": "Finish" }),
    );
    assert!(!added.to_string().contains("pass_start"), "Auto is absent: {added}");
    client.ok("edit_operation", json!({ "name": "Finish", "params": { "pattern": "contour" } }));
    let edit = client.ok("edit_operation", json!({ "name": "Finish", "params": { "pass_start": "inside" } }));
    assert_eq!(edit["params"]["finish3d"]["pass_start"], "inside", "{edit}");
    let error = client.refused("edit_operation", json!({ "name": "Finish", "params": { "pass_start": "sideways" } }));
    assert!(!error.is_empty());
    let auto = client.ok("edit_operation", json!({ "name": "Finish", "params": { "pass_start": "auto" } }));
    assert_eq!(auto["params"]["finish3d"].get("pass_start"), None, "Auto is not written: {auto}");
    client.ok("undo", json!({}));
    assert_eq!(finish3d(&mut client)["params"]["finish3d"]["pass_start"], "inside", "undo restores Inside");
    client.ok("redo", json!({}));
    assert_eq!(finish3d(&mut client)["params"]["finish3d"].get("pass_start"), None, "redo puts Auto back");
    client.ok("undo", json!({}));
    let saved = dir.join("inside.camproj");
    client.ok("save_project", json!({ "path": saved }));
    client.ok("open_project", json!({ "path": saved }));
    assert_eq!(finish3d(&mut client)["params"]["finish3d"]["pass_start"], "inside", "the file keeps Inside");
    let _ = std::fs::remove_dir_all(&dir);
}
