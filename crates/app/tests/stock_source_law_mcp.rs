//! #1607 over MCP: a stock-source Engrave, Trace or Face answers for
//! the part standing in the stock on the operation itself, the same as
//! through the commands (`stock_source_law`): `verify` names the
//! operation's own blocking gouge, and a legal mark verifies ok.
//!
//! Provenance: our own fixture (`pocket_support::round_pocket_block`,
//! the #1585 block) over the solid and mark laws; no vendor material
//! consulted.
//!
//! Gate map: covers app::mcp (add_operation, verify),
//! app::operation::keep_law (parts_in_stock), app::operation::mark_law
//! (budget_of).

mod pocket_support;

use camcraft_app::geometry::Mesh;
use camcraft_app::mcp::Server;
use serde_json::{json, Value};

/// A 6 mm end mill: the mark budget's cap is its 3 mm radius.
const CUTTER: &str = "EM-0600-4FL";
/// The round pocket's radius and its floor; the block is 40 x 40 x 8.
const RP: f64 = 15.0;
const FLOOR: f64 = 4.0;
const TOP: f64 = 8.0;

fn pocket_block() -> Mesh {
    pocket_support::round_pocket_block(RP, FLOOR, TOP)
}

fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    assert_ne!(result["isError"], json!(true), "{name} refused: {}", result["content"][0]["text"]);
    result["structuredContent"].clone()
}

/// The pocket block over MCP: a stock-source `kind` with `params`;
/// verify's reply.
fn mcp_verify(tag: &str, kind: &str, params: Value) -> Value {
    let dir = std::env::temp_dir().join(format!("camcraft-stock-source-law-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("part.stl");
    pocket_support::write_stl(&pocket_block(), &stl);
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": stl.display().to_string() }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": TOP } }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": CUTTER }));
    call(&mut s, &mut id, "add_operation", json!({ "kind": kind, "tool": CUTTER, "source": "stock", "name": "S1", "params": params }));
    let g = call(&mut s, &mut id, "generate", json!({}));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
    let ver = call(&mut s, &mut id, "verify", json!({}));
    let _ = std::fs::remove_dir_all(&dir);
    ver
}

/// The operation's own blocking gouge findings (not the program gate's).
fn op_gouges(ver: &Value) -> usize {
    ver["findings"]
        .as_array()
        .map_or(0, |f| f.iter().filter(|x| x["kind"] == json!("gouge") && x["op"] == json!("S1") && x["blocks_post"] == json!(true)).count())
}

#[test]
fn over_mcp_a_stock_source_cut_answers_for_the_part_the_same() {
    for kind in ["Engrave", "Trace"] {
        let deep = mcp_verify(&format!("deep-{kind}"), kind, json!({ "top_z": TOP, "bottom_z": 1.0, "stepdown": 10.0 }));
        assert!(op_gouges(&deep) > 0, "{kind}: the operation's own gouge: {deep}");
        assert_eq!(deep["ok"], json!(false), "{kind}: {deep}");
        let good = mcp_verify(&format!("legal-{kind}"), kind, json!({ "top_z": TOP, "bottom_z": 7.7, "stepdown": 10.0 }));
        assert_eq!(op_gouges(&good), 0, "{kind}: {good}");
        assert_eq!(good["ok"], json!(true), "{kind}: a legal mark off the stock verifies ok: {good}");
    }
    let face = mcp_verify("face", "Face", json!({ "cut_z": 5.0 }));
    assert!(op_gouges(&face) > 0, "a face into the part is the op's own gouge: {face}");
    assert_eq!(face["ok"], json!(false), "{face}");
}

