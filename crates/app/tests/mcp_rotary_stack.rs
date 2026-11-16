//! The MCP raised-rotary tools (#821), driven in-process over the
//! server's JSON-RPC handler: a rotary subplate added and pinned on a
//! plate's dowel pair, the stack read back, the setup's stated
//! nose_to_axis set and cleared - each one undo - and bad input refused
//! by name.
//!
//! Gate map (M4.4): covers app::mcp::rotary_tools, app::mcp::accessory_tools
//! (rotary_subplate), app::mcp::plate_tools (the pin law).

use camcraft_app::fixture::table_of;
use camcraft_app::machine::parse_mch;
use camcraft_app::mcp::Server;
use camcraft_app::posting::MACHINES;

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
fn a_subplate_is_pinned_on_a_plate_and_raises_the_rotary_over_mcp() {
    let mut c = Client::new();
    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    for want in ["get_rotary_stack", "set_setup_nose_to_axis"] {
        assert!(names.iter().any(|n| n == want), "missing tool {want}");
    }
    c.ok("load_machine", json!({ "builtin": "Tormach 770MX + microARC 4" }));
    c.ok("add_setup", json!({ "name": "Setup1" }));
    let plate = c.ok("add_fixture_plate", json!({ "preset": "300 x 300 mm", "on_table": true }))["fixture"].as_str().expect("name").to_owned();
    // On the table's top (#821 review: the rise is read from where it stands).
    let text = MACHINES.iter().find(|(n, _)| *n == "Tormach 770MX + microARC 4").map(|(_, t)| *t).expect("shipped");
    let top = table_of(&parse_mch(text).expect("parses")).expect("a table").max.z;
    let added = c.ok("add_accessory", json!({ "kind": "rotary_subplate", "position": [0.0, 0.0, top], "dimensions": { "bore_spacing": 100.0 } }));
    let sub = added["fixture"].as_str().expect("name").to_owned();
    assert_eq!(added["accessory"]["kind"], json!("rotary_subplate"), "{added}");
    let why = c.refused("place_on_plate", json!({ "fixture": sub, "plate": plate, "hole": "C3" }));
    assert!(why.contains("dowel pair"), "{why}");
    let why = c.refused("place_on_plate", json!({ "fixture": sub, "plate": plate, "dowels": ["C2", "C3"] }));
    assert!(why.contains("50.000 mm apart"), "{why}");
    c.ok("place_on_plate", json!({ "fixture": sub, "plate": plate, "dowels": ["C2", "C4"] }));
    let st = c.ok("get_rotary_stack", json!({}));
    let t = st["stack"]["thickness"].as_f64().unwrap_or_default();
    assert!((t - (20.0 + 19.05)).abs() < 1e-6, "{st}");
    assert_eq!(st["stack"]["layers"][1]["fixture"], json!(plate), "{st}");
    assert_eq!(st["machine_rotaries"][0]["rotary"], json!("A"), "{st}");
    assert!(st["stated_nose_to_axis"].is_null() && st["refused"].is_null(), "{st}");

    let why = c.refused("set_setup_nose_to_axis", json!({ "setup": "Setup1", "nose_to_axis": 0.0 }));
    assert!(why.contains("length over zero"), "{why}");
    let why = c.refused("set_setup_nose_to_axis", json!({ "setup": "Setup1", "nose_to_axis": "tall" }));
    assert!(why.contains("number of mm"), "{why}");
    let set = c.ok("set_setup_nose_to_axis", json!({ "setup": "Setup1", "nose_to_axis": 228.0 }));
    assert_eq!(set["stated_nose_to_axis"], json!(228.0), "{set}");
    let cleared = c.ok("set_setup_nose_to_axis", json!({ "setup": "Setup1", "nose_to_axis": null }));
    assert!(cleared["stated_nose_to_axis"].is_null(), "{cleared}");
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_rotary_stack", json!({}))["stated_nose_to_axis"], json!(228.0), "undo brings the figure back");
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    let st = c.ok("get_rotary_stack", json!({}));
    assert_eq!(st["stack"]["layers"].as_array().map(Vec::len), Some(1), "undoing the place leaves the subplate alone: {st}");
}
