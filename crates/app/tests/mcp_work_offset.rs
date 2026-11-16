//! #931 (slice 3) over MCP: `set_work_offset` takes a 1-based `index`,
//! keeps the words G54-G59 as aliases of offsets 1-6, refuses by setup
//! name an offset the setup's own post cannot express (the command's own
//! check), and answers with `index` and `name` - `word` kept as an alias
//! of `name` for older callers. `get_job` reports the same three.
//!
//! Gate map (#931): covers app::mcp::setup_tools (set_work_offset,
//! set_wcs), app::mcp::job (setups_view), app::setup_offsets
//! (parse_index, entry_label).
//! Lane shape (M4.4): all tests here run in seconds.
//! Provenance: our own tool surface over RS-274 / ISO 6983 work-offset
//! selection; no vendor material consulted.

use camcraft_app::mcp::Server;
use serde_json::{json, Value};

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
        assert!(err, "{name} should refuse: {body}");
        body.as_str().unwrap_or_default().to_owned()
    }
}

/// One declared setup on a boxed stock, posting with the default post.
fn one_setup() -> Client {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 50.0, "dy": 40.0, "dz": 10.0 } }));
    c.ok("add_setup", json!({}));
    c
}

fn wcs(c: &mut Client) -> Value {
    c.ok("get_job", json!({}))["setups"][0]["wcs"].clone()
}

#[test]
fn set_work_offset_takes_a_one_based_index_and_answers_index_name_and_word() {
    let mut c = one_setup();
    let r = c.ok("set_work_offset", json!({ "index": 3 }));
    assert_eq!(r["setup"], json!("Setup1"), "{r}");
    assert_eq!(r["index"], json!(3), "{r}");
    assert_eq!(r["name"], json!("G56"), "the post names offset 3: {r}");
    assert_eq!(r["word"], r["name"], "word is an alias of name: {r}");
    let w = wcs(&mut c);
    assert_eq!(w["index"], json!(3), "{w}");
    assert_eq!(w["name"], json!("G56"), "{w}");
    assert_eq!(w["word"], json!("G56"), "{w}");
    // One undoable step, through the command.
    c.ok("undo", json!({}));
    let w = wcs(&mut c);
    assert_eq!(w["index"], json!(1), "undo restores offset 1: {w}");
    assert_eq!(w["name"], json!("G54"), "{w}");
}

#[test]
fn the_legacy_words_are_aliases_of_offsets_one_to_six() {
    let mut c = one_setup();
    for (word, n) in [("G54", 1), ("g55", 2), ("G59", 6)] {
        let r = c.ok("set_work_offset", json!({ "word": word }));
        assert_eq!(r["index"], json!(n), "{word}: {r}");
        if n > 1 {
            c.ok("undo", json!({}));
        }
    }
    // A word as `index` is the same alias, and agreeing aliases are one pick.
    let r = c.ok("set_work_offset", json!({ "index": "G58", "word": "G58" }));
    assert_eq!(r["index"], json!(5), "{r}");
    let e = c.refused("set_work_offset", json!({ "index": 2, "word": "G57" }));
    assert!(e.contains("different"), "{e}");
    // null clears back to offset 1.
    let r = c.ok("set_work_offset", json!({ "index": null }));
    assert_eq!(r["index"], json!(1), "{r}");
}

#[test]
fn an_offset_the_setups_post_cannot_express_is_refused_by_setup_name() {
    let mut c = one_setup();
    // The default post expresses offsets 1-6: offset 7 is the command's
    // refusal, naming the setup - never stored, never read as offset 1.
    let e = c.refused("set_work_offset", json!({ "index": 7 }));
    assert!(e.contains("setup Setup1"), "refused by setup name: {e}");
    assert!(e.contains("1-6"), "says the post's range: {e}");
    assert_eq!(wcs(&mut c)["index"], json!(1), "nothing changed");
    for bad in [json!(0), json!(-1), json!(2.5), json!(256), json!("G60"), json!(true)] {
        let e = c.refused("set_work_offset", json!({ "index": bad }));
        assert!(e.contains("G54-G59"), "{bad}: {e}");
    }
    let e = c.refused("set_work_offset", json!({}));
    assert!(e.contains("\"index\" is required"), "{e}");
}

#[test]
fn the_schema_offers_index_and_keeps_word() {
    let mut c = Client::new();
    let tools = c.request("tools/list", json!({}));
    let list = tools["result"]["tools"].as_array().cloned().unwrap_or_default();
    let t = list.iter().find(|t| t["name"] == json!("set_work_offset")).cloned().unwrap_or_default();
    let props = &t["inputSchema"]["properties"];
    assert!(props["index"].is_object(), "{t}");
    assert!(props["word"].is_object(), "{t}");
}
