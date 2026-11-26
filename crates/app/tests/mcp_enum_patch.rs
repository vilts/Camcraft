//! #2876: enum merge-patch refusals name the field and retained shapes.
//! Gate map (M4.4): covers app::mcp::edit, app::mcp, app::project::json.
//! Provenance: our own MCP contract and RFC 7386 merge-patch semantics.

use camcraft_app::mcp::Server;
use serde_json::{Value, json};

struct Client(Server);

impl Client {
    fn new() -> Self {
        let mut client = Self(Server::new());
        client.ok(
            "set_stock",
            json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }),
        );
        client.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
        client
    }

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

    fn refused(&mut self, name: &str, arguments: Value, path: &str) {
        let result = self.call(name, arguments);
        assert_eq!(
            result["isError"],
            json!(true),
            "{name} must refuse: {result}"
        );
        let text = result["content"][0]["text"].as_str().expect("refusal text");
        for word in [path, "arc", "linear", "null"] {
            assert!(text.contains(word), "refusal must name {word}: {text}");
        }
    }

    fn contour(&mut self, params: Value) {
        self.ok(
            "add_operation",
            json!({
                "kind": "2D Contour", "tool": "EM-0600-4FL", "name": "Contour", "params": params
            }),
        );
    }

    fn params(&mut self) -> Value {
        self.ok("get_job", json!({}))["operations"][0]["params"].clone()
    }
}

#[test]
fn add_contour_lead_shape_refusal_names_retained_shapes() {
    let mut client = Client::new();
    client.refused(
        "add_operation",
        json!({
            "kind": "2D Contour", "tool": "EM-0600-4FL", "name": "Contour",
            "params": { "lead": { "arc": { "radius": 3.0 } } }
        }),
        "lead",
    );
    assert_eq!(client.ok("get_job", json!({}))["operations"], json!([]));
    client.contour(json!({ "lead": { "linear": null, "arc": { "radius": 3.0 } } }));
    assert_eq!(
        client.params()["contour"]["lead"],
        json!({ "arc": { "radius": 3.0 } })
    );
}

#[test]
fn edit_contour_lead_shape_refusal_preserves_parameters_and_undo() {
    let mut client = Client::new();
    client.contour(json!({}));
    let before = client.params();
    client.refused(
        "edit_operation",
        json!({
            "name": "Contour", "params": { "lead": { "arc": { "radius": 3.0 } } }
        }),
        "lead",
    );
    assert_eq!(client.params(), before);
    client.ok("undo", json!({}));
    assert_eq!(client.ok("get_job", json!({}))["operations"], json!([]));
    client.ok("redo", json!({}));
    client.ok(
        "edit_operation",
        json!({
            "name": "Contour", "params": { "lead": { "linear": null, "arc": { "radius": 3.0 } } }
        }),
    );
    assert_eq!(
        client.params()["contour"]["lead"],
        json!({ "arc": { "radius": 3.0 } })
    );
    client.ok(
        "edit_operation",
        json!({
            "name": "Contour", "params": { "lead": { "arc": { "radius": 4.0 } } }
        }),
    );
    assert_eq!(
        client.params()["contour"]["lead"],
        json!({ "arc": { "radius": 4.0 } })
    );
}

#[test]
fn nested_lead_out_shape_refusal_names_full_parameter_path() {
    let mut client = Client::new();
    client.contour(
        json!({ "links": { "approach": { "lead_out": { "linear": { "length": 8.0 } } } } }),
    );
    let before = client.params();
    client.refused("edit_operation", json!({
        "name": "Contour", "params": { "links": { "approach": { "lead_out": { "arc": { "radius": 3.0 } } } } }
    }), "links.approach.lead_out");
    assert_eq!(client.params(), before);
    client.ok(
        "edit_operation",
        json!({
            "name": "Contour", "params": { "links": { "approach": { "lead_out": {
                "linear": null, "arc": { "radius": 3.0 }
            } } } }
        }),
    );
    assert_eq!(
        client.params()["contour"]["links"]["approach"]["lead_out"],
        json!({ "arc": { "radius": 3.0 } })
    );
}
