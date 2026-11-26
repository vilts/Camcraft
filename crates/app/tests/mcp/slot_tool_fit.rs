//! #3475: `set_tool` refuses a tool wider than a plain Slot, by name with
//! both sizes, and changes nothing; a tool that fits is set.
//! Provenance: our own operation and its MCP contract.
//!
//! Gate map: covers app::mcp (set_tool).

use super::Client;
use serde_json::json;

#[test]
fn set_tool_refuses_a_tool_wider_than_the_slot_without_mutation() {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 50.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    for (name, width) in [("S4", 4.0), ("S6", 6.0)] {
        c.ok("add_operation", json!({
            "kind": "Slot", "tool": "EM-0400-2FL", "name": name, "params": { "width": width, "stock_to_leave": 0.0 }
        }));
    }
    let before = c.ok("get_job", json!({}));
    let msg = c.refused("set_tool", json!({ "operation": "S4", "tool": "EM-0600-4FL" }));
    assert_eq!(msg, "set_tool: invalid parameters: the tool 'EM-0600-4FL' is 6 mm wide and the slot is 4 mm wide: choose a tool of 4 mm or less");
    assert_eq!(c.ok("get_job", json!({})), before, "the refused set_tool changes nothing");
    // A tool of the slot's own width fits.
    let set = c.ok("set_tool", json!({ "operation": "S6", "tool": "EM-0600-4FL" }));
    assert_eq!(set["tool"], json!("EM-0600-4FL"), "{set}");
    // A width that the caller states is judged: 4 mm under the 6 mm tool is refused.
    let msg = c.refused("add_operation", json!({
        "kind": "Slot", "tool": "EM-0600-4FL", "name": "S7", "params": { "width": 4.0 }
    }));
    assert!(msg.contains("the slot is 4 mm wide: choose a tool of 4 mm or less"), "{msg}");
}

/// #3631: an edit of a plain Slot's width or wall stock that leaves the
/// mounted tool too wide is refused at the command with the #3475
/// sentence, and changes nothing. An edit of another field passes.
#[test]
fn edit_operation_refuses_a_slot_width_below_the_tool_without_mutation() {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 50.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    c.ok("add_operation", json!({
        "kind": "Slot", "tool": "EM-0600-4FL", "name": "S6", "params": { "width": 6.0, "stock_to_leave": 0.0 }
    }));
    let before = c.ok("get_job", json!({}));
    let msg = c.refused("edit_operation", json!({ "name": "S6", "params": { "width": 4.0 } }));
    assert_eq!(msg, "edit_operation: invalid parameters: the tool 'EM-0600-4FL' is 6 mm wide and the slot is 4 mm wide: choose a tool of 4 mm or less");
    let msg = c.refused("edit_operation", json!({ "name": "S6", "params": { "stock_to_leave": 0.5 } }));
    assert!(msg.contains("with 0.5 mm left on each wall: choose a tool of 5 mm or less"), "{msg}");
    assert_eq!(c.ok("get_job", json!({})), before, "the refused edits change nothing");
    c.ok("edit_operation", json!({ "name": "S6", "params": { "feed": 900.0 } }));
}

/// A Slot added with no width takes the larger of the seed's 4 mm and its
/// tool's diameter, so the default Slot always holds its own tool.
#[test]
fn add_operation_seeds_the_slot_width_from_the_tool() {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 50.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    c.ok("add_tool", json!({ "catalog": "EM-0300-2FL" }));
    c.ok("add_operation", json!({ "kind": "Slot", "tool": "EM-0600-4FL", "name": "S6" }));
    c.ok("add_operation", json!({ "kind": "Slot", "tool": "EM-0300-2FL", "name": "S3" }));
    let job = c.ok("get_job", json!({}));
    let width = |name: &str| {
        let ops = job["operations"].as_array().cloned().unwrap_or_default();
        let op = ops.into_iter().find(|o| o["name"] == json!(name)).unwrap_or_else(|| panic!("{name} in {job}"));
        op["params"]["slot"]["width"].clone()
    };
    assert_eq!(width("S6"), json!(6.0), "a 6 mm tool widens the seed");
    assert_eq!(width("S3"), json!(4.0), "a 3 mm tool keeps the seed's 4 mm");
}
