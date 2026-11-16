//! The MCP fixture plate tools (#819), driven in-process over the
//! server's JSON-RPC handler: a plate added, a vise placed at a grid
//! location and carried by a plate edit, every step one undo, every bad input a named
//! tool error.
//!
//! Gate map (M4.4): covers app::mcp::plate_tools.

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
fn a_plate_is_added_a_vise_placed_on_its_grid_and_undone_over_mcp() {
    let mut c = Client::new();
    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    for want in ["add_fixture_plate", "set_fixture_plate", "place_on_plate", "detect_plate_grid", "get_plate_grid"] {
        assert!(names.iter().any(|n| n == want), "missing tool {want}");
    }
    let why = c.refused("add_fixture_plate", json!({ "preset": "9 x 9 in" }));
    assert!(why.contains("no built-in plate") && why.contains("12 x 12 in"), "{why}");
    let why = c.refused("add_fixture_plate", json!({ "preset": "12 x 12 in", "dimensions": { "thickness": 0.0 } }));
    assert!(why.contains("thickness"), "{why}");
    let why = c.refused("add_fixture_plate", json!({ "preset": "12 x 12 in", "on_table": true }));
    assert!(why.contains("no table"), "{why}");

    let added = c.ok("add_fixture_plate", json!({ "preset": "770MX table 28 x 15 in" }));
    let plate = added["fixture"].as_str().expect("name").to_owned();
    assert_eq!(added["plate"]["grid"]["extent"], json!("rows A-O, columns 1-28"), "{added}");
    c.ok("add_builtin_vise", json!({ "size": "6 in", "position": [0.0, 0.0, 300.0] }));
    let why = c.refused("place_on_plate", json!({ "fixture": "Vise 6 in", "plate": plate, "hole": "P1" }));
    assert!(why.contains("no location P1"), "{why}");
    let why = c.refused("place_on_plate", json!({ "fixture": "Vise 6 in", "plate": plate, "hole": "A1" }));
    assert!(why.contains("off the plate"), "{why}");
    let placed = c.ok("place_on_plate", json!({ "fixture": "Vise 6 in", "plate": plate, "hole": "H10", "yaw": 90.0 }));
    assert_eq!(placed["at"], json!("H10"), "{placed}");
    let x = placed["origin"][0].as_f64().unwrap_or_default();
    assert!((x - 9.5 * 25.4).abs() < 1e-6, "{placed}");
    let grid = c.ok("get_plate_grid", json!({ "name": plate }));
    assert_eq!(grid["carries"], json!(["Vise 6 in"]), "{grid}");
    assert_eq!(grid["locations"][0], json!("A1"));

    let set = c.ok("set_fixture_plate", json!({ "name": plate, "dimensions": { "offset_x": 25.4 } }));
    assert_eq!(set["moved"], json!(["Vise 6 in"]), "{set}");
    // #832 (the #833 review): the edit's warning field is always there -
    // null when the re-seated vise and the new slab are clear (no stock,
    // no part here); a warned edit is pinned in mcp_mod_vise.
    assert_eq!(set.get("warning"), Some(&Value::Null), "{set}");
    let why = c.refused("detect_plate_grid", json!({ "name": "Vise 6 in" }));
    assert!(why.contains("no holes found"), "{why}");

    for _ in 0..4 {
        c.ok("undo", json!({}));
    }
    let why = c.refused("get_plate_grid", json!({ "name": plate }));
    assert!(why.contains(&plate), "four undos: the edit, the place, the vise, the plate: {why}");
}
