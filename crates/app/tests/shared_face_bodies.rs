//! #1407: `sample_ap214.step` laid flat - three solids, two of them
//! stacked so their cylindrical walls stand on the same lines - is
//! machined by 3D Finish and 3D Pocket as the union of its solids, and
//! the stock sim and verify find no gouge. Genuine non-manifold input is
//! still refused, now named as a model fault, not "invalid machining
//! parameters". The model as imported (axis horizontal) is untouched.
//! Its 3D Finish, in level bands, is in `shared_face_bodies_finish.rs`
//! (#1715).
//!
//! Gate map: covers app::operation::material (from_slice), app::mcp's
//! generate / verify, through camcraft_geometry::geometry::slice's union
//! fallback and camcraft_motion::machining3d (including the pocket ladder
//! and camcraft_motion::machining3d::rest).
//!
//! Provenance: the issue's own MCP reproduction; hand-built boxes for the
//! refusal (a closed shell welded to another along one vertical edge).

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

fn model(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// The issue's job: the sample, optionally laid flat (-90 deg about X),
/// box stock, a ball 3D Finish S1 and a flat 3D Pocket R1.
fn sample_job(laid_flat: bool) -> Client {
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": model("sample_ap214.step") }));
    if laid_flat {
        let h = std::f64::consts::FRAC_1_SQRT_2;
        c.ok(
            "move_model",
            json!({ "name": "sample_ap214", "position": [0.0, -26.7192, 1.5875], "rotation": [-h, 0.0, 0.0, h] }),
        );
        c.ok("set_stock", json!({ "box": { "dx": 70.0, "dy": 124.0, "dz": 8.0 } }));
    } else {
        c.ok("set_stock", json!({ "from_model": "sample_ap214" }));
    }
    c.ok("add_tool", json!({ "catalog": "BN-0600" }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    // Rough first, then finish: the program order a rest-aware 3D Pocket
    // expects (after the finish it would find nothing left to cut).
    c.ok("add_operation", json!({ "kind": "pocket3d", "tool": "EM-0600-4FL", "name": "R1", "source": "sample_ap214" }));
    c.ok("add_operation", json!({ "kind": "finish3d", "tool": "BN-0600", "name": "S1", "source": "sample_ap214" }));
    c
}

fn statuses(generated: &Value) -> Vec<(String, String, String)> {
    generated["generated"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|g| {
            let s = |k: &str| g[k].as_str().unwrap_or_default().to_owned();
            (s("name"), s("status"), g.to_string())
        })
        .collect()
}

fn gate_problems(v: &Value) -> Vec<String> {
    v["gate"]["problems"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

#[test]
fn the_sample_laid_flat_generates_3d_finish_and_pocket_with_no_gouge() {
    let mut c = sample_job(true);
    let generated = c.ok("generate", json!({}));
    let st = statuses(&generated);
    assert_eq!(st.len(), 2, "{generated}");
    for (name, status, row) in &st {
        assert_eq!(status, "ok", "{name} must generate: {row}");
        assert!(!row.contains("non-manifold") && !row.contains("machining parameters"), "{row}");
    }
    let v = c.ok("verify", json!({}));
    // The program's stock sim: nothing cut below the model.
    assert_eq!(v["gate"]["ok"], json!(true), "{v}");
    let problems = gate_problems(&v);
    assert!(problems.iter().all(|p| !p.starts_with("gouge")), "no gouge: {problems:?}\n{v}");
    // The finish envelope: no facet stands inside the swept ball past
    // the gouge tolerance. (Its rest half may hold the post: the steep
    // walls a parallel finish leaves - the same on the bodies that
    // never needed the union, so not this fix's concern.)
    for f in v["findings"].as_array().into_iter().flatten() {
        assert_ne!(f["kind"], json!("gouge"), "{f}");
        if f["kind"] == json!("envelope") {
            let msg = f["message"].as_str().unwrap_or_default();
            let mm: f64 = msg
                .split(": ")
                .nth(1)
                .and_then(|r| r.split(" mm gouge").next())
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(f64::INFINITY);
            assert!(mm < 0.01, "envelope gouge {mm} mm: {msg}");
        }
    }
}

/// The model as imported (cylinder axis horizontal) never meets a plane
/// 4-way, so it never reaches the union fallback: every plane through
/// its span sections on the plain read, exactly as before #1407.
#[test]
fn the_sample_as_imported_still_sections_on_the_plain_read() {
    let m = camcraft_app::geometry::import::load_model(std::path::Path::new(&model("sample_ap214.step")))
        .expect("the sample imports");
    let b = m.bounds().expect("bounds");
    let n = 240;
    for k in 0..n {
        let z = b.min.z + (f64::from(k) + 0.5) * (b.max.z - b.min.z) / f64::from(n);
        let r = camcraft_app::geometry::slice::slice(&m.facets, z);
        assert!(r.is_ok(), "z = {z}: {r:?}");
    }
}

/// #1706, read again by #1749: a 3D Pocket's machine area is the
/// model's own XY silhouette (`operation::material_source`, `Pocket3D`).
/// The sample as imported casts a strip only 6.53 mm wide (Y -4.94 ..
/// 1.59), so with the centre kept a radius inside it no 6 mm end mill
/// fit at any level and the op refused. Since #1749 the centre rides the
/// silhouette edge (the outside of a silhouette is air at every Z), so
/// the pocket clears the stock along the strip's sides - never a cut
/// into the part (no gouge).
///
/// #1844: the short tool is refused by name before the ladder runs.
/// A reaching tool still generates and verifies clean beside the silhouette.
#[test]
fn the_sample_as_imported_roughs_the_3d_pocket_beside_its_silhouette() {
    let mut c = sample_job(false);
    c.ok("delete_operation", json!({ "name": "S1" }));
    let (_, generated) = c.call("generate", json!({ "name": "R1" }));
    let text = generated.to_string();
    assert!(!text.contains("\"status\":\"ok\""), "the short tool must refuse at generation: {text}");
    assert!(text.contains("3D Pocket") && text.contains("shank") && text.contains("Bottom"), "name the reach and remedy: {text}");

    // Longer flutes remove the obstruction without weakening verify.
    c.ok("edit_tool", json!({ "name": "EM-0600-4FL", "patch": { "flute_len": 125.0, "shank_d": 6.0 } }));
    let generated = c.ok("generate", json!({ "name": "R1" }));
    assert!(statuses(&generated).iter().all(|(_, s, _)| s == "ok"), "{generated}");
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(true), "a reaching tool posts: {v}");
    assert_eq!(v["findings"], json!([]), "{v}");
}

/// Two blocks welded along one vertical edge, written as one ASCII STL:
/// one non-manifold shell. 3D Pocket refuses it by the model's fault.
#[test]
fn welded_non_manifold_solids_are_refused_as_a_model_fault() {
    let dir = std::env::temp_dir().join(format!("camcraft-1407-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("welded.stl");
    let mut stl = String::from("solid welded\n");
    let mut facets = Vec::new();
    for (x0, y0) in [(0.0, 0.0), (10.0, 10.0)] {
        let m = camcraft_app::geometry::cube(
            camcraft_app::geometry::Point3::new(x0, y0, 0.0),
            camcraft_app::geometry::Point3::new(x0 + 10.0, y0 + 10.0, 5.0),
        );
        facets.extend(m.facets.iter().copied());
    }
    for t in &facets {
        stl.push_str("facet normal 0 0 0\nouter loop\n");
        for p in [t.a, t.b, t.c] {
            stl.push_str(&format!("vertex {} {} {}\n", p.x, p.y, p.z));
        }
        stl.push_str("endloop\nendfacet\n");
    }
    stl.push_str("endsolid welded\n");
    std::fs::write(&path, stl).expect("write stl");
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": path.to_string_lossy(), "name": "welded" }));
    c.ok("set_stock", json!({ "box": { "dx": 30.0, "dy": 30.0, "dz": 6.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    c.ok("add_operation", json!({ "kind": "pocket3d", "tool": "EM-0600-4FL", "name": "R1", "source": "welded" }));
    let (err, result) = c.call("generate", json!({ "name": "R1" }));
    let text = result.to_string();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(err || text.contains("\"status\":\"error\"") || !text.contains("\"status\":\"ok\""), "{text}");
    assert!(text.contains("the source model cannot be sliced") && text.contains("non-manifold"), "{text}");
    assert!(!text.contains("invalid machining parameters"), "a model fault, not parameters: {text}");
}

/// The band the issue found refused (z 1.60 .. 3.15, where the two
/// stacked solids share their wall lines): every plane now sections, as
/// the union - wider than the lower solid's section under the band - and
/// the 3-D ladder's islands law reads it too.
#[test]
fn the_laid_flat_band_sections_as_the_union_of_the_solids() {
    use camcraft_app::geometry::{Point3, Triangle};
    let m = camcraft_app::geometry::import::load_model(std::path::Path::new(&model("sample_ap214.step")))
        .expect("the sample imports");
    // -90 deg about X, then the issue's position.
    let lay = |q: Point3| Point3::new(q.x, q.z - 26.7192, -q.y + 1.5875);
    let flat: Vec<Triangle> = m.facets.iter().map(|t| Triangle::new(lay(t.a), lay(t.b), lay(t.c))).collect();
    let below = camcraft_app::geometry::slice::slice(&flat, 1.0).expect("under the band").area();
    let mut z = 1.6;
    while z < 3.15 {
        assert!(camcraft_app::machining3d::material_above(&flat, z).is_ok(), "islands at z = {z}");
        let r = camcraft_app::geometry::slice::slice(&flat, z);
        let area = r.as_ref().map(|r| r.area()).unwrap_or(0.0);
        assert!(r.is_ok() && area > below, "z = {z}: {:?} (below the band {below})", r.map(|r| r.area()));
        z += 0.05;
    }
}
