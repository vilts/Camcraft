//! #3627: a 3D Finish (Contour) on a stepped pocket, with a retract
//! plane between the pocket floor and the top of the part. The edit is
//! accepted (no rule refuses a retract below the top of the part), so
//! the generated hop from the top passes to the floor passes must not
//! take the tool into the part, and the verify gate must stay clean.
//!
//! Gate map: covers app::mcp's edit_operation / generate / verify, through
//! camcraft_motion::finishing3d (the plain hop of `emit_runs`).
//!
//! Provenance: the issue's own reproduction (#3627), on an analytic
//! fixture written as an ASCII STL.

use camcraft_app::mcp::Server;
use serde_json::{Value, json};

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        Client { server: Server::new(), next: 1 }
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = reply["result"].clone();
        (result["isError"] == json!(true), result)
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, result) = self.call(name, args);
        assert!(!err, "{name} refused: {result}");
        result["structuredContent"].clone()
    }
}

type P = [f64; 3];

/// The stepped pocket of `camcraft-motion`'s `finish3d.rs`: a 24 x 24
/// block, top at 10, with a 12 x 12 pocket down to a floor at 5.
fn step_pocket_stl() -> String {
    let (t, f) = (10.0, 5.0);
    let quads: Vec<[P; 4]> = vec![
        [[-12.0, -12.0, t], [12.0, -12.0, t], [12.0, -6.0, t], [-12.0, -6.0, t]],
        [[-12.0, 6.0, t], [12.0, 6.0, t], [12.0, 12.0, t], [-12.0, 12.0, t]],
        [[-12.0, -6.0, t], [-6.0, -6.0, t], [-6.0, 6.0, t], [-12.0, 6.0, t]],
        [[6.0, -6.0, t], [12.0, -6.0, t], [12.0, 6.0, t], [6.0, 6.0, t]],
        [[6.0, -6.0, f], [-6.0, -6.0, f], [-6.0, -6.0, t], [6.0, -6.0, t]],
        [[6.0, 6.0, f], [6.0, -6.0, f], [6.0, -6.0, t], [6.0, 6.0, t]],
        [[-6.0, 6.0, f], [6.0, 6.0, f], [6.0, 6.0, t], [-6.0, 6.0, t]],
        [[-6.0, -6.0, f], [-6.0, 6.0, f], [-6.0, 6.0, t], [-6.0, -6.0, t]],
        [[-6.0, -6.0, f], [6.0, -6.0, f], [6.0, 6.0, f], [-6.0, 6.0, f]],
        [[-12.0, -12.0, 0.0], [12.0, -12.0, 0.0], [12.0, -12.0, t], [-12.0, -12.0, t]],
        [[12.0, -12.0, 0.0], [12.0, 12.0, 0.0], [12.0, 12.0, t], [12.0, -12.0, t]],
        [[12.0, 12.0, 0.0], [-12.0, 12.0, 0.0], [-12.0, 12.0, t], [12.0, 12.0, t]],
        [[-12.0, 12.0, 0.0], [-12.0, -12.0, 0.0], [-12.0, -12.0, t], [-12.0, 12.0, t]],
        [[-12.0, -12.0, 0.0], [-12.0, 12.0, 0.0], [12.0, 12.0, 0.0], [12.0, -12.0, 0.0]],
    ];
    let mut s = String::from("solid step_pocket\n");
    for q in quads {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            s.push_str("facet normal 0 0 0\nouter loop\n");
            for v in tri {
                s.push_str(&format!("vertex {} {} {}\n", v[0], v[1], v[2]));
            }
            s.push_str("endloop\nendfacet\n");
        }
    }
    s.push_str("endsolid step_pocket\n");
    s
}

/// The job: the pocket, stock from the model, a ball 3D Finish with
/// the Contour pattern, generated with `retract` (offset from the top
/// of the model; `None` keeps the default hop to clearance). Each test
/// passes its own `test` tag: the fixture file is per test, so tests that
/// run at the same time never share it.
fn generate(test: &str, retract: Option<f64>) -> (Client, Value) {
    let dir = std::env::temp_dir().join(format!("camcraft-3627-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("step_pocket.stl");
    std::fs::write(&path, step_pocket_stl()).expect("write the fixture");
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": path.to_string_lossy() }));
    c.ok("set_stock", json!({ "from_model": "step_pocket" }));
    c.ok("add_tool", json!({ "catalog": "BN-0600" }));
    c.ok("add_operation", json!({ "kind": "finish3d", "tool": "BN-0600", "name": "F1", "source": "step_pocket" }));
    let mut params = json!({
        "pattern": "contour",
        "top_z": { "reference": "top_of_model", "offset": 0.0 },
        "bottom_z": { "reference": "bottom_of_model", "offset": 5.0 },
    });
    if let Some(off) = retract {
        params["retract_z"] = json!({ "reference": "top_of_model", "offset": off });
    }
    c.ok("edit_operation", json!({ "name": "F1", "params": params }));
    let generated = c.ok("generate", json!({ "name": "F1" }));
    (c, generated)
}

fn gate_problems(v: &Value) -> Vec<String> {
    v["gate"]["problems"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// The retract at 7 (top of model - 3) is between the floor (5) and the
/// top (10). The edit is accepted and the operation generates; verify
/// finds no gouge and does not hold the post.
#[test]
fn a_retract_below_the_top_of_the_part_is_accepted_and_verifies_clean() {
    let (mut c, generated) = generate("low_retract", Some(-3.0));
    assert!(generated.to_string().contains("\"ok\""), "F1 generates: {generated}");
    let v = c.ok("verify", json!({}));
    eprintln!("verify: {v}");
    assert!(v["gate"]["skipped"].is_null(), "verify gate skipped: {v}");
    assert!(gate_problems(&v).iter().all(|p| !p.starts_with("gouge")), "{v}");
    let blocking: Vec<&Value> = v["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["blocks_post"] == json!(true))
        .collect();
    assert_eq!(blocking, Vec::<&Value>::new(), "no blocking finding: {v}");
}

/// The usual setup: no retract plane, the hop goes to clearance.
#[test]
fn the_default_hop_verifies_clean() {
    let (mut c, generated) = generate("default_hop", None);
    assert!(generated.to_string().contains("\"ok\""), "F1 generates: {generated}");
    let v = c.ok("verify", json!({}));
    let blocking: Vec<&Value> = v["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["blocks_post"] == json!(true))
        .collect();
    assert_eq!(blocking, Vec::<&Value>::new(), "no blocking finding: {v}");
}
