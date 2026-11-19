//! Provenance: our MCP contract over the undoable DeleteEntity command (#2444).
//! Gate map: covers app::mcp::model_tools, app::commands::delete_entity.

use camcraft_app::mcp::Server;
use serde_json::{json, Value};

struct Client {
    server: Server,
    next: i64,
}
impl Client {
    fn new() -> Self {
        Self {
            server: Server::new(),
            next: 0,
        }
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        self.server
            .handle(
                &json!({ "jsonrpc": "2.0", "id": self.next, "method": method, "params": params }),
            )
            .expect("response")
    }
    fn call(&mut self, name: &str, args: Value) -> Value {
        self.request("tools/call", json!({ "name": name, "arguments": args }))["result"].clone()
    }
    fn ok(&mut self, name: &str, args: Value) -> Value {
        let r = self.call(name, args);
        assert_eq!(r["isError"], false, "{name}: {r}");
        r["structuredContent"].clone()
    }
    fn refused(&mut self, args: Value, words: &[&str]) {
        let before = self.job();
        let r = self.call("delete_model", args);
        assert_eq!(r["isError"], true, "{r}");
        let text = r["content"][0]["text"].as_str().expect("error text");
        assert!(
            text.starts_with("delete_model:"),
            "handler must refuse: {text}"
        );
        for word in words {
            assert!(text.contains(word), "missing {word}: {text}");
        }
        assert_eq!(self.job(), before, "refusal changes nothing");
    }
    fn job(&mut self) -> Value {
        let mut job = self.ok("get_job", json!({}));
        let fields = job.as_object_mut().expect("job");
        fields.remove("can_undo");
        fields.remove("can_redo");
        job
    }
    fn imported(tag: &str) -> Self {
        let mut c = Self::new();
        let path = std::env::temp_dir().join(format!(
            "camcraft-delete-model-{tag}-{}.obj",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "v 0 0 0\nv 10 0 0\nv 0 10 0\nv 0 0 10\nf 1 3 2\nf 1 2 4\nf 2 3 4\nf 3 1 4\n",
        )
        .expect("OBJ");
        c.ok("import_model", json!({ "path": path, "name": "part" }));
        std::fs::remove_file(path).expect("remove fixture file");
        c
    }
    fn op(&mut self, source: &str) {
        self.ok(
            "set_stock",
            json!({ "box": { "dx": 30, "dy": 30, "dz": 20 } }),
        );
        self.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
        self.ok(
            "add_operation",
            json!({ "kind": "Face", "tool": "EM-0400-2FL", "name": "F1", "source": source }),
        );
    }
    fn pick(&mut self) -> Value {
        let f = self.ok("list_features", json!({ "model": "part" }));
        json!({ "model": "part", "id": f["faces"][0]["id"] })
    }
}

#[test]
fn delete_model_is_listed_and_round_trips_in_one_undo_step() {
    let mut c = Client::imported("roundtrip");
    let rows = c.request("tools/list", json!({}));
    let row = rows["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|r| r["name"] == "delete_model")
        .expect("delete_model is listed");
    assert_eq!(row["inputSchema"]["required"], json!(["name"]));
    let before = c.job();
    assert_eq!(
        c.ok("delete_model", json!({ "name": "part" }))["deleted"],
        "part"
    );
    let deleted = c.job();
    assert!(deleted["models"].as_array().expect("models").is_empty());
    c.ok("undo", json!({}));
    assert_eq!(c.job(), before);
    c.ok("redo", json!({}));
    assert_eq!(c.job(), deleted);
}

#[test]
fn delete_model_refuses_unknown_non_model_fixture_and_bad_arguments() {
    let mut c = Client::imported("bad");
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.refused(json!({ "name": "ghost" }), &["ghost", "no model"]);
    c.refused(
        json!({ "name": "EM-0400-2FL" }),
        &["EM-0400-2FL", "not a model"],
    );
    for args in [
        json!({}),
        json!({"name": 9}),
        json!({"name": ""}),
        json!({"name": "part", "typo": true}),
    ] {
        c.refused(args, &[]);
    }
    c.ok("add_fixture", json!({ "model": "part" }));
    c.refused(json!({ "name": "part" }), &["part", "fixture"]);
    c.ok("undo", json!({}));
    assert_eq!(
        c.job()["models"][0]["fixture"],
        false,
        "undo still removes the fixture mark"
    );
}

#[test]
fn delete_model_refuses_an_operation_source_without_history() {
    let mut c = Client::imported("source");
    c.op("part");
    c.refused(json!({ "name": "part" }), &["part", "F1"]);
    c.ok("undo", json!({}));
    assert!(c.job()["operations"]
        .as_array()
        .expect("operations")
        .is_empty());
}

#[test]
fn delete_model_refuses_an_operation_selection_without_history() {
    let mut c = Client::imported("selection");
    c.op("stock");
    let before = c.job();
    let pick = c.pick();
    c.ok(
        "set_operation_geometry",
        json!({ "operation": "F1", "picks": [pick] }),
    );
    c.refused(json!({ "name": "part" }), &["part", "F1"]);
    c.ok("undo", json!({}));
    assert_eq!(c.job(), before);
}

#[test]
fn delete_model_refuses_an_area_pick_without_history() {
    let mut c = Client::imported("area");
    c.op("stock");
    let before = c.job();
    let pick = c.pick();
    c.ok(
        "set_operation_area",
        json!({ "operation": "F1", "area": { "extra": [{ "mode": "include", "picks": [pick] }] } }),
    );
    c.refused(json!({ "name": "part" }), &["part", "F1"]);
    c.ok("undo", json!({}));
    assert_eq!(c.job(), before);
}

#[test]
fn delete_model_refuses_a_setups_model_stock_without_history() {
    let mut c = Client::imported("stock");
    let before = c.job();
    c.ok("set_stock", json!({ "from_model": "part" }));
    c.ok("add_setup", json!({ "name": "First" }));
    c.refused(json!({ "name": "part" }), &["part", "stock"]);
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    assert_eq!(c.job(), before);
}
