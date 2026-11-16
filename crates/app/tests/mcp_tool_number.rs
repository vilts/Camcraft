//! The MCP tool-number tool (#730): `set_tool_number` over the server's
//! JSON-RPC handler. A renumbered tool posts as that `T` word, `get_job`
//! reads `number` and `posts_as`, one undo restores the old number, and
//! every refusal is named.
//!
//! Gate map (M4.4): covers app::mcp::tool_number_tools, app::mcp::job,
//! app::commands (tool_number).

use camcraft_app::mcp::Server;

use serde_json::{Value, json};

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        let mut c = Self { server: Server::new(), next: 1 };
        c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
        c
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.server.handle(&msg).expect("a request gets a response")
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        let is_err = result["isError"] == json!(true);
        let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
        (is_err, body)
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }

    fn tool_row(&mut self, name: &str) -> Value {
        let job = self.ok("get_job", json!({}));
        job["tools"].as_array().and_then(|t| t.iter().find(|t| t["name"] == json!(name))).cloned().unwrap_or(Value::Null)
    }
}

fn write_box_stl(path: &std::path::Path) {
    let (x, y, z) = (20.0f64, 20.0f64, 15.0f64);
    let v = |i: usize| -> [f64; 3] {
        [if i & 1 == 0 { -x } else { x }, if i & 2 == 0 { -y } else { y }, if i & 4 == 0 { 0.0 } else { z }]
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let mut s = String::from("solid box\n");
    for q in quads {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            s.push_str(" facet normal 0 0 0\n  outer loop\n");
            for i in tri {
                let p = v(i);
                s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
            }
            s.push_str("  endloop\n endfacet\n");
        }
    }
    s.push_str("endsolid box\n");
    std::fs::write(path, s).expect("write stl");
}

#[test]
fn a_tool_is_renumbered_read_posted_undone_and_refused_over_mcp() {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-toolnum-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    let stl = p.join("block.stl");
    write_box_stl(&stl);

    let mut c = Client::new();
    let listed = c.request("tools/list", json!({}))["result"]["tools"].to_string();
    assert!(listed.contains("set_tool_number"));
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));

    // Unnumbered: no own number, posts as its list position.
    let row = c.tool_row("EM-0600-4FL");
    assert_eq!(row["number"], Value::Null, "{row}");
    assert_eq!(row["posts_as"], json!(2), "{row}");

    let set = c.ok("set_tool_number", json!({ "tool": "EM-0600-4FL", "number": 17 }));
    assert_eq!(set["number"], json!(17));
    assert_eq!(set["posts_as"], json!(17));
    let row = c.tool_row("EM-0600-4FL");
    assert_eq!((row["number"].clone(), row["posts_as"].clone()), (json!(17), json!(17)), "{row}");

    // The post writes the renumbered T word.
    c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0600-4FL" }));
    c.ok("generate", json!({}));
    let nc = p.join("out.nc");
    c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    let text = std::fs::read_to_string(&nc).unwrap_or_default();
    assert!(text.contains("T17"), "the program posts T17:\n{text}");

    // Refusals, each by name; none changes the document.
    assert!(c.refused("set_tool_number", json!({ "tool": "EM-0400-2FL", "number": 17 })).contains("already 'EM-0600-4FL'"));
    assert!(c.refused("set_tool_number", json!({ "tool": "EM-0400-2FL", "number": 0 })).contains("out of range"));
    assert!(c.refused("set_tool_number", json!({ "tool": "EM-0400-2FL", "number": 100000 })).contains("out of range"));
    assert!(c.refused("set_tool_number", json!({ "tool": "EM-0400-2FL", "number": -3 })).contains("positive integer"));
    assert!(c.refused("set_tool_number", json!({ "tool": "nope", "number": 5 })).contains("nope"));
    assert!(c.refused("set_tool_number", json!({ "tool": "block", "number": 5 })).contains("not a tool"));
    assert!(c.refused("set_tool_number", json!({ "tool": "EM-0600-4FL", "number": 17 })).contains("already has that tool number"));
    assert!(c.refused("set_tool_number", json!({ "tool": "EM-0600-4FL" })).contains("number"));

    // Clearing is its own step; undo walks back one step at a time.
    let cleared = c.ok("set_tool_number", json!({ "tool": "EM-0600-4FL", "number": null }));
    assert_eq!((cleared["number"].clone(), cleared["posts_as"].clone()), (Value::Null, json!(2)));
    c.ok("undo", json!({}));
    assert_eq!(c.tool_row("EM-0600-4FL")["number"], json!(17));
    c.ok("undo", json!({}));
    // One undo step undid the set; generate/op steps may sit above it, so
    // keep undoing until the number is gone.
    for _ in 0..6 {
        if c.tool_row("EM-0600-4FL")["number"] == Value::Null {
            break;
        }
        c.ok("undo", json!({}));
    }
    let row = c.tool_row("EM-0600-4FL");
    assert_eq!((row["number"].clone(), row["posts_as"].clone()), (Value::Null, json!(2)), "{row}");
}
