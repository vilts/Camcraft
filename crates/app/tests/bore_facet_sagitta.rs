//! #689: a Bore at a STEP hole's own size must pass the program gate. The
//! imported mesh's chords stand inside a concave wall by their sagitta, so a
//! mesh coarser than the gouge tolerance charged the true circle as a
//! gouge the wall's full height. A bore cut oversize into the same wall is
//! still refused.
//!
//! Gate map: covers app::geometry::import::step (tessellation fidelity)
//! through app::mcp and app::posting's program gate.
//!
//! Provenance: the issue's own MCP reproduction; chord sagitta
//! `r * (1 - cos(theta / 2))` from elementary geometry.

use camcraft_app::mcp::Server;
use serde_json::{json, Value};

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        Self { server: Server::new(), next: 1 }
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
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
}

fn model() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/Milling-test.step")
        .to_string_lossy()
        .into_owned()
}

/// The issue's steps: the part, its own stock, an Ø25.4 end mill, a bore on
/// the detected Ø25 hole, cut `oversize` mm past the model's wall (0: at
/// nominal). Returns the `verify` reply.
fn bore_verdict(oversize: f64) -> Value {
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": model(), "name": "part" }));
    c.ok("set_stock", json!({ "from_model": "part" }));
    c.ok("add_tool", json!({ "catalog": "EM-1000-4FL" }));
    c.ok("add_operation", json!({ "kind": "bore", "tool": "EM-1000-4FL", "source": "part", "name": "OP" }));
    c.ok("detect_holes", json!({ "name": "OP" }));
    if oversize > 0.0 {
        // The wall circle `oversize` mm wider than the hole the model has:
        // the same detected hole, its radius grown.
        let job = c.ok("get_job", json!({}));
        let mut holes = find_holes(&job).expect("the bore carries its detected hole");
        for h in holes.as_array_mut().into_iter().flatten() {
            let r = h["radius"].as_f64().expect("a hole radius");
            h["radius"] = json!(r + oversize);
        }
        c.ok("edit_operation", json!({ "name": "OP", "params": { "holes": holes } }));
    }
    c.ok("generate", json!({ "name": "OP" }));
    c.ok("verify", json!({}))
}

/// The first `holes` array anywhere in `v` (the bore's parameters in
/// `get_job`).
fn find_holes(v: &Value) -> Option<Value> {
    match v {
        Value::Object(m) => m
            .get("holes")
            .filter(|h| h.is_array())
            .cloned()
            .or_else(|| m.values().find_map(find_holes)),
        Value::Array(a) => a.iter().find_map(find_holes),
        _ => None,
    }
}

/// The program gate's problems in a `verify` reply.
fn gate_problems(v: &Value) -> Vec<String> {
    v["gate"]["problems"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

#[test]
fn a_bore_at_the_step_holes_own_size_passes_the_gate() {
    let v = bore_verdict(0.0);
    assert!(gate_problems(&v).is_empty(), "a nominal bore is no gouge: {v}");
    assert_eq!(v["ok"], json!(true), "{v}");
}

/// The blocking finding that holds the post at 0.05 mm: a toolpath `gouge`
/// warning of the bore, "move N cuts past the finish surface".
fn held_by_a_toolpath_gouge(v: &Value) -> bool {
    v["findings"].as_array().is_some_and(|f| {
        f.iter().any(|w| {
            w["op"] == json!("OP")
                && w["kind"] == json!("gouge")
                && w["blocks_post"] == json!(true)
                && w["message"].as_str().is_some_and(|m| m.ends_with("cuts past the finish surface"))
        })
    })
}

/// 0.05 mm into the wall: the toolpath check holds the post (`ok` false,
/// the bore in `held_by`). Since #917 the wall meshes finer than the stock
/// sim's lattice, so the program gate alone sees no gouge cell at this depth
/// (#2846); the 0.1 mm test below pins the gate's own refusal.
#[test]
fn a_bore_cut_oversize_into_the_same_wall_is_still_refused() {
    let v = bore_verdict(0.05);
    assert_eq!(v["ok"], json!(false), "0.05 mm into the wall is refused: {v}");
    assert_eq!(v["held_by"], json!(["OP"]), "the oversize bore holds the post: {v}");
    assert!(held_by_a_toolpath_gouge(&v), "held by the bore's toolpath gouge warning: {v}");
}

/// 0.1 mm into the wall: the program gate itself names the gouge.
#[test]
fn a_bore_cut_a_tenth_into_the_same_wall_is_a_gate_gouge() {
    let v = bore_verdict(0.1);
    let problems = gate_problems(&v);
    assert!(problems.iter().any(|p| p.starts_with("gouge:")), "0.1 mm into the wall is a gouge: {v}");
    assert_eq!(v["ok"], json!(false), "{v}");
}
