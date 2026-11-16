//! MCP parity of the keep law (#1603, #1554): the boss job over the
//! wire, one test for each verify (#1715: the four verifies ran in
//! series in one test).

use super::*;

fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    assert_ne!(result["isError"], json!(true), "{name} refused: {}", result["content"][0]["text"]);
    result["structuredContent"].clone()
}

/// The boss job over MCP: a `kind` clearing from `source` with `area`;
/// verify's reply.
fn mcp_verify(tag: &str, kind: &str, source: &str, area: Option<Value>) -> Value {
    let dir = std::env::temp_dir().join(format!("camcraft-keep-law-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("part.stl");
    pocket_support::write_stl(&block_with_boss(), &stl);
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": stl.display().to_string() }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": SIDE, "dy": SIDE, "dz": 20.0 } }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": TOOL }));
    let params = json!({ "top_z": 20.0, "bottom_z": 5.0, "stepdown": 5.0, "stepover": 4.0, "stock_to_leave": 0.0 });
    call(&mut s, &mut id, "add_operation", json!({ "kind": kind, "tool": TOOL, "source": source, "name": "P1", "params": params }));
    if let Some(area) = area {
        call(&mut s, &mut id, "set_operation_area", json!({ "operation": "P1", "area": area }));
    }
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
        .map_or(0, |f| f.iter().filter(|x| x["kind"] == json!("gouge") && x["op"] == json!("P1") && x["blocks_post"] == json!(true)).count())
}

/// One kind over the wire: the law is the kinds' shared arm, pinned per
/// kind in the parent suite; these pin that the tools reach it.
const KIND: &str = "2D Pocket";

mod over_mcp_the_pocket_family_keeps_the_part_the_same {
    use super::*;

    #[test]
    fn an_offset_into_the_part() {
        let offset = mcp_verify("offset", KIND, "part", Some(json!({ "offset": 3.0 })));
        assert!(op_gouges(&offset) > 0, "an offset into the part is the op's own gouge: {offset}");
        assert_eq!(offset["ok"], json!(false), "{offset}");
    }

    #[test]
    fn a_stock_source() {
        // #1554: the stock source clears around the part, and verifies ok.
        let stock = mcp_verify("stock", KIND, "stock", None);
        assert_eq!(op_gouges(&stock), 0, "the stock source keeps the part: {stock}");
        assert_eq!(stock["ok"], json!(true), "a stock-source clearing around the part verifies ok: {stock}");
    }

    #[test]
    fn a_stock_source_offset_into_the_part() {
        let into = mcp_verify("stock-offset", KIND, "stock", Some(json!({ "offset": 3.0 })));
        assert!(op_gouges(&into) > 0, "a stock-source offset into the part is the op's own gouge: {into}");
        assert_eq!(into["ok"], json!(false), "{into}");
    }

    #[test]
    fn a_legal_clearing() {
        let good = mcp_verify("legal", KIND, "part", None);
        assert_eq!(op_gouges(&good), 0, "{good}");
        assert_eq!(good["ok"], json!(true), "a legal clearing verifies ok: {good}");
    }
}
