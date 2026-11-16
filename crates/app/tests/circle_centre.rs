//! #739: circle centre from a picked edge - the shared pick primitive, the
//! undoable WCS anchor command on it, and its MCP tool. A plate with a
//! drilled (tessellated) hole gives the hole's true centre, axis and
//! radius; a straight edge, an elliptical rim and a face are refused by
//! name; undo puts the previous origin back.
//!
//! Gate map (#739): covers app::pick::circle, app::commands::wcs_circle,
//! app::mcp::wcs_circle.
//!
//! Provenance: our own fixtures and assertions; no vendor material.

use std::path::{Path, PathBuf};

use camcraft_app::commands::{History, ImportModel, SetWcsAtCircleCentre, SetWcsOrigin};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{
    self, CircleRefusal, EdgeUnit, PickKinds, PickQuery, PickRay, Projection, circle_centre, circle_of_edge,
};
use camcraft_app::schema::Location;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

const TOP: f64 = 10.0;
const HALF: f64 = 20.0;
const SEGS: usize = 32;
const CX: f64 = 5.0;
const CY: f64 = -3.0;

/// A `2*HALF` square plate, z 0..TOP, centred on (CX, CY), with a through
/// hole of semi-axes `rx` x `ry` (a circle when equal) tessellated into
/// [`SEGS`] chords. Every face shares the hole's angular subdivision, so
/// the mesh is watertight with no T-junctions (the square's corners fall
/// on the 45-degree spokes).
fn holed_plate(rx: f64, ry: f64) -> Mesh {
    let ang = |k: usize| std::f64::consts::TAU * (k % SEGS) as f64 / SEGS as f64;
    let outer = |k: usize, z: f64| {
        let (s, c) = ang(k).sin_cos();
        let m = HALF / c.abs().max(s.abs());
        Point3::new(CX + m * c, CY + m * s, z)
    };
    let inner = |k: usize, z: f64| {
        let (s, c) = ang(k).sin_cos();
        Point3::new(CX + rx * c, CY + ry * s, z)
    };
    let mut f = Vec::new();
    let mut quad = |a: Point3, b: Point3, c: Point3, d: Point3| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    for k in 0..SEGS {
        let n = k + 1;
        quad(outer(k, TOP), outer(n, TOP), inner(n, TOP), inner(k, TOP)); // top, +Z
        quad(inner(k, 0.0), inner(n, 0.0), outer(n, 0.0), outer(k, 0.0)); // bottom, -Z
        quad(outer(k, 0.0), outer(n, 0.0), outer(n, TOP), outer(k, TOP)); // outer wall
        quad(inner(k, TOP), inner(n, TOP), inner(n, 0.0), inner(k, 0.0)); // bore wall
    }
    Mesh::from_facets(f)
}

fn world_with(mesh: Mesh) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("plate", mesh));
    (world, history)
}

fn near(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol)
}

/// The feature edge of `plate` whose both ends satisfy `on`.
fn edge_where(world: &World, on: impl Fn([f64; 3]) -> bool) -> u32 {
    let g = pick::graph_of(world, "plate").expect("the plate's graph");
    *g.edges
        .iter()
        .find(|&&e| g.edge_ends(e).is_some_and(|(a, b)| on(a) && on(b)))
        .expect("an edge there")
}

fn on_rim(r: f64) -> impl Fn([f64; 3]) -> bool {
    move |p| (p[2] - TOP).abs() < 1e-9 && ((p[0] - CX).hypot(p[1] - CY) - r).abs() < 1e-6
}

/// The run pick a click on the top rim makes, by the one pick rule.
fn rim_pick(world: &World, r: f64) -> GeoSel {
    let at = [CX + r, CY, TOP];
    let eye = [CX + r + 30.0, CY + 1.0, TOP + 40.0];
    let dir = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
    let q = PickQuery {
        ray: PickRay { origin: eye, dir, projection: Projection::Perspective },
        radius: 1.0,
        radius_per_depth: 0.0,
        kinds: PickKinds::EDGES,
        through: false,
    };
    let (found, sel) = pick::pick_selection(world, &q, EdgeUnit::Run, |_| false).expect("the rim is under the ray");
    GeoSel { entity: found.entity, feature: sel.expect("an edge pick names its run") }
}

#[test]
fn a_drilled_hole_rim_gives_its_true_centre_axis_and_radius() {
    let (world, _h) = world_with(holed_plate(4.0, 4.0));
    let g = pick::graph_of(&world, "plate").expect("graph");
    let c = circle_of_edge(&g, edge_where(&world, on_rim(4.0))).expect("the rim is a circle");
    assert!(near(c.centre, [CX, CY, TOP], 1e-6), "{c:?}");
    assert!((c.radius - 4.0).abs() < 1e-6 && (c.diameter() - 8.0).abs() < 1e-6, "{c:?}");
    assert!(near(c.axis, [0.0, 0.0, 1.0], 1e-9), "a hole in a top face reads +Z: {c:?}");
    assert!(c.closed, "a full rim");

    // The world pick, through a click and its stored run, in the frame the
    // viewport draws (a moved part moves the centre with it).
    let pick = rim_pick(&world, 4.0);
    let w = circle_centre(&world, &pick).expect("the picked rim");
    assert!(near(w.centre, [CX, CY, TOP], 1e-6), "{w:?}");
    let e = model::find_by_name(&world, "plate").expect("plate");
    model::set_transform(&world, &e, Location { pos: Point3::new(100.0, 0.0, -5.0), rot: IDENTITY_Q });
    let moved = circle_centre(&world, &pick).expect("still the rim");
    assert!(near(moved.centre, [CX + 100.0, CY, TOP - 5.0], 1e-6), "{moved:?}");
}

#[test]
fn a_straight_edge_is_refused_by_name() {
    let (world, _h) = world_with(holed_plate(4.0, 4.0));
    let g = pick::graph_of(&world, "plate").expect("graph");
    // The top face's front edge, y = CY - HALF.
    let edge = edge_where(&world, |p| (p[2] - TOP).abs() < 1e-9 && (p[1] - (CY - HALF)).abs() < 1e-9);
    let why = circle_of_edge(&g, edge).expect_err("a straight edge has no centre");
    assert_eq!(why, CircleRefusal::Straight);
    assert!(why.to_string().contains("straight"), "{why}");
}

#[test]
fn an_elliptical_rim_and_a_face_are_refused_never_guessed() {
    let (world, _h) = world_with(holed_plate(6.0, 3.0));
    let g = pick::graph_of(&world, "plate").expect("graph");
    let edge = edge_where(&world, |p| {
        let (u, v) = ((p[0] - CX) / 6.0, (p[1] - CY) / 3.0);
        (p[2] - TOP).abs() < 1e-9 && (u * u + v * v - 1.0).abs() < 1e-6
    });
    let why = circle_of_edge(&g, edge).expect_err("an ellipse is not a circle");
    assert_eq!(why, CircleRefusal::NotCircular, "{why}");

    let face = g.faces.iter().find_map(|f| f.sel).expect("a face key");
    let pick = GeoSel { entity: "plate".into(), feature: face };
    let why = circle_centre(&world, &pick).expect_err("a face is no edge");
    assert!(matches!(why, CircleRefusal::NotAnEdge(_)) && why.to_string().contains("not an edge"), "{why}");
    let stale = GeoSel { entity: "plate".into(), feature: FeatureSel::Chain(0xdead_beef) };
    assert!(matches!(circle_centre(&world, &stale), Err(CircleRefusal::Gone(_))));
}

fn g54(world: &World) -> [f64; 3] {
    let e = model::find_by_name(world, "G54").expect("G54 placed");
    let p = model::transform_of(world, &e).expect("placed").pos;
    [p.x, p.y, p.z]
}

#[test]
fn the_anchor_command_places_the_origin_and_undo_restores_the_previous_one() {
    let (world, mut history) = world_with(holed_plate(4.0, 4.0));
    let before = Location { pos: Point3::new(1.0, 2.0, 3.0), rot: IDENTITY_Q };
    history.execute(&world, SetWcsOrigin::new(&world, before).expect("a plain origin"));
    let pick = rim_pick(&world, 4.0);
    let cmd = SetWcsAtCircleCentre::new(&world, &pick, IDENTITY_Q).expect("the rim anchors");
    assert!((cmd.circle().radius - 4.0).abs() < 1e-6);
    history.execute(&world, cmd);
    assert!(near(g54(&world), [CX, CY, TOP], 1e-6), "{:?}", g54(&world));
    assert!(history.undo(&world), "one undo step");
    assert!(near(g54(&world), [1.0, 2.0, 3.0], 1e-12), "undo restores the previous origin");
    assert!(history.redo(&world));
    assert!(near(g54(&world), [CX, CY, TOP], 1e-6));

    // A refusal never builds a command, so nothing lands in the history.
    let g = pick::graph_of(&world, "plate").expect("graph");
    let face = g.faces.iter().find_map(|f| f.sel).expect("a face key");
    let refused = SetWcsAtCircleCentre::new(&world, &GeoSel { entity: "plate".into(), feature: face }, IDENTITY_Q);
    assert!(refused.is_err_and(|e| e.to_string().contains("not an edge")));
}

// ---- MCP ----------------------------------------------------------------

fn write_stl(path: &Path, mesh: &Mesh) {
    let mut s = String::from("solid plate\n");
    for t in mesh.facets.iter() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid plate\n");
    std::fs::write(path, s).expect("write stl");
}

fn tmp_dir() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-circle-centre-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

fn call(server: &mut Server, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a response");
    let r = &reply["result"];
    let err = r["isError"] == json!(true);
    (err, if err { r["content"][0]["text"].clone() } else { r["structuredContent"].clone() })
}

fn f3(v: &Value) -> [f64; 3] {
    let n = |i: usize| v[i].as_f64().unwrap_or(f64::NAN);
    [n(0), n(1), n(2)]
}

#[test]
fn mcp_set_wcs_circle_centre_anchors_on_a_rim_and_refuses_a_straight_edge() {
    let dir = tmp_dir();
    let path = dir.join("plate.stl");
    write_stl(&path, &holed_plate(4.0, 4.0));
    let mut s = Server::new();
    s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }));
    let (err, _) = call(&mut s, "import_model", json!({ "path": path.display().to_string() }));
    assert!(!err, "import");
    let (err, f) = call(&mut s, "list_features", json!({ "model": "plate", "kinds": ["chains"] }));
    assert!(!err, "{f}");
    let chains = f["chains"].as_array().cloned().unwrap_or_default();
    let flat = |c: &Value| c["bounds"]["max"][2].as_f64() == c["bounds"]["min"][2].as_f64();
    let span = |c: &Value| c["bounds"]["max"][0].as_f64().unwrap_or(0.0) - c["bounds"]["min"][0].as_f64().unwrap_or(0.0);
    let top = |c: &Value| c["bounds"]["max"][2].as_f64().unwrap_or(f64::NAN);
    let ztop = chains.iter().map(top).fold(f64::NEG_INFINITY, f64::max);
    let rim = chains
        .iter()
        .find(|c| c["closed"] == json!(true) && flat(c) && top(c) == ztop && (span(c) - 8.0).abs() < 1e-6)
        .expect("the top rim chain");
    let edge = |c: &Value| json!({ "model": "plate", "id": c["id"] });
    let (err, out) = call(&mut s, "set_wcs_circle_centre", json!({ "edge": edge(rim) }));
    assert!(!err, "{out}");
    let lo = f3(&rim["bounds"]["min"]);
    let hi = f3(&rim["bounds"]["max"]);
    let want = [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]), hi[2]];
    assert!(near(f3(&out["origin"]), want, 1e-6), "{out} vs {want:?}");
    assert!((out["circle"]["diameter"].as_f64().unwrap_or(0.0) - 8.0).abs() < 1e-6, "{out}");
    assert!(near(f3(&out["circle"]["axis"]), [0.0, 0.0, 1.0], 1e-9), "{out}");
    let (err, _) = call(&mut s, "undo", json!({}));
    assert!(!err);

    let straight = chains.iter().find(|c| c["closed"] == json!(false) && flat(c) && span(c) > 39.0).expect("a side");
    let (err, msg) = call(&mut s, "set_wcs_circle_centre", json!({ "edge": edge(straight) }));
    assert!(err && msg.as_str().is_some_and(|m| m.contains("straight")), "{msg}");
    let (err, msg) = call(&mut s, "set_wcs_circle_centre", json!({}));
    assert!(err && msg.as_str().is_some_and(|m| m.contains("\"edge\"")), "{msg}");
    let _ = std::fs::remove_dir_all(&dir);
}
