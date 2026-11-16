//! The MCP built-in accessory tools (#789), driven in-process over the
//! server's JSON-RPC handler: a strap clamp placed and resized, parallels
//! fitted under the stock in a built-in vise, every step one undo, every
//! bad input a named tool error.
//!
//! Gate map (M4.4): covers app::mcp::accessory_tools.

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
fn accessories_are_placed_fitted_resized_and_undone_over_mcp() {
    let mut c = Client::new();
    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    for want in ["add_accessory", "set_accessory"] {
        assert!(names.iter().any(|n| n == want), "missing tool {want}");
    }

    let why = c.refused("add_accessory", json!({ "kind": "angle_plate" }));
    assert!(why.contains("no built-in accessory 'angle_plate'") && why.contains("strap_clamp"), "{why}");
    let why = c.refused("add_accessory", json!({ "kind": "block", "dimensions": { "stud_height": 3.0 } }));
    assert!(why.contains("no dimension \"stud_height\""), "{why}");
    let why = c.refused("add_accessory", json!({ "kind": "block", "steps": 3 }));
    assert!(why.contains("has no steps"), "{why}");
    let why = c.refused("add_accessory", json!({ "kind": "strap_clamp", "dimensions": { "clamp_height": 0.0 } }));
    assert!(why.contains("clamp height must be more than zero"), "{why}");

    let placed = c.ok("add_accessory", json!({ "kind": "strap_clamp", "position": [40.0, 0.0, 0.0],
        "dimensions": { "clamp_height": 20.0 } }));
    assert_eq!(placed["fixture"], json!("Strap clamp"), "{placed}");
    assert_eq!(placed["accessory"]["clamp_height"], json!(20.0), "{placed}");
    let set = c.ok("set_accessory", json!({ "name": "Strap clamp", "dimensions": { "stud_height": 50.0 } }));
    assert_eq!(set["accessory"]["stud_height"], json!(50.0), "{set}");
    let steps = c.ok("add_accessory", json!({ "kind": "step_block", "steps": 4, "position": [0.0, 200.0, 0.0] }));
    assert_eq!(steps["accessory"]["steps"], json!(4), "{steps}");

    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 50.0, "dz": 20.0 } }));
    c.ok("add_builtin_vise", json!({ "size": "6 in", "fit_to_stock": true }));
    let fit = c.ok("add_accessory", json!({ "kind": "parallels", "under_stock_in_vise": "Vise 6 in", "above": 10.0 }));
    let h = fit["accessory"]["height"].as_f64().unwrap_or_default();
    assert!((h - (1.75 * 25.4 + 10.0 - 20.0)).abs() < 1e-3, "{fit}");
    let why = c.refused("add_accessory", json!({ "kind": "block", "under_stock_in_vise": "Vise 6 in" }));
    assert!(why.contains("only parallels"), "{why}");

    for _ in 0..3 {
        c.ok("undo", json!({}));
    }
    let set = c.ok("set_accessory", json!({ "name": "Strap clamp", "dimensions": { "strap_width": 40.0 } }));
    assert_eq!(set["accessory"]["stud_height"], json!(50.0), "the resize survived three undos: {set}");
    for _ in 0..4 {
        c.ok("undo", json!({}));
    }
    let why = c.refused("set_accessory", json!({ "name": "Strap clamp" }));
    assert!(why.contains("not a built-in accessory"), "undo removed the clamp: {why}");
}

#[test]
fn lathe_chucks_are_placed_fitted_to_the_stock_and_undone_over_mcp() {
    let mut c = Client::new();
    let why = c.refused("add_accessory", json!({ "kind": "three_jaw_chuck", "fit_to_stock": true }));
    assert!(why.contains("no stock"), "{why}");
    let why = c.refused("add_accessory", json!({ "kind": "four_jaw_chuck", "dimensions": { "jaw_width": f64::NAN.to_string() } }));
    assert!(why.contains("jaw_width"), "{why}");
    let why = c.refused("add_accessory", json!({ "kind": "collet_chuck", "dimensions": { "collet_bore": 60.0 } }));
    assert!(why.contains("collet bore") && why.contains("nose diameter"), "{why}");

    c.ok("set_stock", json!({ "cylinder": { "diameter": 30.0, "height": 40.0 } }));
    let placed = c.ok("add_accessory", json!({ "kind": "three_jaw_chuck", "fit_to_stock": true }));
    assert_eq!(placed["fixture"], json!("3-jaw chuck"), "{placed}");
    assert_eq!(placed["accessory"]["grip_diameter"], json!(30.0), "{placed}");
    let why = c.refused("add_accessory", json!({ "kind": "block", "fit_to_stock": true }));
    assert!(why.contains("only a lathe chuck"), "{why}");
    let set = c.ok("set_accessory", json!({ "name": "3-jaw chuck", "dimensions": { "jaw_height": 12.0 } }));
    assert_eq!(set["accessory"]["jaw_height"], json!(12.0), "{set}");
    let collet = c.ok("add_accessory", json!({ "kind": "collet_chuck", "position": [200.0, 0.0, 0.0] }));
    assert_eq!(collet["accessory"]["collet_bore"], json!(20.0), "{collet}");
    for _ in 0..3 {
        c.ok("undo", json!({}));
    }
    let why = c.refused("set_accessory", json!({ "name": "3-jaw chuck" }));
    assert!(why.contains("not a built-in accessory"), "undo removed the chuck: {why}");
}
