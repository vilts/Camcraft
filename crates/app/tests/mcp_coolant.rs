//! `set_operation_coolant` (#954): listed with its schema, sets an
//! operation's coolant through the undoable command (one undo step,
//! redo restores it), refuses an unknown mode or operation by name,
//! warns when the machine has no entry for the mode, and survives a
//! project save and reopen.
//!
//! Gate map (#954): covers app::mcp (coolant_tools, job, tools),
//! app::commands (EditOperationParams::with_coolant).

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

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }

    fn coolant(&mut self, op: &str) -> Value {
        let job = self.ok("get_job", json!({}));
        let ops = job["operations"].as_array().cloned().unwrap_or_default();
        ops.iter().find(|o| o["name"] == json!(op)).map_or(Value::Null, |o| o["coolant"].clone())
    }
}

/// A stock box, a 4 mm end mill and a Face operation "F1" on the stock.
fn job() -> Client {
    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_operation", json!({ "kind": "face", "tool": "EM-0400-2FL", "name": "F1" }));
    c
}

#[test]
fn the_tool_is_listed_with_its_mode_enum() {
    let mut c = job();
    let tools = c.server.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "tools/list" }))
        .and_then(|r| r["result"]["tools"].as_array().cloned())
        .unwrap_or_default();
    let t = tools.iter().find(|t| t["name"] == json!("set_operation_coolant")).expect("listed");
    let modes = &t["inputSchema"]["properties"]["coolant"]["enum"];
    assert_eq!(modes, &json!(["off", "flood", "mist", "air", "through-tool"]), "{t}");
}

#[test]
fn air_sets_in_one_undo_step_and_redo_restores_it() {
    let mut c = job();
    assert_eq!(c.coolant("F1"), json!("flood"));
    let r = c.ok("set_operation_coolant", json!({ "name": "F1", "coolant": "air" }));
    assert_eq!(r["coolant"], json!("air"), "{r}");
    assert_eq!(c.coolant("F1"), json!("air"));
    c.ok("undo", json!({}));
    assert_eq!(c.coolant("F1"), json!("flood"), "one undo step restores the previous mode");
    c.ok("redo", json!({}));
    assert_eq!(c.coolant("F1"), json!("air"));
    // through_tool, any case, reads as the kebab word.
    let r = c.ok("set_operation_coolant", json!({ "name": "F1", "coolant": "Through_Tool" }));
    assert_eq!(r["coolant"], json!("through-tool"));
    c.ok("set_operation_coolant", json!({ "name": "F1", "coolant": "off" }));
    assert_eq!(c.coolant("F1"), json!("off"));
}

#[test]
fn unknown_modes_and_operations_are_refused_by_name() {
    let mut c = job();
    let m = c.refused("set_operation_coolant", json!({ "name": "F1", "coolant": "lava" }));
    assert!(m.contains("lava") && m.contains("through-tool"), "{m}");
    let m = c.refused("set_operation_coolant", json!({ "name": "Nope", "coolant": "air" }));
    assert!(m.contains("Nope"), "{m}");
    assert!(c.refused("set_operation_coolant", json!({ "name": "F1" })).contains("coolant"));
    assert_eq!(c.coolant("F1"), json!("flood"), "a refusal changes nothing");
}

#[test]
fn a_machine_without_the_mode_warns_and_one_with_it_does_not() {
    let mut c = job();
    c.ok("load_machine", json!({ "builtin": "VMC 3-axis" }));
    let r = c.ok("set_operation_coolant", json!({ "name": "F1", "coolant": "air" }));
    assert!(r["warning"].as_str().is_some_and(|w| w.contains("air")), "{r}");
    assert_eq!(c.coolant("F1"), json!("air"), "still set; the post is the one that refuses");
    let r = c.ok("set_operation_coolant", json!({ "name": "F1", "coolant": "flood" }));
    assert!(r.get("warning").is_none(), "{r}");
    let r = c.ok("set_operation_coolant", json!({ "name": "F1", "coolant": "off" }));
    assert!(r.get("warning").is_none(), "{r}");
}

#[test]
fn the_coolant_round_trips_through_a_saved_project() {
    let mut c = job();
    c.ok("set_operation_coolant", json!({ "name": "F1", "coolant": "mist" }));
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-coolant-{}.camproj", std::process::id()));
    let path = p.display().to_string();
    c.ok("save_project", json!({ "path": path }));
    let mut d = Client { server: Server::new(), next: 1 };
    d.ok("open_project", json!({ "path": path }));
    assert_eq!(d.coolant("F1"), json!("mist"));
    let _ = std::fs::remove_file(&p);
}
