//! #3356: "Pass starts at" on Face. Face cuts serpentine rows from one
//! edge of the face to the other, so it has no inside or outside pass
//! order. MCP refuses `pass_start` on a Face by that reason, never with
//! a bare "unknown field", and the Face output does not change.
//!
//! Provenance: our own synthetic fixture (a 40 x 40 x 10 box stock) and
//! the #3316 pass-order design (`docs/decisions/3316-pass-start-shared.md`);
//! no vendor material.
//!
//! Gate map (#3356): covers app::mcp (edit's pass_start refusal).
//! Lane shape: all tests here run in seconds.

use camcraft_app::mcp::Server;
use serde_json::{Value, json};

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    s.handle(&msg).expect("a response")["result"].clone()
}

/// Each word (Auto included) is refused on a Face, and the refusal names
/// why: the rows run edge to edge. The operation stays as it was.
#[test]
fn mcp_refuses_pass_start_on_face_by_the_row_order() {
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    assert!(ok(&call(&mut s, 1, "add_tool", json!({ "catalog": "EM-0600-4FL" }))));
    assert!(ok(&call(&mut s, 2, "set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": 10.0 } }))));
    let add = call(&mut s, 3, "add_operation", json!({ "kind": "Face", "tool": "EM-0600-4FL", "name": "F1" }));
    assert!(ok(&add), "{add}");
    let before = call(&mut s, 4, "get_job", json!({})).to_string();
    for (i, word) in ["outside", "inside", "auto"].into_iter().enumerate() {
        let r = call(&mut s, 5 + i as i64, "edit_operation", json!({ "name": "F1", "params": { "pass_start": word } }));
        assert_eq!(r["isError"], json!(true), "{word} is refused on a Face: {r}");
        let text = r.to_string();
        assert!(text.contains("Face has no pass order"), "the refusal names the reason for {word}: {text}");
        assert!(text.contains("edge to edge"), "the refusal says how Face rows run: {text}");
    }
    let after = call(&mut s, 9, "get_job", json!({})).to_string();
    assert_eq!(before, after, "a refused patch changes nothing");
}
