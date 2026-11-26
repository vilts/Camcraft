//! #2374, the MCP leg: a machine file that names its post makes the MCP
//! `load_machine` reply and `get_job` say so, for the setup the machine
//! went to - the agent's counterpart of the Setup page's banner. A mount
//! never changes the post by itself: the agent takes the offer with
//! `set_setup_post`, one undo step. The `post` tool lists every registry
//! post, so an agent learns "Tormach PathPilot" from the tool list.
//! Provenance: our own fixture over our own tools; no vendor material.
//!
//! Gate map (#2374): covers app::mcp::setup_tools (load_machine,
//! set_setup_post), app::mcp::job (get_job), app::mcp::tools (post
//! schema), app::posting (post_offer_for).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::mcp::Server;
use camcraft_app::posting::BUILTINS;
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

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
        assert!(result["isError"] != json!(true), "{name} refused: {reply}");
        result["structuredContent"].clone()
    }

    /// `get_job`'s entry for the setup named `name`.
    fn job_setup(&mut self, name: &str) -> Value {
        let job = self.ok("get_job", json!({}));
        let setups = job["setups"].as_array().cloned().unwrap_or_default();
        setups.into_iter().find(|s| s["name"] == json!(name)).unwrap_or_else(|| panic!("no {name} in {job}"))
    }
}

#[test]
fn mounting_the_770mx_offers_pathpilot_and_the_agent_takes_it_as_one_undo_step() {
    let mut c = Client::new();
    c.ok("add_setup", json!({}));
    let mounted = c.ok("load_machine", json!({ "builtin": "Tormach 770MX", "setup": "Setup1" }));
    assert_eq!(mounted["post_offer"]["post"], json!("Tormach PathPilot"), "{mounted}");
    assert_eq!(mounted["post_offer"]["why"], json!("the machine names this post"), "{mounted}");
    assert!(mounted["post_offer"]["hint"].as_str().is_some_and(|h| h.contains("set_setup_post")), "{mounted}");

    // The mount changed nothing: the setup still posts with Fanuc, and the job says why it should not.
    let s = c.job_setup("Setup1");
    assert_eq!(s["posts_with"], json!("Fanuc (built-in)"), "{s}");
    assert_eq!(s["post_offer"]["post"], json!("Tormach PathPilot"), "{s}");

    c.ok("set_setup_post", json!({ "setup": "Setup1", "post": "Tormach PathPilot" }));
    let s = c.job_setup("Setup1");
    assert_eq!(s["posts_with"], json!("Tormach PathPilot (built-in)"), "{s}");
    assert_eq!(s["post_offer"], Value::Null, "taken: no offer left: {s}");

    c.ok("undo", json!({}));
    let s = c.job_setup("Setup1");
    assert_eq!(s["posts_with"], json!("Fanuc (built-in)"), "{s}");
    assert_eq!(s["post_offer"]["post"], json!("Tormach PathPilot"), "undo brings the offer back: {s}");
}

#[test]
fn the_offer_follows_the_setup_the_machine_went_to_not_the_active_one() {
    let mut c = Client::new();
    c.ok("add_setup", json!({}));
    c.ok("add_setup", json!({}));
    // add_setup makes the new one active: Setup2 is active, Setup1 is not.
    let mounted = c.ok("load_machine", json!({ "builtin": "grblHAL 3-axis", "setup": "Setup1" }));
    assert_eq!(mounted["post_offer"]["post"], json!("grblHAL"), "{mounted}");
    assert_eq!(c.job_setup("Setup1")["post_offer"]["post"], json!("grblHAL"));
    assert_eq!(c.job_setup("Setup2")["post_offer"], Value::Null, "Setup2 is still on the VMC default");
}

#[test]
fn a_machine_that_names_no_post_and_serves_the_current_one_offers_nothing() {
    let mut c = Client::new();
    c.ok("add_setup", json!({}));
    let mounted = c.ok("load_machine", json!({ "builtin": "VMC 3-axis", "setup": "Setup1" }));
    assert_eq!(mounted["post_offer"], Value::Null, "{mounted}");
    assert_eq!(c.job_setup("Setup1")["post_offer"], Value::Null);
}

#[test]
fn the_post_tool_lists_every_registry_post() {
    let mut c = Client::new();
    let list = c.request("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().cloned().unwrap_or_default();
    let post = tools.iter().find(|t| t["name"] == json!("post")).expect("the post tool is listed");
    let said = post["inputSchema"]["properties"]["post"]["description"].as_str().unwrap_or_default();
    for (name, _) in BUILTINS {
        assert!(said.contains(name), "the post tool does not name {name}: {said}");
    }
}
