//! #3631: a 2D Pocket whose tool has no position in the region that
//! Generate cuts is accepted at edit time with a named `warning`; an
//! Outside pocket that the generator cuts has none.
//! Provenance: our own operation and its MCP contract.
//!
//! Gate map: covers app::mcp (add_operation, edit_operation, set_tool,
//! set_operation_area), app::operation::pocket_fit.

use super::Client;
use serde_json::{json, Value};

const TOO_WIDE: &str = "has no position inside the pocket";

/// An 8 x 8 mm stock, a 4 mm and a 10 mm end mill, and a stock-source
/// 2D Pocket on the 4 mm tool.
fn pocket_client() -> (Client, Value) {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 8.0, "dy": 8.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_tool", json!({ "catalog": "EM-1000-4FL" }));
    let add = c.ok("add_operation", json!({ "kind": "Pocket", "tool": "EM-0400-2FL", "name": "P1", "params": {
        "top_z": { "reference": "fixed", "offset": 10.0 }, "bottom_z": { "reference": "fixed", "offset": 6.0 }
    } }));
    (c, add)
}

fn warning(reply: &Value) -> String {
    reply["warning"].as_str().unwrap_or_default().to_owned()
}

#[test]
fn a_tool_wider_than_a_closed_inside_pocket_is_warned_and_the_generator_agrees() {
    let (mut c, add) = pocket_client();
    assert!(!warning(&add).contains(TOO_WIDE), "the 4 mm tool fits the 8 mm pocket: {add}");
    let set = c.ok("set_tool", json!({ "operation": "P1", "tool": "EM-1000-4FL" }));
    let w = warning(&set);
    assert!(w.contains(TOO_WIDE) && w.contains("'EM-1000-4FL' is 10.000 mm wide"), "{set}");
    let area = c.ok("set_operation_area", json!({ "operation": "P1", "area": { "containment": "inside" } }));
    assert!(warning(&area).contains(TOO_WIDE), "Inside is the pocket's own rule: {area}");
    let edit = c.ok("edit_operation", json!({ "name": "P1", "params": { "stock_to_leave": 0.5 } }));
    let w = warning(&edit);
    assert!(w.contains(TOO_WIDE) && w.contains("0.500 mm left on each wall"), "{edit}");
    // The warning says what Generate does: the same region refuses.
    let (refused, body) = c.call("generate", json!({ "name": "P1" }));
    assert!(refused || body.to_string().contains("narrower than the tool"), "{body}");
    assert!(body.to_string().contains("narrower than the tool"), "{body}");
}

#[test]
fn an_outside_pocket_that_the_generator_cuts_has_no_warning() {
    let (mut c, _) = pocket_client();
    c.ok("set_tool", json!({ "operation": "P1", "tool": "EM-1000-4FL" }));
    let area = c.ok("set_operation_area", json!({ "operation": "P1", "area": { "containment": "outside" } }));
    assert!(!warning(&area).contains(TOO_WIDE), "Outside cuts from the air past the stock: {area}");
    let edit = c.ok("edit_operation", json!({ "name": "P1", "params": { "feed": 900.0 } }));
    assert!(!warning(&edit).contains(TOO_WIDE), "{edit}");
    let generated = c.ok("generate", json!({ "name": "P1" }));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
}
