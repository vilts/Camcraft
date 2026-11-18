//! #2199 / #2180: a holder body beside (or touching the top of) the shank
//! must not widen the judged shank below the holder foot. Below the foot
//! the reach preflight, verify, fixture checks, and post gate judge the
//! stated shank diameter; from the foot upward the holder band still counts
//! and holds the post when it meets stock, part, or fixtures.
//!
//! Gate map (M4.4): covers tools::holder_profile, tools::neck,
//! sim::verify::body_bands, sim::verify::probe, sim::verify::reach,
//! app::operation::tool_gate, app::operation::generate::run_job,
//! app::fixture_struck.
//! Provenance: hand-built cavity, wall, and fixture test fixtures matching
//! issue #2199's T31 geometry and issue #2180's cone geometry.

mod area_support;

use area_support::*;
use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::commands::SetToolBody;
use camcraft_app::fixture::FixtureBox;
use camcraft_app::fixture_struck;
use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::spun::SpunPoint;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::verify::Warning;
use serde_json::{Value, json};

const T31: &str = "T31";
const FLUTE_LEN: f64 = 14.478;
const SHANK_D: f64 = 6.35;
const SHANK_TOP: f64 = 65.278;
const HOLDER_FOOT: f64 = 38.1;
const HOLDER_D: f64 = 35.0;

fn t31_tool() -> Tool {
    let base = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    Tool {
        diameter: 6.35,
        corner_r: 0.0,
        flute_len: FLUTE_LEN,
        shank_d: SHANK_D,
        shank_len: SHANK_TOP - FLUTE_LEN,
        holder_d: HOLDER_D,
        holder_len: 30.0,
        rpm: 10000.0,
        feed: 1200.0,
        center_cutting: true,
        ..base
    }
}

fn holder_cylinder(foot: f64, top: f64, radius: f64) -> Vec<SpunPoint> {
    vec![
        SpunPoint::line(foot, 0.0),
        SpunPoint::line(foot, radius),
        SpunPoint::line(top, radius),
        SpunPoint::line(top, 0.0),
    ]
}

fn cavity_block(depth: f64) -> Mesh {
    let p = Point3::new;
    let floor_z = 60.0 - depth;
    let outer = cube(p(-30.0, -30.0, 0.0), p(30.0, 30.0, 60.0));
    let inner = cube(p(-15.0, -15.0, floor_z), p(15.0, 15.0, 60.0));
    let floor = cube(p(-15.0, -15.0, 0.0), p(15.0, 15.0, floor_z));
    let mut facets = outer.facets[0..2].to_vec();
    facets.extend_from_slice(&outer.facets[4..12]);
    facets.extend_from_slice(&floor.facets[2..4]);
    facets.extend(inner.facets[4..12].iter().map(|t| Triangle::new(t.a, t.c, t.b)));
    let outer_ring = [(-30.0, -30.0), (30.0, -30.0), (30.0, 30.0), (-30.0, 30.0)];
    let inner_ring = [(-15.0, -15.0), (15.0, -15.0), (15.0, 15.0), (-15.0, 15.0)];
    for k in 0..4 {
        let n = (k + 1) % 4;
        let at = |(x, y)| p(x, y, 60.0);
        facets.push(Triangle::new(at(outer_ring[k]), at(outer_ring[n]), at(inner_ring[n])));
        facets.push(Triangle::new(at(outer_ring[k]), at(inner_ring[n]), at(inner_ring[k])));
    }
    Mesh::from_facets(facets)
}

fn run_pocket3d(tool: Tool, holder: Vec<SpunPoint>, depth: f64) -> Result<(Toolpath, Vec<Warning>), String> {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("cavity", cavity_block(depth)));
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 60.0 }, Location::default()).expect("stock"),
    );
    history.execute(&world, AddTool::new(&world, T31, tool).expect("tool"));
    history.execute(&world, SetToolBody::spun(&world, T31, Some(holder)).expect("holder"));
    let params = OpParams::Pocket3D {
        links: camcraft_app::links::Links::ROUGH3D,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(70.0),
        retract_z: None,
        top_z: Height::fixed(60.0),
        bottom_z: Height::fixed(60.0 - depth),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::OffsetRings,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: false,
    };
    let mut o = op(SrcRef::Entity("cavity".into()), params, AreaControls::default());
    o.tool = T31.into();
    operation::generate(&world, &o, "P1").map_err(|e| e.to_string())
}

fn write_box_stl(path: &std::path::Path, lo: [f64; 3], hi: [f64; 3]) {
    let v = |i: usize| -> [f64; 3] {
        [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ]
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let mut s = String::from("solid box\n");
    for q in quads {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            s.push_str(" facet normal 0 0 0\n  outer loop\n");
            for i in tri {
                let pt = v(i);
                s.push_str(&format!("   vertex {} {} {}\n", pt[0], pt[1], pt[2]));
            }
            s.push_str("  endloop\n endfacet\n");
        }
    }
    s.push_str("endsolid box\n");
    std::fs::write(path, s).expect("write stl");
}

struct McpClient {
    server: Server,
    next: i64,
}

impl McpClient {
    fn new() -> Self {
        let mut c = Self { server: Server::new(), next: 1 };
        c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
        c
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.server.handle(&msg).expect("response")
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
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

/// Contour a `wall_height` mm tall 40 x 40 box inside a 60 x 60 stock with
/// T31 + `holder` through MCP, then verify and post.
fn contour_wall_via_mcp(tag: &str, tool: Tool, holder: Vec<SpunPoint>, wall_height: f64) -> (Value, Result<Value, String>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("camcraft-2199-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let part = dir.join("part.stl");
    write_box_stl(&part, [-20.0, -20.0, 0.0], [20.0, 20.0, wall_height]);
    let mut c = McpClient::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 80.0, "dy": 80.0, "dz": wall_height } }));
    let holder_json: Vec<Value> = holder
        .into_iter()
        .map(|pt| json!({ "z": pt.z, "r": pt.r, "arc": pt.arc }))
        .collect();
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL", "name": T31, "holder": "none" }));
    c.ok(
        "edit_tool",
        json!({
            "name": T31,
            "patch": {
                "diameter": tool.diameter,
                "flute_len": tool.flute_len,
                "shank_d": tool.shank_d,
                "shank_len": tool.shank_len,
                "holder_d": tool.holder_d,
                "holder_len": tool.holder_len,
                "rpm": tool.rpm,
                "feed": tool.feed,
                "center_cutting": true
            }
        }),
    );
    c.ok("set_tool_holder", json!({ "tool": T31, "points": holder_json }));
    c.ok("add_operation", json!({ "kind": "contour", "tool": T31, "name": "C1", "source": "part" }));
    let generated = c.ok("generate", json!({}));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let ver = c.ok("verify", json!({}));
    let out = dir.join("out.nc");
    let (post_err, post_body) = c.call("post", json!({ "post": "Fanuc", "output": out.display().to_string() }));
    let post = if post_err {
        Err(post_body.as_str().unwrap_or_default().to_owned())
    } else {
        Ok(post_body)
    };
    (ver, post, dir)
}

#[test]
fn t31_holder_beside_shank_allows_3d_pocket_below_holder_foot_and_refuses_at_foot() {
    let holder = holder_cylinder(HOLDER_FOOT, 95.0, HOLDER_D * 0.5);
    // 25 mm deep: past the 14.478 mm flutes, but 13.1 mm clear of the 38.1 mm holder foot.
    let (path, warnings) = run_pocket3d(t31_tool(), holder.clone(), 25.0)
        .expect("shank is Ø6.35 below 38.1 mm so 25 mm depth must pass reach preflight");
    assert!(path.moves.iter().any(|m| m.kind == MoveKind::Feed), "expected cutting moves");
    assert!(!warnings.iter().any(Warning::blocks_post), "no blocking warning below holder foot: {warnings:?}");

    // 42 mm deep: >= 38.1 mm holder foot, so reach preflight must still refuse.
    let err = run_pocket3d(t31_tool(), holder, 42.0)
        .expect_err("depth >= 38.1 mm must refuse for the Ø35 holder foot");
    assert!(err.contains("3D Pocket") && err.contains(T31) && err.contains("38.100"), "{err}");
}

#[test]
fn t31_verify_judges_shank_at_6_35_below_38_1_and_catches_real_holder_hit() {
    let holder = holder_cylinder(HOLDER_FOOT, 95.0, HOLDER_D * 0.5);
    // 25 mm tall part wall: 25 mm < 38.1 mm holder foot. Cutter edge sits 3.175 mm off the wall;
    // the Ø6.35 shank clears it completely, whereas the widened Ø35 shank falsely collided.
    let (ver_ok, post_ok, dir_ok) = contour_wall_via_mcp("clear", t31_tool(), holder.clone(), 25.0);
    let _ = std::fs::remove_dir_all(&dir_ok);
    assert_eq!(ver_ok["ok"], json!(true), "25 mm wall below 38.1 mm holder foot must verify ok: {ver_ok}");
    assert_eq!(ver_ok["findings"], json!([]), "no false shank_hit below 38.1 mm: {ver_ok}");
    assert!(post_ok.is_ok(), "clean program must post: {post_ok:?}");

    // 42 mm tall part wall: >= 38.1 mm holder foot. The Ø35 holder collides with the wall.
    let (ver_hit, post_hit, dir_hit) = contour_wall_via_mcp("hit", t31_tool(), holder, 42.0);
    let _ = std::fs::remove_dir_all(&dir_hit);
    let findings = ver_hit["findings"].as_array().cloned().unwrap_or_default();
    assert!(
        findings.iter().any(|f| f["blocks_post"] == json!(true) && f["kind"] == json!("shank_hit")),
        "42 mm wall reaches the 38.1 mm holder and must emit shank_hit: {ver_hit}"
    );
    assert_eq!(ver_hit["ok"], json!(false), "holder collision must hold verify: {ver_hit}");
    assert!(post_hit.is_err(), "holder collision must hold post: {post_hit:?}");
}

#[test]
fn t31_fixture_law_judges_shank_below_holder_foot_and_catches_holder_beside_shank() {
    let world = new_world();
    let mut h = History::default();
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 60.0 }, Location::default()).expect("stock"),
    );
    h.execute(&world, AddTool::new(&world, T31, t31_tool()).expect("tool"));
    let holder = holder_cylinder(HOLDER_FOOT, 95.0, HOLDER_D * 0.5);
    h.execute(&world, SetToolBody::spun(&world, T31, Some(holder)).expect("holder"));
    h.execute(
        &world,
        CreateOperation::new(&world, "P1", T31, SrcRef::Stock, face(0.0)).expect("op"),
    );
    let op_ent = model::find_by_name(&world, "P1").expect("op");
    let op_val = model::operation_of(&world, &op_ent).expect("op");
    let path = Toolpath::new(vec![Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 0.0, 0.0), Some(500.0))]);

    // Fixture 10 mm off axis at 20..30 mm above tip (below 38.1 mm holder foot): Ø6.35 shank clears.
    let low_box = FixtureBox { min: Point3::new(5.0, 10.0, 20.0), max: Point3::new(15.0, 20.0, 30.0) };
    let low_hits = fixture_struck::op_hits(&world, &op_val, &path, &[], &[low_box], &[]).expect("judged");
    assert!(low_hits.is_empty(), "fixture 10 mm off axis below 38.1 mm clears Ø6.35 shank: {low_hits:?}");

    // Fixture 10 mm off axis at 42..52 mm above tip (above 38.1 mm, still below 65.278 mm shank top):
    // the Ø35 holder band standing beside the shank must hit.
    let mid_box = FixtureBox { min: Point3::new(5.0, 10.0, 42.0), max: Point3::new(15.0, 20.0, 52.0) };
    let mid_hits = fixture_struck::op_hits(&world, &op_val, &path, &[], &[mid_box], &[]).expect("judged");
    assert!(!mid_hits.is_empty(), "fixture 10 mm off axis at 42..52 mm hits Ø35 holder: {mid_hits:?}");
}

#[test]
fn issue_2180_cone_touching_only_shank_top_never_widens_shank() {
    // Cone holder from r 10 to r 30 starting exactly at SHANK_TOP (65.278 mm).
    let cone = vec![
        SpunPoint::line(SHANK_TOP, 0.0),
        SpunPoint::line(SHANK_TOP, 10.0),
        SpunPoint::line(SHANK_TOP + 25.0, 30.0),
        SpunPoint::line(SHANK_TOP + 25.0, 0.0),
    ];
    // 30 mm cavity is well below 65.278 mm shank top: must pass reach and verify clean.
    let (path, warnings) = run_pocket3d(t31_tool(), cone.clone(), 30.0)
        .expect("cone touching only shank_top must not widen the shank below shank_top");
    assert!(path.moves.iter().any(|m| m.kind == MoveKind::Feed));
    assert!(!warnings.iter().any(Warning::blocks_post), "{warnings:?}");

    // Contouring a 30 mm wall must verify clean (no false shank_hit).
    let (ver_ok, post_ok, dir_ok) = contour_wall_via_mcp("2180-clear", t31_tool(), cone.clone(), 30.0);
    let _ = std::fs::remove_dir_all(&dir_ok);
    assert_eq!(ver_ok["ok"], json!(true), "{ver_ok}");
    assert!(post_ok.is_ok(), "{post_ok:?}");

    // Contouring a 70 mm wall (> 65.278 mm shank top) hits the cone holder and holds the post.
    let (ver_hit, post_hit, dir_hit) = contour_wall_via_mcp("2180-hit", t31_tool(), cone, 70.0);
    let _ = std::fs::remove_dir_all(&dir_hit);
    assert_eq!(ver_hit["ok"], json!(false), "{ver_hit}");
    assert!(post_hit.is_err(), "{post_hit:?}");
}

#[test]
fn holder_narrower_than_stated_shank_never_shrinks_shank_judgement() {
    // Stated shank is Ø10 (r 5.0), holder starts at 38.1 with a narrow r 3.0 nose then r 17.5 flange.
    let tool = Tool { diameter: 6.0, shank_d: 10.0, ..t31_tool() };
    let stepped = vec![
        SpunPoint::line(HOLDER_FOOT, 0.0),
        SpunPoint::line(HOLDER_FOOT, 3.0),
        SpunPoint::line(50.0, 3.0),
        SpunPoint::line(50.0, 17.5),
        SpunPoint::line(95.0, 17.5),
        SpunPoint::line(95.0, 0.0),
    ];
    // At 42 mm tall wall, the Ø6 cutter trims 3 mm from the wall, so the Ø10 shank (r 5) rubs it even
    // though the holder nose at 38.1..50 is only r 3.0.
    let (ver_hit, post_hit, dir_hit) = contour_wall_via_mcp("narrow-holder", tool, stepped, 42.0);
    let _ = std::fs::remove_dir_all(&dir_hit);
    let findings = ver_hit["findings"].as_array().cloned().unwrap_or_default();
    assert!(
        findings.iter().any(|f| f["blocks_post"] == json!(true) && f["kind"] == json!("shank_hit")),
        "stated Ø10 shank must not shrink to the holder's r 3 nose: {ver_hit}"
    );
    assert_eq!(ver_hit["ok"], json!(false), "{ver_hit}");
    assert!(post_hit.is_err(), "{post_hit:?}");
}
