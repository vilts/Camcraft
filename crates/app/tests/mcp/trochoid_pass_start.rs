//! #3358: Trochoidal Slot refuses a pass order because its loops advance
//! from the entry bore through stock cleared by earlier cuts.
//! Provenance: our own operation and its MCP patch contract.
//!
//! Gate map: covers app::mcp (add_operation, edit_operation).

use super::Client;
use serde_json::{Value, json};

#[test]
fn trochoidal_slot_refuses_pass_start_on_add_and_edit_without_mutation() {
    let mut c = Client::new();
    c.ok(
        "set_stock",
        json!({ "box": { "dx": 60.0, "dy": 50.0, "dz": 10.0 } }),
    );
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    c.ok(
        "add_operation",
        json!({ "kind": "Trochoidal Slot", "tool": "EM-0600-4FL", "name": "T1" }),
    );
    let before = c.ok("get_job", json!({}));
    let params = &before["operations"][0]["params"]["trochoidal_slot"];
    assert!(
        params.is_object() && params.get("pass_start").is_none(),
        "{before}"
    );

    for value in [
        json!("auto"),
        json!("inside"),
        json!("outside"),
        Value::Null,
    ] {
        let add_args = json!({
            "kind": "Trochoidal Slot", "tool": "EM-0600-4FL", "name": "T2", "params": { "pass_start": value }
        });
        let edit_args = json!({ "name": "T1", "params": { "pass_start": value, "width": 18.0 } });
        for (tool, args) in [("add_operation", add_args), ("edit_operation", edit_args)] {
            let message = c.refused(tool, args);
            assert!(
                message.contains("pass_start")
                    && message.contains("Trochoidal Slot has no pass order")
                    && message.contains("entry bore")
                    && message.contains("advances along the slot")
                    && message.contains("clearance from the previous cut"),
                "{tool} with {value}: {message}"
            );
            assert_eq!(
                c.ok("get_job", json!({})),
                before,
                "{tool} changed the job for {value}"
            );
        }
    }
    c.ok("undo", json!({}));
    let after_undo = c.ok("get_job", json!({}));
    assert_eq!(
        after_undo["operations"],
        json!([]),
        "no rejected patch entered the history"
    );
}
