//! The MCP tool-body tools (#361), driven in-process over the server's
//! JSON-RPC handler: a milling body from a STEP by its coordinate
//! systems or by numbers, a lathe holder in the turning tool frame with
//! its insert zone, read back, cleared, every step one undo, every bad
//! input a named tool error.
//!
//! Gate map (M4.4): covers app::mcp::body_tools.
//!
//! Provenance: our own fixtures; no vendor file consulted.

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

fn testdata(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name).display().to_string()
}

fn near(v: &Value, want: f64) -> bool {
    v.as_f64().is_some_and(|g| (g - want).abs() < 1e-6)
}

const MILL: &str = "EM-0600-4FL";
const INSERT: &str = "TN-1600-0R8";

#[test]
fn the_body_tools_are_listed_with_their_required_arguments() {
    let mut c = Client::new();
    let list = c.request("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().expect("tools array");
    let required = |name: &str| tools.iter().find(|t| t["name"] == json!(name)).map(|t| t["inputSchema"]["required"].clone());
    assert_eq!(required("get_tool_body"), Some(json!(["tool"])));
    assert_eq!(required("set_tool_body"), Some(json!(["tool", "path"])));
    assert_eq!(required("set_lathe_tool_body"), Some(json!(["tool", "path", "radial", "along"])));
    assert_eq!(required("set_lathe_insert_zone"), Some(json!(["tool", "insert_zone"])));
    assert_eq!(required("clear_tool_body"), Some(json!(["tool"])));
}

#[test]
fn a_milling_body_lands_by_coordinate_systems_or_numbers_and_undoes_over_mcp() {
    let mut c = Client::new();
    c.ok("add_tool", json!({ "catalog": MILL, "holder": "none" }));
    assert_eq!(c.ok("get_tool_body", json!({ "tool": MILL })), json!({ "tool": MILL, "body": null }));

    // A STEP naming ISO 13399 coordinate systems places itself.
    let auto = c.ok("set_tool_body", json!({ "tool": MILL, "path": testdata("tool_csys_box_iso13399.step") }));
    assert_eq!(auto["placed_by"], json!("ISO 13399 coordinate systems"), "{auto}");
    let b = &auto["body"];
    assert_eq!((&b["frame"], &b["source"], &b["drawn"]), (&json!("spindle"), &json!("tool_csys_box_iso13399.step"), &json!(false)), "{auto}");
    assert!(near(&b["tip_offset"], 75.0) && b["axis"] == json!([0.0, 0.0, 1.0]), "{auto}");
    assert!(b["facets"].as_u64().is_some_and(|n| n >= 12) && b["top"].as_f64() > b["foot"].as_f64(), "{auto}");
    assert_eq!(c.ok("get_tool_body", json!({ "tool": MILL }))["body"], *b, "get reads what set landed");

    // One undo takes it off; redo puts it back.
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_tool_body", json!({ "tool": MILL }))["body"], Value::Null);
    c.ok("redo", json!({}));
    assert_eq!(c.ok("get_tool_body", json!({ "tool": MILL }))["body"], *b);

    // By numbers: the same box stood 15 lower than its coordinate
    // systems put it.
    let (foot, top) = (b["foot"].as_f64().expect("foot"), b["top"].as_f64().expect("top"));
    let numeric = c.ok("set_tool_body", json!({ "tool": MILL, "path": testdata("tool_csys_box_iso13399.step"), "axis": [0, 0, 1], "tip_offset": 60 }));
    assert_eq!(numeric["placed_by"], json!("numeric"));
    assert!(near(&numeric["body"]["foot"], foot - 15.0) && near(&numeric["body"]["top"], top - 15.0), "{numeric}");
    assert!(near(&numeric["body"]["tip_offset"], 60.0), "{numeric}");

    // Cleared by command: the holder cylinder again, one undo back.
    assert_eq!(c.ok("clear_tool_body", json!({ "tool": MILL }))["body"], Value::Null);
    c.ok("undo", json!({}));
    assert!(near(&c.ok("get_tool_body", json!({ "tool": MILL }))["body"]["tip_offset"], 60.0));
}

#[test]
fn a_milling_body_is_refused_by_name_over_mcp() {
    let mut c = Client::new();
    c.ok("add_tool", json!({ "catalog": MILL, "holder": "none" }));
    c.ok("add_tool", json!({ "catalog": INSERT }));
    let boxed = testdata("tool_csys_box_iso13399.step");
    let e = c.refused("set_tool_body", json!({ "tool": MILL, "path": testdata("box.step") }));
    assert!(e.contains("the file names no coordinate systems"), "{e}");
    let e = c.refused("set_tool_body", json!({ "tool": MILL, "path": boxed, "axis": [0, 0, 1] }));
    assert!(e.contains("give both \"axis\" and \"tip_offset\""), "{e}");
    let e = c.refused("set_tool_body", json!({ "tool": MILL, "path": boxed, "axis": [0, 0], "tip_offset": 60 }));
    assert!(e.contains("\"axis\" must be [x, y, z] numbers"), "{e}");
    let e = c.refused("set_tool_body", json!({ "tool": MILL, "path": boxed, "axis": [0, 0, 0], "tip_offset": 60 }));
    assert!(e.contains("the tool axis direction must be a finite, non-zero vector"), "{e}");
    // A cube with a corner on the axis is no holder about it.
    let e = c.refused("set_tool_body", json!({ "tool": MILL, "path": testdata("box.step"), "axis": [0, 0, 1], "tip_offset": 60 }));
    assert!(e.contains("does not surround the tool axis"), "{e}");
    let e = c.refused("set_tool_body", json!({ "tool": MILL, "path": boxed, "axis": [0, 0, 1], "tip_offset": 2 }));
    assert!(e.contains("reaches below the tool tip"), "{e}");
    let e = c.refused("set_tool_body", json!({ "tool": INSERT, "path": boxed, "axis": [0, 0, 1], "tip_offset": 60 }));
    assert!(e.contains("lathe-held"), "{e}");
    let e = c.refused("set_tool_body", json!({ "tool": MILL, "path": testdata("no-such.step"), "axis": [0, 0, 1], "tip_offset": 60 }));
    assert!(e.starts_with("set_tool_body:"), "{e}");
    let e = c.refused("set_tool_body", json!({ "tool": "nobody", "path": boxed, "axis": [0, 0, 1], "tip_offset": 60 }));
    assert!(e.contains("nobody"), "{e}");
    let e = c.refused("get_tool_body", json!({ "tool": "nobody" }));
    assert!(e.contains("no tool named 'nobody'"), "{e}");
    let e = c.refused("set_tool_body", json!({ "tool": MILL, "path": boxed, "offset": 60 }));
    assert!(e.contains("unknown argument \"offset\""), "{e}");
    for tool in [MILL, INSERT] {
        assert_eq!(c.ok("get_tool_body", json!({ "tool": tool }))["body"], Value::Null, "a refusal lands nothing");
    }
}

#[test]
fn a_lathe_holder_lands_in_the_turning_frame_with_its_insert_zone_over_mcp() {
    let mut c = Client::new();
    c.ok("add_tool", json!({ "catalog": INSERT }));
    let args = json!({ "tool": INSERT, "path": testdata("lathe_holder_offset.step"), "radial": [0, 1, 0], "along": [1, 0, 0], "tip": [5, 10, 3] });
    let set = c.ok("set_lathe_tool_body", args.clone());
    let b = &set["body"];
    assert_eq!((&b["frame"], &b["source"], &b["insert_zone"]), (&json!("lathe"), &json!("lathe_holder_offset.step"), &Value::Null), "{set}");
    assert_eq!((&b["radial"], &b["along"], &b["tip"]), (&json!([0.0, 1.0, 0.0]), &json!([1.0, 0.0, 0.0]), &json!([5.0, 10.0, 3.0])), "{set}");
    // The offset holder as the turning frame sees it: an L, all of it
    // on the chuck side of the tip.
    let x = &b["extent"];
    assert!(near(&x["radial"][0], 2.0) && near(&x["radial"][1], 120.0), "{set}");
    assert!(near(&x["normal"][0], -10.0) && near(&x["normal"][1], 10.0), "{set}");
    assert!(near(&x["spindle"][0], -26.0) && near(&x["spindle"][1], -1.0), "{set}");

    // The insert zone: set, read back, cleared - each one undo step.
    let zone = json!({ "lo": [0, -6, -4], "hi": [5, 6, 0] });
    let zoned = c.ok("set_lathe_insert_zone", json!({ "tool": INSERT, "insert_zone": zone }));
    assert_eq!(zoned["body"]["insert_zone"], json!({ "lo": [0.0, -6.0, -4.0], "hi": [5.0, 6.0, 0.0] }), "{zoned}");
    assert_eq!(zoned["body"]["extent"], *x, "the zone keeps the mesh and its placement");
    assert_eq!(c.ok("set_lathe_insert_zone", json!({ "tool": INSERT, "insert_zone": null }))["body"]["insert_zone"], Value::Null);
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_tool_body", json!({ "tool": INSERT }))["body"]["insert_zone"]["hi"], json!([5.0, 6.0, 0.0]));
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_tool_body", json!({ "tool": INSERT }))["body"]["insert_zone"], Value::Null);
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_tool_body", json!({ "tool": INSERT }))["body"], Value::Null, "the third undo takes the body off");

    // Body and zone in one call.
    let mut both = args;
    both["insert_zone"] = zone;
    assert_eq!(c.ok("set_lathe_tool_body", both)["body"]["insert_zone"]["lo"], json!([0.0, -6.0, -4.0]));
    assert_eq!(c.ok("clear_tool_body", json!({ "tool": INSERT }))["body"], Value::Null);
}

#[test]
fn a_lathe_holder_is_refused_by_name_over_mcp() {
    let mut c = Client::new();
    c.ok("add_tool", json!({ "catalog": MILL, "holder": "none" }));
    c.ok("add_tool", json!({ "catalog": INSERT }));
    let path = testdata("lathe_holder_offset.step");
    let with = |patch: Value| {
        let mut a = json!({ "tool": INSERT, "path": path, "radial": [0, 1, 0], "along": [1, 0, 0], "tip": [5, 10, 3] });
        for (k, v) in patch.as_object().into_iter().flatten() {
            a[k] = v.clone();
        }
        a
    };
    let e = c.refused("set_lathe_tool_body", with(json!({ "tool": MILL })));
    assert!(e.contains("is no lathe steel"), "{e}");
    let e = c.refused("set_lathe_tool_body", json!({ "tool": INSERT, "path": path, "along": [1, 0, 0] }));
    assert!(e.contains("\"radial\" is required"), "{e}");
    let e = c.refused("set_lathe_tool_body", with(json!({ "along": [1, 0.2, 0] })));
    assert!(e.contains("not square"), "{e}");
    let e = c.refused("set_lathe_tool_body", with(json!({ "radial": [0, 0, 0] })));
    assert!(e.contains("finite, non-zero vectors"), "{e}");
    let e = c.refused("set_lathe_tool_body", with(json!({ "tip": [5, -200, 3] })));
    assert!(e.contains("the tip stands more than 50.8 mm from the lathe body"), "{e}");
    let e = c.refused("set_lathe_tool_body", with(json!({ "insert_zone": { "lo": [0, -6, -4], "hi": [30, 6, 0] } })));
    assert!(e.contains("the insert zone reaches 30.000 mm from the tip along x"), "{e}");
    let e = c.refused("set_lathe_tool_body", with(json!({ "insert_zone": { "lo": [1, -6, -4], "hi": [5, 6, 0] } })));
    assert!(e.contains("the insert zone must hold the tip"), "{e}");
    let e = c.refused("set_lathe_tool_body", with(json!({ "insert_zone": { "lo": [0, -6, -4] } })));
    assert!(e.contains("\"insert_zone\" must be"), "{e}");
    let e = c.refused("set_lathe_tool_body", with(json!({ "path": "no-such.step" })));
    assert!(e.starts_with("set_lathe_tool_body:"), "{e}");
    assert_eq!(c.ok("get_tool_body", json!({ "tool": INSERT }))["body"], Value::Null, "a refusal lands nothing");
    // A zone needs a lathe body to belong to.
    let e = c.refused("set_lathe_insert_zone", json!({ "tool": INSERT, "insert_zone": null }));
    assert!(e.contains("it carries no lathe body"), "{e}");
    let e = c.refused("set_lathe_insert_zone", json!({ "tool": INSERT }));
    assert!(e.contains("\"insert_zone\" is required"), "{e}");
}
