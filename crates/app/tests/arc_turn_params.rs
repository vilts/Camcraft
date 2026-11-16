//! #1733: arc turns at corners and their radius as operation parameters
//! of 2D Pocket, 2D Adaptive and 2D Offset - set through the MCP
//! `edit_operation` door (the undoable EditOperationParams command), kept
//! by the project file, absent from a file that never set them, and
//! refused by name where no arc can be cut.
//!
//! Gate map: covers app::mcp (edit_operation, save_project, open_project,
//! undo), app::commands (EditOperationParams + History), app::operation
//! (params, sane), app::project (params, params_impl).
//!
//! Provenance: general machining knowledge (a corner blended on a tangent
//! arc no larger than the cutter radius); our own command and project-file
//! laws for the rest. No vendor material.

use serde_json::{json, Value};

use camcraft_app::mcp::Server;

const TOOL: &str = "EM-0600-4FL";

fn mcp_call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
    (is_err, body)
}

/// A server with the test part, the tool and one operation of `kind`.
fn server_with(kind: &str) -> Server {
    let model = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/Milling-test.step")
        .to_string_lossy()
        .into_owned();
    let mut s = Server::new();
    let steps = [
        ("import_model", json!({ "path": model, "name": "part" })),
        ("add_tool", json!({ "catalog": TOOL })),
        ("add_operation", json!({ "kind": kind, "tool": TOOL, "source": "part", "name": "OP" })),
    ];
    for (i, (name, args)) in steps.into_iter().enumerate() {
        let (err, body) = mcp_call(&mut s, i as i64 + 1, name, args);
        assert!(!err, "{name} refused: {body}");
    }
    s
}

/// The kind's field object out of a reply's tagged `params`.
fn fields(body: &Value) -> Value {
    body["params"].as_object().and_then(|m| m.values().next()).cloned().unwrap_or(Value::Null)
}

fn job_text(s: &mut Server, id: i64) -> String {
    let (err, body) = mcp_call(s, id, "get_job", json!({}));
    assert!(!err, "get_job refused: {body}");
    body.to_string()
}

#[test]
fn the_three_kinds_take_arc_turns_and_a_radius_through_edit_operation_and_undo() {
    for kind in ["2D Pocket", "2D Adaptive", "2D Offset"] {
        let mut s = server_with(kind);
        assert!(!job_text(&mut s, 5).contains("blend_radius"), "{kind}: a new operation sets no radius");
        let (err, body) =
            mcp_call(&mut s, 10, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": true, "blend_radius": 1.0 } }));
        assert!(!err, "{kind}: edit_operation refused arc turns: {body}");
        let f = fields(&body);
        assert_eq!(f["blend_turns"], json!(true), "{kind}: {body}");
        assert_eq!(f["blend_radius"], json!(1.0), "{kind}: {body}");

        let (err, body) = mcp_call(&mut s, 11, "undo", json!({}));
        assert!(!err, "{kind}: undo refused: {body}");
        let job = job_text(&mut s, 12);
        assert!(!job.contains("blend_radius"), "{kind}: undo restores the operation without a radius: {job}");
    }
}

#[test]
fn a_radius_no_control_can_cut_is_refused_by_name() {
    let mut s = server_with("2D Pocket");
    for bad in [json!(0.0), json!(-1.0), json!(0.001)] {
        let (err, body) =
            mcp_call(&mut s, 10, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": true, "blend_radius": bad } }));
        assert!(err, "radius {bad} is refused, not cut: {body}");
        assert!(body.to_string().contains("corner arc radius"), "the refusal names the radius: {body}");
    }
    // A nonsense radius is not stored with the option off either: a later
    // turn-on would read it.
    let (err, body) =
        mcp_call(&mut s, 20, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": false, "blend_radius": 0.0 } }));
    assert!(err, "a stored nonsense radius is refused: {body}");
    // The flag is written only when ON; turning it off is still a kept
    // edit, and a misspelled flag is still refused.
    let (err, body) = mcp_call(&mut s, 21, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": true } }));
    assert!(!err, "turn on refused: {body}");
    let (err, body) = mcp_call(&mut s, 22, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": false } }));
    assert!(!err, "turn off refused: {body}");
    assert!(fields(&body).get("blend_turns").is_none(), "the option is off again: {body}");
    let (err, body) = mcp_call(&mut s, 23, "edit_operation", json!({ "name": "OP", "params": { "blend_turn": false } }));
    assert!(err, "a misspelled flag is refused: {body}");
}

#[test]
fn arc_turns_beside_the_trochoidal_method_are_refused_by_name() {
    let mut s = server_with("2D Adaptive");
    let (err, body) = mcp_call(
        &mut s,
        10,
        "edit_operation",
        json!({ "name": "OP", "params": { "method": "trochoidal", "blend_turns": true } }),
    );
    assert!(err, "trochoidal slots have no corner to round: {body}");
    assert!(body.to_string().contains("Trochoidal"), "the refusal names the method: {body}");
}

#[test]
fn the_project_file_keeps_the_radius_and_a_default_file_has_no_key() {
    let mut s = server_with("2D Pocket");
    let file = std::env::temp_dir().join(format!("camcraft-arc-turns-{}.camproj", std::process::id()));
    let path = file.to_string_lossy().into_owned();

    let (err, body) = mcp_call(&mut s, 10, "save_project", json!({ "path": path }));
    assert!(!err, "save refused: {body}");
    let saved = std::fs::read_to_string(&file).expect("read saved project");
    assert!(!saved.contains("blend_radius") && !saved.contains("blend_turns"), "a default file states neither key");

    let (err, body) =
        mcp_call(&mut s, 11, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": true, "blend_radius": 1.5 } }));
    assert!(!err, "edit refused: {body}");
    let (err, body) = mcp_call(&mut s, 12, "save_project", json!({ "path": path }));
    assert!(!err, "save refused: {body}");
    let saved = std::fs::read_to_string(&file).expect("read saved project");
    assert!(saved.contains("\"blend_radius\": 1.5"), "the radius is in the file: {saved}");

    let mut loaded = Server::new();
    let (err, body) = mcp_call(&mut loaded, 1, "open_project", json!({ "path": path }));
    let _ = std::fs::remove_file(&file);
    assert!(!err, "open refused: {body}");
    let job = job_text(&mut loaded, 2);
    assert!(job.contains("blend_radius"), "the loaded operation keeps the radius: {job}");
}
