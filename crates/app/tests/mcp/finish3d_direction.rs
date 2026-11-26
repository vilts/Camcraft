//! #2377: the actual MCP and project-file paths for a Parallel direction.
//! Provenance: the generic operation tools and the Camcraft project schema.

use super::{Client, tmp_dir, write_box_stl};
use serde_json::{Value, json};

fn finish3d(client: &mut Client) -> Value {
    client.ok("get_job", json!({}))["operations"][0].clone()
}

#[test]
fn parallel_direction_uses_mcp_history_and_survives_project_round_trip() {
    let dir = tmp_dir("finish3d-direction");
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut client = Client::new();
    client.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
    client.ok("import_model", json!({ "path": stl }));
    client.ok(
        "set_stock",
        json!({ "box": { "dx": 46.0, "dy": 46.0, "dz": 16.0 } }),
    );
    client.ok("add_tool", json!({ "catalog": "BN-0600" }));
    client.ok(
        "add_operation",
        json!({
            "kind": "3D Finish", "tool": "BN-0600", "source": "block", "name": "Finish"
        }),
    );
    client.ok("generate", json!({ "name": "Finish" }));
    assert_eq!(finish3d(&mut client)["status"], "ok");

    let legacy = dir.join("before-angle.camproj");
    client.ok("save_project", json!({ "path": legacy }));
    let mut old: Value = serde_json::from_slice(&std::fs::read(&legacy).unwrap()).unwrap();
    let entry = old["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["name"] == "Finish")
        .unwrap();
    let fields = entry["operation"]["params"]["finish3d"]
        .as_object_mut()
        .unwrap();

    // This edit is the regression on pre-angle main. It uses the generic MCP
    // tool and JSON so it builds there and fails at runtime on the lost key.
    let edit = client.ok(
        "edit_operation",
        json!({
            "name": "Finish", "params": { "direction_angle": 45.0 },
        }),
    );
    assert_eq!(edit["params"]["finish3d"]["direction_angle"], 45.0);
    assert_eq!(fields.remove("direction_angle"), Some(json!(0.0)));
    std::fs::write(&legacy, serde_json::to_vec(&old).unwrap()).unwrap();

    let kinds = client.ok("list_operation_types", json!({}));
    let fields = kinds["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["key"] == "finish3d")
        .unwrap()["fields"]
        .as_array()
        .unwrap();
    assert!(
        fields.contains(&json!({ "name": "direction_angle", "type": "number", "default": 0.0 }))
    );
    assert_eq!(finish3d(&mut client)["status"], "stale");

    for angle in [-0.001, 360.0, 720.0] {
        let error = client.refused(
            "edit_operation",
            json!({
                "name": "Finish", "params": { "direction_angle": angle },
            }),
        );
        assert!(error.contains("direction angle"), "{angle}°: {error}");
    }
    client.ok("undo", json!({}));
    let undone = finish3d(&mut client);
    assert_eq!(undone["params"]["finish3d"]["direction_angle"], 0.0);
    assert_eq!(
        undone["status"], "ok",
        "one undo restores the previous path"
    );
    client.ok("redo", json!({}));
    let redone = finish3d(&mut client);
    assert_eq!(redone["params"]["finish3d"]["direction_angle"], 45.0);
    assert_eq!(redone["status"], "stale");

    for pattern in ["scallop", "parallel"] {
        let edit = client.ok(
            "edit_operation",
            json!({
                "name": "Finish", "params": { "pattern": pattern },
            }),
        );
        assert_eq!(edit["params"]["finish3d"]["pattern"], pattern);
        assert_eq!(edit["params"]["finish3d"]["direction_angle"], 45.0);
    }

    client.ok("generate", json!({ "name": "Finish" }));
    let report = client.ok("verify", json!({ "name": "Finish" }));
    assert_eq!(report["operations"][0]["status"], "ok", "{report}");
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["kind"] != "gouge"),
        "{report}"
    );

    let angled = dir.join("angled.camproj");
    client.ok("save_project", json!({ "path": angled }));
    client.ok("open_project", json!({ "path": angled }));
    let restored = finish3d(&mut client);
    assert_eq!(restored["params"]["finish3d"]["direction_angle"], 45.0);
    assert_eq!(restored["status"], "ok");

    client.ok("open_project", json!({ "path": legacy }));
    let restored = finish3d(&mut client);
    assert_eq!(restored["params"]["finish3d"]["direction_angle"], 0.0);
    assert_eq!(
        restored["status"], "ok",
        "the old zero-angle cache stays valid"
    );

    let added = client.ok(
        "add_operation",
        json!({
            "kind": "3D Finish", "tool": "BN-0600", "source": "block", "name": "Boundary",
            "params": { "direction_angle": 359.999 }
        }),
    );
    assert_eq!(added["operation"], "Boundary");
    let _ = std::fs::remove_dir_all(&dir);
}
