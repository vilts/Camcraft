//! Provenance: issue #1949 guide examples checked against CamCraft's live MCP schema.
//! Gate map: covers camcraft_app::mcp, camcraft_app::cli.

use camcraft_app::{cli, mcp};
use serde_json::{Value, json};

fn guide_raw() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"), "/../../site/content/guide/automate/ai.md"
    )).expect("the AI guide exists")
}

/// The guide text with LF line ends. A Windows checkout with
/// `core.autocrlf` has CRLF in the file, and the fence and line splits
/// below look for `\n` (#3119).
fn guide_text(raw: &str) -> String {
    raw.replace("\r\n", "\n")
}

fn guide() -> String {
    guide_text(&guide_raw())
}

fn live_tools() -> Vec<Value> {
    let reply = mcp::Server::new().handle(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}
    })).expect("tools/list response");
    reply["result"]["tools"].as_array().expect("live tools").clone()
}

/// Checks each fenced `tools/call` example against the live schema and
/// returns how many there are.
fn checked_calls(text: &str, tools: &[Value]) -> usize {
    let mut calls = 0;
    for block in text.split("```json\n").skip(1) {
        let value: Value = serde_json::from_str(block.split("```").next().expect("fence"))
            .expect("valid JSON example");
        if value["method"] != "tools/call" {
            continue;
        }
        let name = value["params"]["name"].as_str().expect("tool name");
        let tool = tools.iter().find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("unknown guide tool {name}"));
        let args = value["params"]["arguments"].as_object().expect("arguments");
        let schema = &tool["inputSchema"];
        for key in args.keys() {
            assert!(schema["properties"].get(key).is_some(), "{name}: unknown argument {key}");
        }
        for key in schema["required"].as_array().expect("required fields") {
            assert!(args.contains_key(key.as_str().expect("key")), "{name}: missing {key}");
        }
        calls += 1;
    }
    calls
}

#[test]
fn ai_guide_calls_match_live_tools_list() {
    let text = guide();
    let tools = live_tools();
    let calls = checked_calls(&text, &tools);
    assert!(calls >= 12, "the four tasks need complete tools/call examples");
    // Also check inline tool names, so prose cannot drift beyond the fenced examples.
    for word in text.split('`').skip(1).step_by(2) {
        if word.contains('_') && word.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            assert!(tools.iter().any(|tool| tool["name"] == word), "unknown inline tool {word}");
        }
    }
}

#[test]
fn ai_guide_camcraft_flags_match_parsers() {
    let text = guide();
    let mcp_help = cli::HELP_LANES.iter().find(|(name, _)| *name == "mcp")
        .expect("MCP help").1;
    for line in text.lines().filter(|line| line.starts_with("camcraft-")) {
        for flag in line.split_whitespace().filter(|word| word.starts_with("--")) {
            if line.starts_with("camcraft-cli ") {
                assert!(mcp_help.contains(flag), "undocumented CLI flag {flag}");
            } else {
                assert_eq!(flag, "--mcp-port", "unknown UI flag");
                assert_eq!(mcp::port_from(&[flag.to_owned(), "7777".to_owned()], None), Ok(Some(7777)));
            }
        }
    }
    assert!(text.contains("camcraft-ui --mcp-port 7777"));
    assert!(text.contains("camcraft-cli mcp --connect 127.0.0.1:7777"));
}

#[test]
fn a_crlf_checkout_of_the_guide_reads_the_same_examples() {
    // #3119: what a Windows checkout with `core.autocrlf` gives.
    let raw = guide_raw().replace("\r\n", "\n");
    let crlf = raw.replace('\n', "\r\n");
    assert!(crlf.contains("```json\r\n"), "the CRLF copy has CRLF fences");
    let tools = live_tools();
    let lf_calls = checked_calls(&guide_text(&raw), &tools);
    assert!(lf_calls >= 12, "the LF guide has its examples: {lf_calls}");
    assert_eq!(checked_calls(&guide_text(&crlf), &tools), lf_calls, "CRLF finds the same tools/call examples");
    assert_eq!(guide_text(&crlf), guide_text(&raw), "CRLF normalizes to the LF text");
}
