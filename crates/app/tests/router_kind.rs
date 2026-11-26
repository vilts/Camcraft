//! The router machine kind (#2410): a file that states "router" is a mill
//! in every rule and offers the router options; a mill offers none. The
//! MCP job view reports the same list that the UI pages read.
//!
//! Gate map (M4.4): covers app::router_offer, app::mcp::job,
//! camcraft_post::machine::kind (through the app re-export).

use camcraft_app::machine::{self, parse_mch, Kind, Machine};
use camcraft_app::mcp::Server;
use camcraft_app::router_offer::{router_options_for, RouterOption};

use serde_json::{json, Value};

fn shipped(name: &str) -> Machine {
    let text = camcraft_app::posting::builtin_machine_text(name)
        .unwrap_or_else(|why| panic!("{name} is not a shipped machine: {why}"));
    parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

const ROUTERS: [&str; 6] = [
    "Tormach 24R",
    "Tormach xsTECH",
    "Tormach xsTECH Pro",
    "Sienci AltMill 2x4",
    "Sienci AltMill 4x4",
    "Sienci AltMill 4x8",
];

#[test]
fn router_word_reads_as_a_mill() {
    for w in ["router", "Router", "CNC router", "routing"] {
        assert_eq!(Kind::from_word(w), Some(Kind::Mill), "{w}");
        assert!(machine::is_router_word(w), "{w}");
    }
    assert!(!machine::is_router_word("mill"));
    assert!(!machine::is_router(None));
}

#[test]
fn shipped_routers_state_the_router_kind() {
    for name in ROUTERS {
        let m = shipped(name);
        assert_eq!(machine::kind(Some(&m)), Kind::Mill, "{name} stays a mill in every rule");
        assert!(machine::is_router(Some(&m)), "{name} states the router kind");
    }
    for name in ["VMC 3-axis", "Tormach 1100M", "Grbl 3-axis"] {
        assert!(!machine::is_router(Some(&shipped(name))), "{name} is not a router");
    }
}

#[test]
fn router_offers_the_router_options_and_a_mill_none() {
    assert_eq!(router_options_for(Some(&shipped("Tormach 24R"))), &RouterOption::ALL);
    assert_eq!(router_options_for(Some(&shipped("VMC 3-axis"))), &[] as &[RouterOption]);
    assert_eq!(router_options_for(Some(&shipped("Tormach 8L"))), &[] as &[RouterOption]);
    // No machine: nothing says the job is not a router job.
    assert_eq!(router_options_for(None), &RouterOption::ALL);
}

fn ok(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    assert!(result["isError"] != json!(true), "{name} refused: {reply}");
    result["structuredContent"].clone()
}

#[test]
fn mcp_get_job_reports_the_router_options_of_the_machine() {
    let mut s = Server::new();
    ok(&mut s, 1, "add_setup", json!({}));
    ok(&mut s, 2, "load_machine", json!({ "builtin": "Tormach 24R" }));
    let job = ok(&mut s, 3, "get_job", json!({}));
    assert_eq!(job["setups"][0]["router_options"], json!(["sheet_job", "corner_relief"]), "{job}");
    ok(&mut s, 4, "load_machine", json!({ "builtin": "VMC 3-axis" }));
    let job = ok(&mut s, 5, "get_job", json!({}));
    assert_eq!(job["setups"][0]["router_options"], json!([]), "{job}");
}
