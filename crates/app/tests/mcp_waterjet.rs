//! #1883: the Waterjet Profile through headless MCP - a waterjet .mch
//! mounted, a nozzle tool, the op added with tabs, edited, undone,
//! redone and generated.
//! Gate map (#1883): covers app::mcp::tools, app::mcp::edit,
//! app::mcp::setup_tools, app::project (waterjet_dto).
//! Provenance: our own MCP tests; the holed plate follows mcp_plasma.rs.

mod pocket_support;

use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::mcp::Server;
use serde_json::{json, Value};
use std::path::PathBuf;

const WATERJET: &str = r#"{"general":{"model":"Waterjet test","kind":"waterjet"},"kinematics":{"default":{"parts":[{"name":"X","type":"linear","travel":{"min":-100,"max":360}},{"name":"Y","type":"linear","travel":{"min":-150,"max":150}},{"name":"Z","type":"linear","travel":{"min":-20,"max":60}},{"name":"nozzle","type":"head"}]}},"controller":{"synced_configuration":{"parts":{"X":{"coordinate":0,"max_normal_speed":1000},"Y":{"coordinate":1,"max_normal_speed":700},"Z":{"coordinate":2,"max_normal_speed":500}}}}}"#;

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let reply = self
            .server
            .handle(&json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } }))
            .expect("a request gets a response");
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        let err = result["isError"] == json!(true);
        (err, if err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() })
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }

    fn params(&mut self) -> Value {
        let job = self.ok("get_job", json!({}));
        job["operations"].as_array().and_then(|ops| ops.iter().find(|o| o["name"] == json!("Jet1"))).expect("Jet1")["params"]
            ["waterjet_profile"]
            .clone()
    }
}

fn tmp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("camcraft-mcp-waterjet-1883-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("temp directory");
    path
}

/// A 40 x 40 x 5 mm plate with a 10 x 10 mm through hole.
fn holed_plate() -> Mesh {
    let ring = |h: f64, z: f64| [Point3::new(-h, -h, z), Point3::new(h, -h, z), Point3::new(h, h, z), Point3::new(-h, h, z)];
    let quad = |a, b, c, d| [Triangle::new(a, b, c), Triangle::new(a, c, d)];
    let mut facets = Vec::new();
    for (z, up) in [(5.0, true), (0.0, false)] {
        let (outer, inner) = (ring(20.0, z), ring(5.0, z));
        for i in 0..4 {
            let j = (i + 1) % 4;
            facets.extend(if up { quad(outer[i], outer[j], inner[j], inner[i]) } else { quad(outer[i], inner[i], inner[j], outer[j]) });
        }
    }
    for (radius, outward) in [(20.0, true), (5.0, false)] {
        let (lo, hi) = (ring(radius, 0.0), ring(radius, 5.0));
        for i in 0..4 {
            let j = (i + 1) % 4;
            facets.extend(if outward { quad(lo[i], lo[j], hi[j], hi[i]) } else { quad(lo[i], hi[i], hi[j], lo[j]) });
        }
    }
    Mesh::from_facets(facets)
}

#[test]
fn waterjet_profile_is_added_edited_undone_redone_and_generated_over_mcp() {
    let dir = tmp_dir();
    let mch = dir.join("wj.mch");
    std::fs::write(&mch, WATERJET).expect("write mch");
    let stl = dir.join("plate.stl");
    pocket_support::write_stl(&holed_plate(), &stl);
    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("load_machine", json!({ "path": mch }));
    // A nozzle from a full definition: the torch's shape, the waterjet kind.
    c.ok("add_tool", json!({ "catalog": "PT-KERF-150", "name": "T0" }));
    let mut def = c.ok("get_job", json!({}))["tools"][0]["definition"].clone();
    def["kind"] = json!("waterjet");
    def["diameter"] = json!(0.8);
    c.ok("add_tool", json!({ "name": "Nozzle", "definition": def }));
    c.ok("import_model", json!({ "path": stl, "name": "plate" }));
    let added = c.ok("add_operation", json!({
        "kind": "Waterjet Profile", "name": "Jet1", "tool": "Nozzle", "source": "plate",
        "params": { "clearance_z": 25.0, "top_z": 5.0, "bottom_z": 0.0, "feed": 250.0,
            "tabs": { "count": 1, "width": 2.0, "at": [[20.0, 0.0]] } }
    }));
    assert_eq!(added["kind"], json!("Waterjet Profile"), "{added}");
    let before = c.params();
    assert_eq!(before["tabs"]["count"], json!(1), "{before}");
    let edited = c.ok("edit_operation", json!({ "name": "Jet1", "params": { "pierce_time": 2.5 } }));
    assert_eq!(edited["params"]["waterjet_profile"]["pierce_time"], json!(2.5), "{edited}");
    c.ok("undo", json!({}));
    assert_eq!(c.params(), before);
    c.ok("redo", json!({}));
    assert_eq!(c.params()["pierce_time"], json!(2.5));
    let generated = c.ok("generate", json!({ "name": "Jet1" }));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let _ = std::fs::remove_dir_all(&dir);
}
