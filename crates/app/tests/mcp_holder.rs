//! `set_tool_holder` / `list_holder_library` over MCP (#799): a library
//! holder lands on a tool and `get_job` shows it, a drawn profile and
//! `clear` go through the same command, each call is one undo step that
//! restores the previous holder, and every bad argument is a tool error
//! naming the reason.
//!
//! Gate map (#799): covers app::mcp (holders, job, tools),
//! app::commands (SetToolBody), camcraft_tools::holder_library.

use camcraft_app::mcp::Server;

use serde_json::{json, Value};

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
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

    fn refused(&mut self, args: Value) -> String {
        let (err, body) = self.call("set_tool_holder", args);
        assert!(err, "set_tool_holder should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }

    fn holder(&mut self, tool: &str) -> Value {
        let job = self.ok("get_job", json!({}));
        let t = job["tools"].as_array().and_then(|l| l.iter().find(|t| t["name"] == json!(tool))).cloned();
        t.expect("tool listed")["holder"].clone()
    }
}

const TOOL: &str = "EM-1000-4FL";

fn client() -> Client {
    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("add_tool", json!({ "catalog": TOOL }));
    c
}

fn pts(h: &Value) -> usize {
    h["points"].as_array().map_or(0, Vec::len)
}

#[test]
fn library_entry_lands_on_the_tool_and_get_job_shows_it() {
    let mut c = client();
    assert_eq!(c.holder(TOOL), Value::Null, "no holder body to start");
    let got = c.ok("set_tool_holder", json!({ "tool": TOOL, "library": "BT30 ER20" }));
    assert_eq!(got["tool"], json!(TOOL));
    assert!(pts(&got["holder"]) >= 4, "{got}");
    let shown = c.holder(TOOL);
    assert_eq!(shown, got["holder"]);
    assert_eq!(shown["source"], json!("drawn profile"));
    // A second entry with explicit gauge length and projection replaces it.
    let again = c.ok("set_tool_holder", json!({ "tool": TOOL, "library": "bt40  er32", "gauge_len": 90.0, "projection": 40.0 }));
    assert_ne!(again["holder"], shown, "a different holder");
}

#[test]
fn undo_restores_the_previous_holder_in_one_step() {
    let mut c = client();
    let first = c.ok("set_tool_holder", json!({ "tool": TOOL, "library": "BT30 ER20" }))["holder"].clone();
    c.ok("set_tool_holder", json!({ "tool": TOOL, "library": "BT40 ER32" }));
    c.ok("undo", json!({}));
    assert_eq!(c.holder(TOOL), first, "one undo is one call");
    c.ok("undo", json!({}));
    assert_eq!(c.holder(TOOL), Value::Null);
    c.ok("redo", json!({}));
    assert_eq!(c.holder(TOOL), first);
}

#[test]
fn drawn_points_set_and_clear_restores_the_cylinder() {
    let mut c = client();
    let profile = json!([{ "z": 30.0, "r": 0.0 }, { "z": 30.0, "r": 12.0 }, { "z": 60.0, "r": 12.0 }, { "z": 60.0, "r": 0.0 }]);
    let got = c.ok("set_tool_holder", json!({ "tool": TOOL, "points": profile }));
    assert_eq!(pts(&got["holder"]), 4, "{got}");
    assert_eq!(got["holder"]["points"][1]["r"], json!(12.0));
    let cleared = c.ok("set_tool_holder", json!({ "tool": TOOL, "clear": true }));
    assert_eq!(cleared["holder"], Value::Null);
    c.ok("undo", json!({}));
    assert_eq!(pts(&c.holder(TOOL)), 4, "undo of clear brings the drawn holder back");
}

#[test]
fn list_holder_library_names_every_entry_with_dimensions() {
    let mut c = client();
    let l = c.ok("list_holder_library", json!({}));
    let entries = l["entries"].as_array().expect("entries");
    assert_eq!(l["count"], json!(entries.len()));
    assert!(entries.len() > 36, "taper and standalone chucks");
    let bt30 = entries.iter().find(|e| e["name"] == json!("BT30 ER20")).expect("BT30 ER20");
    assert_eq!(bt30["clamp"]["kind"], json!("ER"));
    assert_eq!(bt30["taper"]["name"], json!("BT30"));
    assert!(bt30["default_gauge_len"].as_f64().is_some_and(|g| g > 0.0));
    // Every listed name is one set_tool_holder knows (the shank may be refused, never the name).
    for e in entries {
        let (err, body) = c.call("set_tool_holder", json!({ "tool": TOOL, "library": e["name"] }));
        assert!(!err || !body.as_str().unwrap_or_default().contains("no holder"), "{e}: {body}");
    }
}

#[test]
fn bad_input_is_refused_by_name_and_changes_nothing() {
    let mut c = client();
    let e = c.refused(json!({ "tool": TOOL, "library": "BT99 ER20" }));
    assert!(e.contains("no holder 'BT99 ER20'"), "{e}");
    let e = c.refused(json!({ "tool": TOOL, "library": "BT30 Weldon 12 mm" }));
    assert!(e.contains("does not fit"), "a 10 mm shank in a 12 mm Weldon: {e}");
    let e = c.refused(json!({ "tool": "nope", "library": "BT30 ER20" }));
    assert!(e.contains("nope"), "{e}");
    let e = c.refused(json!({ "tool": TOOL, "library": "BT30 ER20", "points": [{ "z": 1.0, "r": 1.0 }] }));
    assert!(e.contains("exactly one"), "{e}");
    let e = c.refused(json!({ "tool": TOOL }));
    assert!(e.contains("exactly one"), "{e}");
    let e = c.refused(json!({ "tool": TOOL, "points": [] }));
    assert!(e.contains("\"points\" is empty"), "{e}");
    let e = c.refused(json!({ "tool": TOOL, "points": [{ "z": 1.0 }] }));
    assert!(e.contains("\"points\"[0] needs \"r\""), "{e}");
    let e = c.refused(json!({ "tool": TOOL, "points": [{ "z": 1.0, "r": "x" }] }));
    assert!(e.contains("points\"[0].r"), "{e}");
    let e = c.refused(json!({ "tool": TOOL, "points": [{ "z": 1.0, "r": 1.0 }] }));
    assert!(e.contains("holder profile"), "a one-point profile: {e}");
    let e = c.refused(json!({ "tool": TOOL, "library": "BT30 ER20", "gauge_len": -5.0 }));
    assert!(e.contains("\"gauge_len\""), "{e}");
    let e = c.refused(json!({ "tool": TOOL, "library": "BT30 ER20", "projection": -1.0 }));
    assert!(e.contains("\"projection\""), "{e}");
    let e = c.refused(json!({ "tool": TOOL, "library": "BT30 ER20", "gauge_len": 5.0 }));
    assert!(e.contains("shorter than the flange"), "{e}");
    let e = c.refused(json!({ "tool": TOOL, "clear": true, "gauge_len": 5.0 }));
    assert!(e.contains("go with \"library\""), "{e}");
    assert_eq!(c.holder(TOOL), Value::Null, "every refusal left the tool alone");
    assert_eq!(c.ok("get_job", json!({}))["can_undo"], json!(true), "add_tool is the only history");
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["can_undo"], json!(false), "no refusal reached the history");
}
