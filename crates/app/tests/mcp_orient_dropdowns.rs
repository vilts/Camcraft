//! Provenance: our own headless MCP fixtures over the setup orientation commands (#1586).
//! Gate map: covers app::mcp::align_tools, app::commands::set_wcs, app::pick::cylinder.

use camcraft_app::geometry::{Mesh, Point3, cube, cylinder};
use camcraft_app::mcp::Server;
use serde_json::{Value, json};
use std::path::PathBuf;

struct Client {
    server: Server,
    next: u64,
    dir: PathBuf,
}

impl Client {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("camcraft-mcp-orient-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let mut c = Self {
            server: Server::new(),
            next: 1,
            dir,
        };
        c.request("initialize", json!({"protocolVersion": "2025-06-18"}));
        c
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        self.server
            .handle(&json!({"jsonrpc": "2.0", "id": self.next, "method": method, "params": params}))
            .expect("response")
    }

    fn call(&mut self, name: &str, args: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": args}))["result"].clone()
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let r = self.call(name, args);
        assert_ne!(r["isError"], json!(true), "{name}: {r}");
        r["structuredContent"].clone()
    }

    fn refused(&mut self, name: &str, args: Value, text: &str) {
        let r = self.call(name, args);
        assert_eq!(r["isError"], json!(true), "{name}: {r}");
        let msg = r["content"][0]["text"].as_str().expect("refusal text");
        assert!(msg.contains(text), "expected {text:?}: {msg}");
    }

    fn import(&mut self, name: &str, mesh: Mesh) {
        let path = self.dir.join(format!("{name}.stl"));
        let mut stl = String::from("solid part\n");
        for t in mesh.facets.iter() {
            stl.push_str("facet normal 0 0 0\nouter loop\n");
            for p in [t.a, t.b, t.c] {
                stl.push_str(&format!("vertex {} {} {}\n", p.x, p.y, p.z));
            }
            stl.push_str("endloop\nendfacet\n");
        }
        stl.push_str("endsolid part\n");
        std::fs::write(&path, stl).expect("STL fixture");
        self.ok("import_model", json!({"path": path, "name": name}));
    }

    fn saved_setup(&mut self, name: &str) -> Value {
        let path = self.dir.join("pose.camproj");
        self.ok("save_project", json!({"path": path}));
        let project: Value = serde_json::from_slice(&std::fs::read(path).expect("saved project"))
            .expect("project JSON");
        let setup = project["entities"]
            .as_array()
            .expect("entities")
            .iter()
            .find(|e| e["name"] == name)
            .expect("named setup")["setup"]
            .clone();
        assert!(setup.is_object(), "saved setup payload: {setup}");
        setup
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn rotated(q: &Value, p: [f64; 3]) -> [f64; 3] {
    let q: Vec<f64> = q
        .as_array()
        .expect("quaternion")
        .iter()
        .map(|v| v.as_f64().expect("number"))
        .collect();
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let t = cross([q[0], q[1], q[2]], p).map(|x| 2.0 * x);
    let u = cross([q[0], q[1], q[2]], t);
    [
        p[0] + q[3] * t[0] + u[0],
        p[1] + q[3] * t[1] + u[1],
        p[2] + q[3] * t[2] + u[2],
    ]
}

#[test]
fn orientation_tools_are_listed() {
    let mut c = Client::new("list");
    let r = c.request("tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().expect("tools");
    for name in ["align_part_cylinder", "place_setup_origin"] {
        assert!(tools.iter().any(|t| t["name"] == name), "missing {name}");
    }
}

#[test]
fn align_part_cylinder_lands_undoes_and_refuses_by_name() {
    let mut c = Client::new("cylinder");
    c.import("part", cylinder(20.0, 10.0, 48));
    c.ok("add_setup", json!({}));
    let f = c.ok("list_features", json!({"model": "part"}));
    // The wall contains the curved strips; each cap has one planar face.
    let wall = f["patches"]
        .as_array()
        .expect("patches")
        .iter()
        .max_by_key(|p| p["faces"].as_array().map_or(0, Vec::len))
        .expect("wall");
    let pick = json!({"model": "part", "id": wall["id"]});
    let before = c.saved_setup("Setup1");
    let args = json!({"pick": pick, "target": "x", "angle_deg": 0.0});
    let placed = c.ok("align_part_cylinder", args.clone());
    assert_ne!(
        c.saved_setup("Setup1"),
        before,
        "alignment changes the stored pose"
    );
    let axis = rotated(&placed["alignment"]["rotation"], [0.0, 0.0, 1.0]);
    assert!(
        axis[0].abs() > 1.0 - 1e-8 && axis[1].abs() < 1e-8 && axis[2].abs() < 1e-8,
        "axis: {axis:?}"
    );
    c.ok("undo", json!({}));
    assert_eq!(
        c.saved_setup("Setup1"),
        before,
        "one undo restores the stored prior pose"
    );
    let again = c.ok("align_part_cylinder", args);
    assert_eq!(
        again["alignment"], placed["alignment"],
        "one undo restores the prior pose"
    );
    c.ok("undo", json!({}));
    let angled = c.ok(
        "align_part_cylinder",
        json!({"pick": pick, "target": "x", "angle_deg": 30.0}),
    );
    assert_ne!(
        angled["alignment"]["rotation"], placed["alignment"]["rotation"],
        "the angle turns about the landed axis"
    );
    c.ok("undo", json!({}));
    c.refused(
        "align_part_cylinder",
        json!({"pick": pick, "setup": "missing"}),
        "missing",
    );
    c.refused(
        "align_part_cylinder",
        json!({"pick": {"model": "missing", "id": wall["id"]}}),
        "no model named 'missing'",
    );
    let cap = &f["faces"][0]["id"];
    c.refused(
        "align_part_cylinder",
        json!({"pick": {"model": "part", "id": cap}}),
        "not a welded wall",
    );
    c.refused(
        "align_part_cylinder",
        json!({"pick": pick, "target": "bad"}),
        "target",
    );
    c.refused(
        "align_part_cylinder",
        json!({"pick": pick, "angle_deg": "bad"}),
        "angle_deg",
    );
    c.refused(
        "align_part_cylinder",
        json!({"pick": pick, "flip": "bad"}),
        "flip",
    );
}

#[test]
fn consecutive_cylinder_alignments_land_the_original_axis() {
    let mut c = Client::new("consecutive-cylinder");
    c.import("part", cylinder(20.0, 10.0, 48));
    c.ok("add_setup", json!({}));
    let features = c.ok("list_features", json!({"model": "part"}));
    let wall = features["patches"]
        .as_array()
        .expect("patches")
        .iter()
        .max_by_key(|p| p["faces"].as_array().map_or(0, Vec::len))
        .expect("wall");
    let pick = json!({"model": "part", "id": wall["id"]});
    let first = c.ok(
        "align_part_cylinder",
        json!({"pick": pick, "target": "x", "angle_deg": 21.0}),
    );
    let first_setup = c.saved_setup("Setup1");
    let axis = rotated(&first["alignment"]["rotation"], [0.0, 0.0, 1.0]);
    assert!(
        axis[0].abs() > 1.0 - 1e-8 && axis[1].abs() < 1e-8 && axis[2].abs() < 1e-8,
        "first alignment's original cylinder axis: {axis:?}"
    );
    let second = c.ok(
        "align_part_cylinder",
        json!({"pick": pick, "target": "y", "angle_deg": 33.0, "flip": true}),
    );
    // Read the actual stored pose, independently of the backend's landing proof.
    let second_setup = c.saved_setup("Setup1");
    assert_ne!(second_setup, first_setup);
    assert_eq!(second["alignment"]["rotation"], second_setup["part_alignment"]["rotation"]);
    let axis = rotated(&second_setup["part_alignment"]["rotation"], [0.0, 0.0, 1.0]);
    assert!(
        axis[0].abs() < 1e-8 && axis[1].abs() > 1.0 - 1e-8 && axis[2].abs() < 1e-8,
        "second alignment's original cylinder axis: {axis:?}"
    );
    c.ok("undo", json!({}));
    assert_eq!(c.saved_setup("Setup1"), first_setup);
    c.ok("redo", json!({}));
    assert_eq!(c.saved_setup("Setup1"), second_setup);
}

#[test]
fn place_setup_origin_moves_named_setup_keeps_rotation_and_undoes() {
    let mut c = Client::new("origin");
    c.import(
        "part",
        cube(
            Point3::new(-10.0, -15.0, 0.0),
            Point3::new(10.0, 15.0, 10.0),
        ),
    );
    c.ok("add_setup", json!({}));
    let half = std::f64::consts::FRAC_1_SQRT_2;
    c.ok(
        "set_wcs",
        json!({"origin": [3.0, 4.0, 5.0], "rotation": [0.0, 0.0, half, half]}),
    );
    let before = c.ok("get_job", json!({}))["setups"][0]["wcs"].clone();
    c.ok("add_setup", json!({}));
    c.ok(
        "place_setup_origin",
        json!({"setup": "Setup1", "source": "model_box", "point": "centre"}),
    );
    let after = c.ok("get_job", json!({}));
    assert_eq!(after["setups"][0]["wcs"]["origin"], json!([0.0, 0.0, 5.0]));
    assert_eq!(after["setups"][0]["wcs"]["rotation"], before["rotation"]);
    assert_eq!(
        after["setups"][1]["wcs"]["placed"], false,
        "active Setup2 stays untouched"
    );
    c.refused(
        "place_setup_origin",
        json!({"setup": "Setup1", "source": "model_box", "point": "centre"}),
        "already",
    );
    c.ok("undo", json!({}));
    assert_eq!(
        c.ok("get_job", json!({}))["setups"][0]["wcs"],
        before,
        "one undo restores origin and rotation"
    );
    c.refused(
        "place_setup_origin",
        json!({"setup": "missing", "source": "model_box"}),
        "missing",
    );
    c.refused(
        "place_setup_origin",
        json!({"source": "stock_box"}),
        "no stock",
    );
    c.refused("place_setup_origin", json!({"source": "bad"}), "source");
    c.refused(
        "place_setup_origin",
        json!({"source": "model_box", "point": "bad"}),
        "point",
    );
    let f = c.ok("list_features", json!({"model": "part"}));
    c.refused(
        "place_setup_origin",
        json!({"source": "model_box", "pick": {"model": "part", "id": f["vertices"][0]["id"]}}),
        "pick",
    );
    c.refused("place_setup_origin", json!({"source": "vertex", "point": "corner", "pick": {"model": "part", "id": f["vertices"][0]["id"]}}), "point");
    c.refused(
        "place_setup_origin",
        json!({"source": "vertex", "pick": {"model": "part", "id": f["faces"][0]["id"]}}),
        "re-pick it",
    );
    let vertex = &f["vertices"][0];
    c.ok(
        "place_setup_origin",
        json!({"source": "vertex", "pick": {"model": "part", "id": vertex["id"]}}),
    );
    assert_eq!(
        c.ok("get_job", json!({}))["setups"][1]["wcs"]["origin"],
        vertex["at"]
    );
    c.ok("undo", json!({}));
    assert_eq!(
        c.ok("get_job", json!({}))["setups"][1]["wcs"]["placed"],
        false
    );
    c.ok(
        "set_stock",
        json!({"box": {"dx": 40.0, "dy": 50.0, "dz": 12.0}}),
    );
    c.ok(
        "place_setup_origin",
        json!({"source": "stock_box", "point": "corner"}),
    );
    assert_eq!(
        c.ok("get_job", json!({}))["setups"][1]["wcs"]["origin"],
        json!([-20.0, -25.0, 0.0])
    );
    c.ok("undo", json!({}));
    assert_eq!(
        c.ok("get_job", json!({}))["setups"][1]["wcs"]["placed"],
        false
    );
}

#[test]
fn named_setup_vertex_origin_uses_its_pose_when_inactive() {
    let mut c = Client::new("named-vertex-origin");
    c.import("part", cylinder(20.0, 10.0, 48));
    c.import("block", cube(Point3::new(30.0, 40.0, 50.0), Point3::new(40.0, 50.0, 60.0)));
    c.ok("add_setup", json!({}));
    let features = c.ok("list_features", json!({"model": "part"}));
    let wall = features["patches"].as_array().expect("patches").iter()
        .max_by_key(|p| p["faces"].as_array().map_or(0, Vec::len)).expect("wall");
    let vertices = c.ok("list_features", json!({"model": "block"}));
    let vertex = vertices["vertices"].as_array().expect("vertices").iter()
        .find(|v| v["at"] == json!([30.0, 50.0, 50.0])).expect("cube corner");
    let args = json!({"setup": "Setup1", "source": "vertex",
        "pick": {"model": "block", "id": vertex["id"]}});
    c.ok("align_part_cylinder", json!({"pick": {"model": "part", "id": wall["id"]}, "target": "x"}));
    c.ok("add_setup", json!({}));
    let before = c.ok("get_job", json!({}));
    let inactive = c.ok("place_setup_origin", args.clone());
    let after = c.ok("get_job", json!({}));
    assert_eq!(after["active_setup"], "Setup2");
    assert_eq!(after["setups"][1], before["setups"][1], "Setup2 is untouched");
    assert_eq!(after["setups"][0]["wcs"]["origin"], inactive["origin"]);
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["setups"][0]["wcs"], before["setups"][0]["wcs"]);
    c.ok("activate_setup", json!({"name": "Setup1"}));
    let active = c.ok("place_setup_origin", args);
    assert_eq!(inactive, active, "the named setup's pose must not depend on the active setup");
    for (got, expected) in inactive["origin"].as_array().expect("origin").iter().zip([35.0, 35.0, 60.0]) {
        assert!((got.as_f64().expect("coordinate") - expected).abs() < 1e-8, "origin: {inactive}");
    }
}
