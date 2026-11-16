//! #1162: the MCP `edit_machine` tool - a merge patch over the mounted
//! sinker machine's `edm` section, one undo step, every refusal named
//! by key with nothing changed, and `get_job` reading the result back.
//!
//! Gate map (#1162): covers app::mcp::machine_tools, app::mcp::job (machine),
//! post::machine::edm_named.

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::PathBuf;

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

fn sinker() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/edm/sinker-edm.mch")
}

fn edm(c: &mut Client) -> Value {
    c.ok("get_job", json!({}))["machine"]["definition"]["edm"].clone()
}

#[test]
fn an_edm_edit_is_one_undo_step_and_get_job_reads_it_back() {
    let mut c = Client::new();
    c.ok("load_machine", json!({ "path": sinker().display().to_string() }));
    let cond = json!({ "id": "C110", "gap": 0.04, "undersize": 0.1, "role": "rough" });
    let r = c.ok("edit_machine", json!({ "patch": { "edm": {
        "max_orbit_radius": 0.5, "pickup": "chuck", "orbits": [{ "kind": "circular", "word": "OC" }], "conditions": [cond] } } }));
    assert_eq!(r["machine"]["edm"]["max_orbit_radius"], json!(0.5), "{r}");
    let e = edm(&mut c);
    assert_eq!(e["conditions"][0]["id"], json!("C110"), "{e}");
    assert_eq!(e["orbits"][0]["word"], json!("OC"));
    c.ok("undo", json!({}));
    assert!(edm(&mut c).get("conditions").is_none(), "undo takes the whole edit back");
    c.ok("redo", json!({}));
    assert_eq!(edm(&mut c)["max_orbit_radius"], json!(0.5));
    // A null removes a key.
    c.ok("edit_machine", json!({ "patch": { "edm": { "pickup": null } } }));
    assert!(edm(&mut c).get("pickup").is_none());
}

#[test]
fn a_bad_edit_is_refused_by_name_and_changes_nothing() {
    let mut c = Client::new();
    c.ok("load_machine", json!({ "path": sinker().display().to_string() }));
    c.ok("edit_machine", json!({ "patch": { "edm": { "max_orbit_radius": 0.5 } } }));
    let before = c.ok("get_job", json!({}))["machine"].clone();

    let neg = c.refused("edit_machine", json!({ "patch": { "edm": { "max_orbit_radius": -1.0 } } }));
    assert!(neg.contains("edm.max_orbit_radius"), "{neg}");
    let kind = c.refused("edit_machine", json!({ "patch": { "edm": { "orbits": [{ "kind": "spiral", "word": "X" }] } } }));
    assert!(kind.contains("edm.orbits[0].kind"), "{kind}");
    let cond = c.refused("edit_machine", json!({ "patch": { "edm": { "conditions": [{ "id": " ", "gap": 0.04 }] } } }));
    assert!(cond.contains("edm.conditions[0]"), "{cond}");
    let wire = c.refused("edit_machine", json!({ "patch": { "edm": { "max_taper_deg": 5.0 } } }));
    assert!(wire.contains("edm: states a key this machine kind"), "a wire key on a sinker: {wire}");
    let unknown = c.refused("edit_machine", json!({ "patch": { "edm": { "orbit_radius": 1.0 } } }));
    assert!(unknown.contains("unknown key 'edm.orbit_radius'"), "{unknown}");
    let top = c.refused("edit_machine", json!({ "patch": { "bogus": 1 } }));
    assert!(top.contains("unknown key 'bogus'"), "{top}");
    let same = c.refused("edit_machine", json!({ "patch": { "edm": { "max_orbit_radius": 0.5 } } }));
    assert!(same.contains("changes nothing"), "{same}");

    assert_eq!(c.ok("get_job", json!({}))["machine"], before, "nothing changed");
    // Only the one good edit is on the undo stack.
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
}

#[test]
fn edit_machine_needs_a_machine_and_a_known_setup() {
    let mut c = Client::new();
    assert!(c.refused("edit_machine", json!({ "patch": { "edm": {} } })).contains("no machine"));
    c.ok("load_machine", json!({ "path": sinker().display().to_string() }));
    assert!(c.refused("edit_machine", json!({ "patch": {}, "setup": "Nope" })).contains("no setup"));
    let tools = c.request("tools/list", json!({}))["result"]["tools"].to_string();
    assert!(tools.contains("edit_machine"));
}
