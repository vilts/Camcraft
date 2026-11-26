//! #3469: the part-off's kept piece and back stock over MCP. `edit_operation`
//! patches `kept` and `back_stock` through `EditOperationParams` (one undo
//! step), `get_job` reads them back, and a negative back stock is refused.
//! Gate map (M4.4): covers app::mcp::edit, app::project (the Part keys).
//! Provenance: our own MCP contract over the project file keys.

use camcraft_app::mcp::Server;
use serde_json::{json, Value};

struct Client(Server);

impl Client {
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let reply = self
            .0
            .handle(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            }))
            .expect("a request gets a response");
        assert!(reply["result"].is_object(), "tool response: {reply}");
        reply["result"].clone()
    }

    fn ok(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.call(name, arguments);
        assert_ne!(result["isError"], json!(true), "{name}: {result}");
        result["structuredContent"].clone()
    }

    fn part(&mut self) -> Value {
        self.ok("get_job", json!({}))["operations"][0]["params"]["part"].clone()
    }
}

#[test]
fn edit_operation_sets_the_kept_piece_and_back_stock_and_undo_restores_them() {
    let mut c = Client(Server::new());
    c.ok("load_machine", json!({ "builtin": "Lathe" }));
    c.ok("set_stock", json!({ "box": { "dx": 34.0, "dy": 34.0, "dz": 84.0 } }));
    c.ok("add_tool", json!({ "catalog": "GT-0300" }));
    c.ok("add_operation", json!({ "kind": "Parting", "tool": "GT-0300", "name": "P1", "params": {} }));
    let p = c.part();
    assert!(p.get("kept").is_none() && p.get("back_stock").is_none(), "the default writes no key: {p}");
    c.ok("edit_operation", json!({ "name": "P1", "params": { "kept": "chuck", "back_stock": 0.5 } }));
    let p = c.part();
    assert_eq!((p["kept"].clone(), p["back_stock"].clone()), (json!("chuck"), json!(0.5)), "{p}");
    let refused = c.call("edit_operation", json!({ "name": "P1", "params": { "back_stock": -1.0 } }));
    assert_eq!(refused["isError"], json!(true), "a negative back stock is refused: {refused}");
    c.ok("undo", json!({}));
    let p = c.part();
    assert!(p.get("kept").is_none() && p.get("back_stock").is_none(), "one undo restores the default: {p}");
}
