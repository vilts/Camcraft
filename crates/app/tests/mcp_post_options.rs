//! #1534: MCP `get_post_options` and `set_post_options` expose explicit
//! document (`SetPostOptions`) and setup-owned (`SetSetupPostOptions`)
//! post options without implicit active-setup routing.
//!
//! Provenance: our own tests over our own MCP tools and undoable
//! commands; no vendor material consulted.
//!
//! Gate map (M4.4): covers app::mcp (post_options_tools),
//! app::commands (SetPostOptions, SetSetupPostOptions),
//! app::setup_post_options.

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
fn tools_list_includes_post_options_tools() {
    let mut c = Client::new();
    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    for want in ["get_post_options", "set_post_options"] {
        assert!(names.iter().any(|n| n == want), "missing tool {want}: {names:?}");
    }
}

#[test]
fn set_post_options_on_setup2_leaves_setup1_and_document_unchanged_and_undo_restores() {
    let mut c = Client::new();
    c.ok("add_setup", json!({ "name": "Setup1" }));
    c.ok("add_setup", json!({ "name": "Setup2" }));
    // Setup2 is now active; setting without `setup` still targets the document.
    let doc = c.ok("set_post_options", json!({ "skip_stops": true }));
    assert_eq!(doc["setup"], Value::Null);
    assert_eq!(doc["follows_document"], json!(false));
    assert_eq!(doc["own"], Value::Null);
    assert_eq!(doc["options"]["skip_stops"], json!(true));

    let s1 = c.ok("get_post_options", json!({ "setup": "Setup1" }));
    assert_eq!(s1["follows_document"], json!(true));
    assert_eq!(s1["own"], Value::Null);
    assert_eq!(s1["options"]["skip_stops"], json!(true));

    // Give Setup2 its own post options.
    let s2 = c.ok(
        "set_post_options",
        json!({ "setup": "Setup2", "skip_stops": false, "per_op": true, "end_units": "inch", "explain": true }),
    );
    assert_eq!(s2["setup"], json!("Setup2"));
    assert_eq!(s2["follows_document"], json!(false));
    assert!(s2["own"].is_object());
    assert_eq!(s2["options"]["skip_stops"], json!(false));
    assert_eq!(s2["options"]["per_op"], json!(true));
    assert_eq!(s2["options"]["end_units"], json!("inch"));
    assert_eq!(s2["options"]["explain"], json!(true));
    assert_eq!(s2["own"]["explain"], json!(true));

    // Setup1 and document resolved options are unchanged.
    let s1_after = c.ok("get_post_options", json!({ "setup": "Setup1" }));
    assert_eq!(s1_after["follows_document"], json!(true));
    assert_eq!(s1_after["options"]["skip_stops"], json!(true));
    assert_eq!(s1_after["options"]["per_op"], json!(false));
    assert_eq!(s1_after["options"]["end_units"], json!("leave"));
    assert_eq!(s1_after["options"]["explain"], json!(false));

    let doc_after = c.ok("get_post_options", json!({}));
    assert_eq!(doc_after["options"]["skip_stops"], json!(true));
    assert_eq!(doc_after["options"]["per_op"], json!(false));
    assert_eq!(doc_after["options"]["end_units"], json!("leave"));
    assert_eq!(doc_after["options"]["explain"], json!(false));

    // Undo restores Setup2 to following the document; redo brings its own back.
    c.ok("undo", json!({}));
    let s2_undone = c.ok("get_post_options", json!({ "setup": "Setup2" }));
    assert_eq!(s2_undone["follows_document"], json!(true));
    assert_eq!(s2_undone["own"], Value::Null);
    assert_eq!(s2_undone["options"]["skip_stops"], json!(true));
    assert_eq!(s2_undone["options"]["per_op"], json!(false));
    assert_eq!(s2_undone["options"]["explain"], json!(false));

    c.ok("redo", json!({}));
    let s2_redone = c.ok("get_post_options", json!({ "setup": "Setup2" }));
    assert_eq!(s2_redone["follows_document"], json!(false));
    assert_eq!(s2_redone["options"]["skip_stops"], json!(false));
    assert_eq!(s2_redone["options"]["per_op"], json!(true));
    assert_eq!(s2_redone["options"]["end_units"], json!("inch"));
    assert_eq!(s2_redone["options"]["explain"], json!(true));
}

#[test]
fn set_post_options_document_fields_and_follow_reset() {
    let mut c = Client::new();
    c.ok("add_setup", json!({ "name": "Setup1" }));
    c.ok("add_setup", json!({ "name": "Setup2" }));

    // Even with Setup2 active, omitting `setup` sets the document's options.
    let doc = c.ok(
        "set_post_options",
        json!({
            "threading": "g76",
            "smoothing": true,
            "between_setups": "optional-stop",
            "explain": true,
            "end_units": "mm"
        }),
    );
    assert_eq!(doc["setup"], Value::Null);
    assert_eq!(doc["options"]["threading"], json!("g76"));
    assert_eq!(doc["options"]["smoothing"], json!(true));
    assert_eq!(doc["options"]["between_setups"], json!("optional-stop"));
    assert_eq!(doc["options"]["explain"], json!(true));
    assert_eq!(doc["options"]["end_units"], json!("mm"));

    // Both setups follow the updated document.
    for setup in ["Setup1", "Setup2"] {
        let got = c.ok("get_post_options", json!({ "setup": setup }));
        assert_eq!(got["follows_document"], json!(true));
        assert_eq!(got["options"]["threading"], json!("g76"));
        assert_eq!(got["options"]["smoothing"], json!(true));
        assert_eq!(got["options"]["between_setups"], json!("optional-stop"));
        assert_eq!(got["options"]["explain"], json!(true));
        assert_eq!(got["options"]["end_units"], json!("mm"));
    }

    // Partial patch on Setup2 inherits the document's current options and overrides only `skip_stops`.
    let own = c.ok("set_post_options", json!({ "setup": "Setup2", "skip_stops": true }));
    assert_eq!(own["follows_document"], json!(false));
    assert_eq!(own["options"]["skip_stops"], json!(true));
    assert_eq!(own["options"]["threading"], json!("g76"));
    assert_eq!(own["options"]["smoothing"], json!(true));

    // `follow: true` returns Setup2 to following the document, and is undoable.
    let reset = c.ok("set_post_options", json!({ "setup": "Setup2", "follow": true }));
    assert_eq!(reset["follows_document"], json!(true));
    assert_eq!(reset["own"], Value::Null);
    assert_eq!(reset["options"]["skip_stops"], json!(false));
    assert_eq!(reset["options"]["threading"], json!("g76"));

    c.ok("undo", json!({}));
    let undone = c.ok("get_post_options", json!({ "setup": "Setup2" }));
    assert_eq!(undone["follows_document"], json!(false));
    assert_eq!(undone["options"]["skip_stops"], json!(true));

    // `follow: true` then `follow: false` (snapshot document options into own).
    c.ok("set_post_options", json!({ "setup": "Setup2", "follow": true }));
    let snapshot = c.ok("set_post_options", json!({ "setup": "Setup2", "follow": false }));
    assert_eq!(snapshot["follows_document"], json!(false));
    assert_eq!(snapshot["own"], snapshot["options"]);
    assert_eq!(snapshot["options"]["skip_stops"], json!(false));
    assert_eq!(snapshot["options"]["threading"], json!("g76"));
    assert!(c.refused("set_post_options", json!({ "setup": "Setup2", "follow": false })).contains("already"));
}

#[test]
fn post_options_refuses_bad_inputs_and_no_ops_by_name() {
    let mut c = Client::new();
    c.ok("add_setup", json!({ "name": "Setup1" }));

    assert!(c.refused("get_post_options", json!({ "setup": "NoSuch" })).contains("NoSuch"));
    assert!(c.refused("get_post_options", json!({ "setup": "" })).contains("setup"));
    assert!(c.refused("set_post_options", json!({ "setup": "NoSuch", "skip_stops": true })).contains("NoSuch"));
    assert!(c.refused("set_post_options", json!({ "setup": "", "skip_stops": true })).contains("setup"));

    assert!(c.refused("set_post_options", json!({})).contains("follow"));
    assert!(c.refused("set_post_options", json!({ "follow": true })).contains("setup"));
    assert!(c.refused("set_post_options", json!({ "follow": false })).contains("setup"));

    assert!(c.refused("set_post_options", json!({ "skip_stops": "yes" })).contains("skip_stops"));
    assert!(c.refused("set_post_options", json!({ "per_op": Value::Null })).contains("per_op"));
    assert!(c.refused("set_post_options", json!({ "smoothing": 1 })).contains("smoothing"));
    assert!(c.refused("set_post_options", json!({ "explain": Value::Null })).contains("explain"));
    assert!(c.refused("set_post_options", json!({ "follow": "yes", "setup": "Setup1" })).contains("follow"));
    assert!(c.refused("set_post_options", json!({ "threading": "g99" })).contains("threading"));
    assert!(c.refused("set_post_options", json!({ "end_units": "meters" })).contains("end_units"));
    assert!(c.refused("set_post_options", json!({ "between_setups": "stop" })).contains("between_setups"));
    assert!(c.refused("set_post_options", json!({ "bogus": true })).contains("unknown argument"));

    // No-op on document and already-following setup.
    assert!(c.refused("set_post_options", json!({ "skip_stops": false })).contains("already"));
    assert!(c.refused("set_post_options", json!({ "setup": "Setup1", "follow": true })).contains("already"));

    // Give Setup1 its own options, then check no-op and contradictory follow+field refusals.
    c.ok("set_post_options", json!({ "setup": "Setup1", "skip_stops": true }));
    assert!(c.refused("set_post_options", json!({ "setup": "Setup1", "skip_stops": true })).contains("already"));
    assert!(
        c.refused("set_post_options", json!({ "setup": "Setup1", "follow": true, "skip_stops": false }))
            .contains("together")
    );
}

#[test]
fn post_options_refuses_enum_types_without_applying_valid_fields() {
    let mut c = Client::new();
    c.ok("add_setup", json!({ "name": "Setup1" }));
    c.ok("set_post_options", json!({ "setup": "Setup1", "per_op": true }));
    let document = c.ok("get_post_options", json!({}));
    let setup = c.ok("get_post_options", json!({ "setup": "Setup1" }));

    for field in ["threading", "end_units", "between_setups"] {
        for invalid in [Value::Null, json!(true), json!(42), json!([]), json!({})] {
            for target in [None, Some("Setup1")] {
                let mut args = json!({ "skip_stops": true });
                args[field] = invalid.clone();
                if let Some(name) = target {
                    args["setup"] = json!(name);
                }
                let error = c.refused("set_post_options", args.clone());
                assert!(error.contains(field), "{args}: {error}");
                assert_eq!(c.ok("get_post_options", json!({})), document, "{args}");
                assert_eq!(c.ok("get_post_options", json!({ "setup": "Setup1" })), setup, "{args}");
            }
        }
    }
}
