//! Named posts must preserve per-operation stored script options (#2190 / PR #2206).
//! Gate map (M4.4): covers app::posting, app::posting::per_op,
//! app::setup_post_options, app::script, app::mcp.

use std::path::PathBuf;

use camcraft_app::{mcp::Server, script};
use serde_json::{json, Value};

fn call(server: &mut Server, id: &mut i64, name: &str, arguments: Value) -> Value {
    let reply = server
        .handle(&json!({
            "jsonrpc": "2.0", "id": *id, "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }))
        .expect("MCP reply");
    *id += 1;
    assert_ne!(reply["result"]["isError"], json!(true), "{name}: {reply}");
    reply["result"]["structuredContent"].clone()
}

fn fixture() -> (Server, i64) {
    let mut server = Server::new();
    let _ = server.handle(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18" }
    }));
    let mut id = 2;
    call(
        &mut server,
        &mut id,
        "load_machine",
        json!({ "builtin": "VMC 3-axis" }),
    );
    call(
        &mut server,
        &mut id,
        "set_stock",
        json!({ "box": { "dx": 60, "dy": 40, "dz": 10 } }),
    );
    call(
        &mut server,
        &mut id,
        "add_tool",
        json!({ "catalog": "EM-1000-4FL" }),
    );
    for name in ["F1", "F2"] {
        call(
            &mut server,
            &mut id,
            "add_operation",
            json!({
                "kind": "face", "tool": "EM-1000-4FL", "name": name
            }),
        );
    }
    call(&mut server, &mut id, "generate", json!({}));
    call(
        &mut server,
        &mut id,
        "set_post_options",
        json!({
            "per_op": true,
            "script_options": { "safeRetract": "G53", "showSequenceNumbers": "every-block" }
        }),
    );
    (server, id)
}

fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-2206-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test folder");
    dir
}

fn programs(result: &Value) -> Vec<String> {
    let files = result["files"]
        .as_array()
        .expect("per-operation file paths");
    assert_eq!(files.len(), 2, "both operations posted: {result}");
    files
        .iter()
        .map(|p| std::fs::read_to_string(p.as_str().expect("path")).expect("posted file"))
        .collect()
}

#[test]
fn per_op_named_post_applies_matching_options_to_every_operation() {
    let (mut server, mut id) = fixture();
    let dir = folder("matching");
    let named = call(
        &mut server,
        &mut id,
        "post",
        json!({
            "output": dir.join("named.nc"), "post": "Fanuc"
        }),
    );
    let omitted = call(
        &mut server,
        &mut id,
        "post",
        json!({ "output": dir.join("omitted.nc") }),
    );
    assert!(named.get("warning").is_none(), "{named}");
    assert!(omitted.get("warning").is_none(), "{omitted}");
    let named_programs = programs(&named);
    assert_eq!(
        named_programs,
        programs(&omitted),
        "naming the document post must preserve its options"
    );
    for program in named_programs {
        assert!(program.contains("G53 G00 Z0."), "stored retract: {program}");
        assert!(!program.contains("G28"), "no default retract: {program}");
        assert!(
            program.contains("N10 "),
            "stored sequence numbering: {program}"
        );
    }
    std::fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn per_op_different_post_reports_unapplied_options_in_result_and_log() {
    let (mut server, mut id) = fixture();
    let dir = folder("different");
    let result = call(
        &mut server,
        &mut id,
        "post",
        json!({
            "output": dir.join("haas.nc"), "post": "Haas"
        }),
    );
    let warning = result["warning"]
        .as_str()
        .expect("unapplied-options warning");
    for word in ["not applied", "Fanuc", "safeRetract", "showSequenceNumbers"] {
        assert!(warning.contains(word), "missing {word}: {warning}");
    }
    for program in programs(&result) {
        assert!(program.contains("G28"), "Haas default retract: {program}");
        assert!(
            !program.contains("G53 G00 Z0."),
            "Fanuc options must stay isolated: {program}"
        );
        assert!(
            !program.contains("N10 "),
            "Fanuc numbering must stay isolated: {program}"
        );
    }

    let project = dir.join("job.camproj");
    call(
        &mut server,
        &mut id,
        "save_project",
        json!({ "path": project }),
    );
    let out = script::run(&format!(
        "cam.load({}); cam.generate(); cam.post({{ to: {}, cps: 'Haas', program: true }})",
        json!(project),
        json!(dir.join("script.nc"))
    ));
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result["warning"], result["warning"]);
    assert!(
        out.logs.iter().any(|line| line == warning),
        "warning must reach script logs: {:?}",
        out.logs
    );
    let operations = out.result["operations"]
        .as_array()
        .expect("operation summaries");
    assert_eq!(operations.len(), 2);
    for row in operations {
        let disk =
            std::fs::read_to_string(row["path"].as_str().expect("path")).expect("script file");
        assert_eq!(
            row["program"],
            json!(disk),
            "returned text is the written program"
        );
    }
    std::fs::remove_dir_all(dir).expect("cleanup");
}
