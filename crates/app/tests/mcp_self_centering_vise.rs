//! The MCP built-in vise tools with a 5-axis self-centering vise (#787),
//! driven in-process over the server's JSON-RPC handler: listed among the
//! sizes, fitted centred on the stock, its lip and clamping range edited
//! through the `dimensions` object, every bad input a named tool error,
//! every step one undo.
//!
//! Gate map (M4.4): covers app::mcp::vise_tools.

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
fn a_self_centering_vise_is_fitted_edited_and_undone_over_mcp() {
    let mut c = Client::new();
    let tools = c.request("tools/list", json!({}));
    let add = tools["result"]["tools"]
        .as_array()
        .and_then(|t| t.iter().find(|t| t["name"] == json!("add_builtin_vise")))
        .cloned()
        .expect("add_builtin_vise is listed");
    let sizes = add["inputSchema"]["properties"]["size"]["enum"].to_string();
    for want in ["52 mm self-centering", "75 mm self-centering", "80 mm self-centering", "125 mm self-centering", "6 in"] {
        assert!(sizes.contains(want), "{want} in {sizes}");
    }

    c.ok("set_stock", json!({ "box": { "dx": 40.0, "dy": 50.0, "dz": 20.0 } }));
    let why = c.refused("add_builtin_vise", json!({ "size": "52 mm self-centering", "fit_to_stock": true }));
    assert!(why.contains("wider than its clamping range"), "{why}");
    let placed = c.ok("add_builtin_vise", json!({ "size": "80 mm self-centering", "fit_to_stock": true }));
    assert_eq!(placed["fixture"], json!("Vise 80 mm self-centering"), "{placed}");
    assert_eq!(placed["origin"], json!([0.0, 0.0, 0.0]), "centred on the stock: {placed}");
    assert_eq!(placed["vise"]["style"], json!("self-centering"), "{placed}");
    assert_eq!(placed["vise"]["opening"], json!(50.0), "{placed}");
    assert_eq!(placed["vise"]["min_opening"], json!(0.0), "{placed}");
    let name = "Vise 80 mm self-centering";

    let why = c.refused("set_vise", json!({ "name": name, "dimensions": { "lip_height": 3.0 } }));
    assert!(why.contains("both a height and a depth"), "{why}");
    let why = c.refused("set_vise", json!({ "name": name, "dimensions": { "housing_length": 20.0 } }));
    assert!(why.contains("no screw housing"), "{why}");
    let why = c.refused("set_vise", json!({ "name": name, "opening": 200.0 }));
    assert!(why.contains("clamping range"), "{why}");
    let set = c.ok("set_vise", json!({ "name": name, "dimensions": { "lip_height": 3.0, "lip_depth": 5.0, "min_opening": 10.0 } }));
    assert_eq!(set["vise"]["lip_height"], json!(3.0), "{set}");
    assert_eq!(set["vise"]["min_opening"], json!(10.0), "{set}");
    let set = c.ok("set_vise", json!({ "name": name, "opening": 60.0 }));
    assert_eq!(set["vise"]["opening"], json!(60.0), "{set}");
    let set = c.ok("set_vise", json!({ "name": name, "from_stock": true }));
    assert_eq!(set["vise"]["opening"], json!(50.0), "closed on the stock again: {set}");
    // #1274: the grip step, not beside the dovetail lip; refused by name.
    let why = c.refused("set_vise", json!({ "name": name, "dimensions": { "grip_depth": 3.0, "grip_width": 1.5 } }));
    assert!(why.contains("a dovetail or a grip jaw"), "{why}");
    let dims = json!({ "lip_height": 0.0, "lip_depth": 0.0, "grip_depth": 3.0, "grip_width": 1.5 });
    let set = c.ok("set_vise", json!({ "name": name, "dimensions": dims }));
    assert_eq!([&set["vise"]["grip_depth"], &set["vise"]["grip_width"]], [&json!(3.0), &json!(1.5)], "{set}");
    let why = c.refused("set_vise", json!({ "name": name, "dimensions": { "grip_depth": 15.0 } }));
    assert!(why.contains("grip step depth"), "{why}");

    // A standard vise has no clamping-range floor or lip.
    c.ok("add_builtin_vise", json!({ "size": "6 in", "position": [0.0, 200.0, 0.0] }));
    let why = c.refused("set_vise", json!({ "name": "Vise 6 in", "dimensions": { "min_opening": 1.0 } }));
    assert!(why.contains("self-centering vise's"), "{why}");

    for _ in 0..6 {
        c.ok("undo", json!({}));
    }
    let f = c.ok("get_fixture_findings", json!({})).to_string();
    assert!(!f.contains("Vise 80 mm self-centering"), "six undos take the 6 in, four edits and the vise: {f}");
}
