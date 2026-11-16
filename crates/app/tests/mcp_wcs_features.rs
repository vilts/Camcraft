//! #937: the MCP `set_wcs_from_features` tool, driven in-process over
//! the server's JSON-RPC handler. Pins: the WCS lands on exactly the
//! frame the GUI's law (`pick::align_roles`, #701) fits from the same
//! picks - position and the quaternion the GUI's `SetWcs` builds from
//! its angles, a curved wall's fitted centre included - as one undo
//! step, and every bad pick set is refused by name with nothing placed.
//!
//! Gate map (#937): covers app::mcp::wcs_tools, app::pick::roles (the
//! Z-then-Y role pair).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::commands::{History, ImportModel};
use camcraft_app::geometry::{cylinder, quat_from_euler};
use camcraft_app::mcp::Server;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{Role, align_roles};
use camcraft_app::world::new_world;

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-wcs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

fn write_stl(path: &Path, tris: &[[[f64; 3]; 3]]) {
    let mut s = String::from("solid part\n");
    for t in tris {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in t {
            s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid part\n");
    std::fs::write(path, s).expect("write stl");
}

/// A 40 x 40 x 15 box, centred in XY, z from 0.
fn write_box(path: &Path) {
    let v = |i: usize| -> [f64; 3] {
        [if i & 1 == 0 { -20.0 } else { 20.0 }, if i & 2 == 0 { -20.0 } else { 20.0 }, if i & 4 == 0 { 0.0 } else { 15.0 }]
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let tris: Vec<[[f64; 3]; 3]> = quads
        .iter()
        .flat_map(|q| [[v(q[0]), v(q[1]), v(q[2])], [v(q[0]), v(q[2]), v(q[3])]])
        .collect();
    write_stl(path, &tris);
}

/// A 20-dia, 10-high, 48-segment cylinder: its wall welds to one patch.
fn write_bore(path: &Path) {
    let p = |q: camcraft_app::geometry::Point3| [q.x, q.y, q.z];
    let tris: Vec<[[f64; 3]; 3]> = cylinder(20.0, 10.0, 48).facets.iter().map(|t| [p(t.a), p(t.b), p(t.c)]).collect();
    write_stl(path, &tris);
}

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

    fn wcs(&mut self) -> Value {
        self.ok("get_job", json!({}))["setups"][0]["wcs"].clone()
    }
}

/// The box's feature ids by what they are: a face by its bounds (a
/// face row's `normal` is the plane's canonical one, not its outward
/// side - the top and bottom both read +Z), an edge by its two ends, a
/// vertex by its point.
struct Ids(Value);

impl Ids {
    fn face(&self, min: [f64; 3], max: [f64; 3]) -> String {
        self.find("faces", |f| f["bounds"]["min"] == json!(min) && f["bounds"]["max"] == json!(max))
    }
    fn edge(&self, a: [f64; 3], b: [f64; 3]) -> String {
        self.find("chains", |c| {
            (c["start"] == json!(a) && c["end"] == json!(b)) || (c["start"] == json!(b) && c["end"] == json!(a))
        })
    }
    fn vertex(&self, at: [f64; 3]) -> String {
        self.find("vertices", |v| v["at"] == json!(at))
    }
    fn find(&self, kind: &str, want: impl Fn(&Value) -> bool) -> String {
        self.0[kind]
            .as_array()
            .and_then(|rows| rows.iter().find(|r| want(r)))
            .and_then(|r| r["id"].as_str())
            .unwrap_or_else(|| panic!("no such {kind} row in {}", self.0))
            .to_owned()
    }
}

fn pick(model: &str, id: &str, role: &str) -> Value {
    json!({ "model": model, "id": id, "role": role })
}

/// A list_features id back to the selection the GUI's picks carry.
fn sel(model: &str, id: &str) -> GeoSel {
    let (kind, hex) = id.split_once(':').expect("kind:0x…");
    let n = u64::from_str_radix(hex.trim_start_matches("0x"), 16).expect("hex id");
    let feature = match kind {
        "face" => FeatureSel::Face(n),
        "chain" => FeatureSel::Chain(n),
        "patch" => FeatureSel::Patch(n),
        _ => FeatureSel::Vertex(n),
    };
    GeoSel { entity: model.into(), feature }
}

fn role(word: &str) -> Role {
    match word {
        "origin" => Role::Origin,
        "x" => Role::X,
        "y" => Role::Y,
        _ => Role::Z,
    }
}

#[test]
fn set_wcs_from_features_lands_on_the_guis_frame_as_one_undo_step() {
    let dir = tmp_dir("frame");
    let (block, bore) = (dir.join("block.stl"), dir.join("bore.stl"));
    write_box(&block);
    write_bore(&bore);
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": block.display().to_string() }));
    c.ok("import_model", json!({ "path": bore.display().to_string() }));
    c.ok("add_setup", json!({}));
    let ids = Ids(c.ok("list_features", json!({ "model": "block" })));
    let wall = c.ok("list_features", json!({ "model": "bore", "kinds": ["patches"] }))["patches"]
        .as_array()
        .and_then(|ps| ps.iter().max_by_key(|p| p["faces"].as_array().map_or(0, Vec::len)))
        .and_then(|p| p["id"].as_str())
        .expect("the bore's wall welds to a patch")
        .to_owned();

    let names: Vec<String> = c.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    assert!(names.iter().any(|n| n == "set_wcs_from_features"), "{names:?}");

    // The plain 3-2-1: top face Z, front edge X, its corner the origin.
    let plain = [
        pick("block", &ids.face([-20.0, -20.0, 15.0], [20.0, 20.0, 15.0]), "z"),
        pick("block", &ids.edge([-20.0, -20.0, 15.0], [20.0, -20.0, 15.0]), "x"),
        pick("block", &ids.vertex([-20.0, -20.0, 15.0]), "origin"),
    ];
    // A turned frame (#937's Z-then-Y pair): the +X side's normal is Z,
    // its top edge Y, its near top corner the origin.
    let turned = [
        pick("block", &ids.face([20.0, -20.0, 0.0], [20.0, 20.0, 15.0]), "z"),
        pick("block", &ids.edge([20.0, -20.0, 15.0], [20.0, 20.0, 15.0]), "y"),
        pick("block", &ids.vertex([20.0, -20.0, 15.0]), "origin"),
    ];
    // A curved wall for the origin: the bore's fitted centre, projected
    // onto the top face's plane.
    let walled = [
        pick("block", &ids.face([-20.0, -20.0, 15.0], [20.0, 20.0, 15.0]), "z"),
        pick("block", &ids.edge([-20.0, -20.0, 15.0], [20.0, -20.0, 15.0]), "x"),
        pick("bore", &wall, "origin"),
    ];

    // The reference: the GUI's law on an identical world, turned into a
    // WCS placement exactly as the GUI's `SetWcs` does.
    let world = new_world();
    let mut h = History::default();
    for p in [&block, &bore] {
        h.execute(&world, ImportModel::new(p).expect("import"));
    }
    let mut placed = Vec::new();
    for set in [&plain, &turned, &walled] {
        let roles: Vec<(GeoSel, Role)> = set
            .iter()
            .map(|p| {
                let (m, id, r) = (p["model"].as_str(), p["id"].as_str(), p["role"].as_str());
                (sel(m.unwrap_or_default(), id.unwrap_or_default()), role(r.unwrap_or_default()))
            })
            .collect();
        let law = align_roles(&world, &roles).expect("the GUI's law takes these picks");
        let q = quat_from_euler(law.roll_deg.to_radians(), law.pitch_deg.to_radians(), law.yaw_deg.to_radians());

        let out = c.ok("set_wcs_from_features", json!({ "picks": set }));
        assert_eq!(out["origin"], json!(law.pos), "origin is the law's: {out}");
        assert_eq!(out["rotation"], json!(q), "rotation is the GUI's quaternion: {out}");
        assert_eq!(out["axes"]["z"], json!(law.z), "{out}");
        assert_eq!(c.wcs()["origin"], json!(law.pos), "get_job reads it back");
        placed.push(out);
    }

    // The frames, spelled out.
    let near = |v: &Value, want: [f64; 3]| {
        let got: Vec<f64> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
        got.len() == 3 && got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-9)
    };
    let p = &placed[0];
    assert!(near(&p["origin"], [-20.0, -20.0, 15.0]), "identity at the corner: {p}");
    assert!(near(&p["axes"]["x"], [1.0, 0.0, 0.0]) && near(&p["axes"]["z"], [0.0, 0.0, 1.0]), "{p}");
    // Z stays on the Z pick and Y on the Y pick; X closes the frame.
    let p = &placed[1];
    assert!(near(&p["origin"], [20.0, -20.0, 15.0]), "{p}");
    assert!(near(&p["axes"]["z"], [1.0, 0.0, 0.0]), "Z is the side's outward normal: {p}");
    assert!(near(&p["axes"]["y"], [0.0, 1.0, 0.0]), "Y runs along the picked edge, away from the origin: {p}");
    assert!(near(&p["axes"]["x"], [0.0, 0.0, -1.0]), "X = Y x Z: {p}");
    let p = &placed[2];
    assert!(near(&p["origin"], [0.0, 0.0, 15.0]), "the bore's centre on the top face's plane: {p}");

    // One History step each: undo goes back to the turned frame, redo
    // forward to the walled one.
    c.ok("undo", json!({}));
    assert!(near(&c.wcs()["origin"], [20.0, -20.0, 15.0]));
    c.ok("redo", json!({}));
    assert_eq!(c.wcs()["origin"], placed[2]["origin"]);
}

#[test]
fn bad_feature_picks_are_refused_by_name_and_place_nothing() {
    let dir = tmp_dir("refuse");
    let block = dir.join("block.stl");
    write_box(&block);
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": block.display().to_string() }));
    c.ok("add_setup", json!({}));
    c.ok("add_setup", json!({ "name": "Back" }));
    c.ok("activate_setup", json!({ "name": "Setup1" }));
    let ids = Ids(c.ok("list_features", json!({ "model": "block" })));
    let top = ids.face([-20.0, -20.0, 15.0], [20.0, 20.0, 15.0]);
    let side = ids.face([20.0, -20.0, 0.0], [20.0, 20.0, 15.0]);
    let front = ids.edge([-20.0, -20.0, 15.0], [20.0, -20.0, 15.0]);
    let upright = ids.edge([-20.0, -20.0, 0.0], [-20.0, -20.0, 15.0]);
    let corner = ids.vertex([-20.0, -20.0, 15.0]);
    let go = |c: &mut Client, args: Value| c.refused("set_wcs_from_features", args);
    let cases: Vec<(Value, &str)> = vec![
        (json!({}), "\"picks\" is required"),
        (json!({ "picks": [pick("block", &top, "z")] }), "two axes, plus an optional origin"),
        (json!({ "picks": [pick("block", &top, "z"), pick("block", &side, "z")] }), "only one pick may set Z"),
        (json!({ "picks": [pick("block", &top, "z"), pick("block", &corner, "origin")] }), "two axes"),
        (json!({ "picks": [pick("block", &top, "z"), pick("block", &corner, "x")] }), "sets no axis"),
        (json!({ "picks": [pick("block", &top, "z"), pick("block", &front, "x"), pick("block", &front, "origin")] }), "sets an axis"),
        (json!({ "picks": [pick("block", &top, "z"), pick("block", &upright, "x")] }), "too near parallel"),
        (json!({ "picks": [pick("block", &top, "z"), pick("block", "face:0x0000000000000001", "x")] }), "no longer in the model"),
        (json!({ "picks": [pick("block", &top, "z"), pick("block", &front, "w")] }), "\"role\" must be"),
        (json!({ "picks": [pick("block", &top, "z"), pick("nope", &front, "x")] }), "no model named 'nope'"),
        (json!({ "picks": [pick("block", &top, "z"), { "model": "block", "id": front, "role": "x", "flip": true }] }), "not \"flip\""),
        (json!({ "picks": [pick("block", &top, "z"), pick("block", &front, "x")], "setup": "Back" }), "not the active setup"),
        (json!({ "picks": [pick("block", &top, "z"), pick("block", &front, "x")], "rotation": [0, 0, 0, 1] }), "unknown argument \"rotation\""),
    ];
    for (args, want) in cases {
        let msg = go(&mut c, args.clone());
        assert!(msg.contains(want), "{args} -> {msg} (wanted '{want}')");
    }
    assert_eq!(c.wcs()["placed"], json!(false), "no refusal placed the WCS");
    // An empty pick list is too few, not a malformed request.
    let msg = go(&mut c, json!({ "picks": [] }));
    assert!(msg.contains("two axes, plus an optional origin"), "{msg}");
}
