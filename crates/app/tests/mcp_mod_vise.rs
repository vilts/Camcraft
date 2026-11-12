//! The MCP mod-vise tool (#820), driven in-process over the server's
//! JSON-RPC handler: a pair placed on a plate's grid and fitted to the
//! stock, its holes, slide and opening reported, one undo for both
//! blocks, and every bad input a named tool error.
//!
//! Gate map (M4.4): covers app::mcp::modvise_tools.

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
fn a_mod_vise_pair_is_placed_fitted_and_undone_over_mcp() {
    let mut c = Client::new();
    let listed = c.request("tools/list", json!({}))["result"]["tools"].as_array().cloned().unwrap_or_default();
    assert!(listed.iter().any(|t| t["name"] == json!("add_mod_vise_pair")), "listed");

    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    // A 12 in plate laid so C3's fixed block face sits on the stock's -Y
    // side: C3 is (2.5, 2.5) in from the corner; the 1 in preset's face is
    // 1.75 in in front of its anchor and its body centred 1 in across.
    let x = -25.4 - 2.5 * 25.4;
    let y = -30.0 - 1.75 * 25.4 - 2.5 * 25.4;
    let plate = c.ok("add_fixture_plate", json!({ "preset": "12 x 12 in", "position": [x, y, 0.0] }))["fixture"]
        .as_str()
        .expect("name")
        .to_owned();

    let why = c.refused("add_mod_vise_pair", json!({ "plate": plate, "hole": "C3", "size": "3 in grid" }));
    assert!(why.contains("no mod vise preset") && why.contains("1 in grid 3/8-16"), "{why}");
    let why = c.refused("add_mod_vise_pair", json!({ "plate": plate, "hole": "C3", "size": "1 in grid 3/8-16", "adjustable": { "slot_travel": 10.0 } }));
    assert!(why.contains("out of reach"), "{why}");
    let why = c.refused("add_mod_vise_pair", json!({ "plate": plate, "hole": "C3", "size": "1 in grid 3/8-16", "fixed": { "body_length": -1.0 } }));
    assert!(why.contains("body length"), "{why}");
    let why = c.refused("add_mod_vise_pair", json!({ "plate": plate, "hole": "C3", "size": "1 in grid 3/8-16", "fixed": { "wings": 1.0 } }));
    assert!(why.contains("unknown jaw block figure"), "{why}");
    let why = c.refused("add_mod_vise_pair", json!({ "plate": plate, "hole": "C3", "size": "1 in grid 3/8-16", "yaw": 30.0 }));
    assert!(why.contains("0, 90, 180 or 270"), "{why}");

    let out = c.ok("add_mod_vise_pair", json!({ "plate": plate, "hole": "C3", "size": "1 in grid 3/8-16", "insert": "soft" }));
    assert_eq!(out["fixed"]["at"], json!("C3"), "{out}");
    assert_eq!(out["adjustable"]["at"], json!("I5"), "{out}");
    assert_eq!(out["fixed"]["block"]["insert"], json!("soft"), "{out}");
    let slide = out["slide"].as_f64().unwrap_or_default();
    assert!((slide - (5.0 * 25.4 - 104.45)).abs() < 1e-6, "{out}");
    assert!((out["opening"].as_f64().unwrap_or_default() - 60.0).abs() < 1e-6, "{out}");
    let grid = c.ok("get_plate_grid", json!({ "name": plate }));
    assert_eq!(grid["carries"].as_array().map(Vec::len), Some(2), "{grid}");

    c.ok("undo", json!({}));
    let grid = c.ok("get_plate_grid", json!({ "name": plate }));
    assert_eq!(grid["carries"].as_array().map(Vec::len).unwrap_or_default(), 0, "one undo takes both: {grid}");
}

/// #832: a placed block's figures edited, the plate edit's warning
/// field, and the adjustable block re-fitted - one undo each.
#[test]
fn a_placed_pair_is_edited_and_re_fitted_over_mcp() {
    let mut c = Client::new();
    let listed = c.request("tools/list", json!({}))["result"]["tools"].as_array().cloned().unwrap_or_default();
    for want in ["set_mod_jaw", "refit_mod_vise"] {
        assert!(listed.iter().any(|t| t["name"] == json!(want)), "{want} listed");
    }
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    let x = -25.4 - 2.5 * 25.4;
    let y = -30.0 - 1.75 * 25.4 - 2.5 * 25.4;
    let plate = c.ok("add_fixture_plate", json!({ "preset": "12 x 12 in", "position": [x, y, 0.0] }))["fixture"].as_str().expect("name").to_owned();
    let out = c.ok("add_mod_vise_pair", json!({ "plate": plate, "hole": "C3", "size": "1 in grid 3/8-16" }));
    let fixed = out["fixed"]["fixture"].as_str().expect("fixed").to_owned();
    let adj = out["adjustable"]["fixture"].as_str().expect("adjustable").to_owned();
    let slide = out["slide"].as_f64().unwrap_or_default();

    let why = c.refused("set_mod_jaw", json!({ "name": plate, "figures": { "jaw_height": 20.0 } }));
    assert!(why.contains("not a mod vise jaw block"), "{why}");
    let why = c.refused("set_mod_jaw", json!({ "name": fixed, "figures": { "face_offset": 50.0, "body_length": 70.0 } }));
    assert!(why.contains("into the stock"), "{why}");
    let why = c.refused("set_mod_jaw", json!({ "name": fixed, "figures": { "wings": 1.0 } }));
    assert!(why.contains("unknown jaw block figure"), "{why}");
    let set = c.ok("set_mod_jaw", json!({ "name": fixed, "figures": { "jaw_height": 20.0 }, "insert": "soft" }));
    assert_eq!(set["block"]["jaw_height"], json!(20.0), "{set}");
    assert_eq!(set["block"]["insert"], json!("soft"), "{set}");
    assert_eq!(set.get("warning"), Some(&Value::Null), "{set}");
    // Backed off 5 mm: lawful, the gap named.
    let set = c.ok("set_mod_jaw", json!({ "name": adj, "figures": { "slide": slide - 5.0 } }));
    let w = set["warning"].as_str().unwrap_or_default();
    assert!(w.contains("adjustable jaw 5.000 mm off the stock"), "{set}");
    // The grid 2 mm toward -Y: the fixed jaw leaves the stock (a warning),
    // the adjustable one closes to 3 mm (still clear).
    // (The 12 in preset's A1 is 0.5 in from its corner.)
    let edit = c.ok("set_fixture_plate", json!({ "name": plate, "dimensions": { "offset_y": 0.5 * 25.4 - 2.0 } }));
    let w = edit["warning"].as_str().unwrap_or_default();
    assert!(w.contains(&format!("mod vise '{fixed}''s fixed jaw 2.000 mm off the stock")), "{edit}");

    let re = c.ok("refit_mod_vise", json!({ "adjustable": adj }));
    assert_eq!(re["at"], json!("I5"), "{re}");
    assert!((re["fixed_gap"].as_f64().unwrap_or_default() - 2.0).abs() < 1e-6, "the fixed jaw's gap is reported: {re}");
    assert!((re["opening"].as_f64().unwrap_or_default() - 62.0).abs() < 1e-6, "{re}");
    let why = c.refused("refit_mod_vise", json!({ "adjustable": fixed }));
    assert!(why.contains("only the adjustable jaw block"), "{why}");

    // One undo each: the re-fit, the plate edit, the two block edits.
    for _ in 0..4 {
        c.ok("undo", json!({}));
    }
    let re = c.ok("refit_mod_vise", json!({ "adjustable": adj }));
    assert!((re["slide"].as_f64().unwrap_or_default() - slide).abs() < 1e-9, "back where it was placed: {re}");
}
