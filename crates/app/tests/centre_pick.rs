//! #692: circle and arc centres as feature points - offered only to a
//! picker that takes points AND opts in to centres, resolved through
//! #759's `circle_of_run`, a straight edge refused by name, a hidden rim's
//! centre offered only when picking through, and the MCP point picks
//! reading a round edge's centre through the same service.
//!
//! Gate map (#692): covers app::pick::centre, app::pick::feature_pick
//! (centre tier), app::pick::feature_sel (centre selection),
//! app::pick::points, app::pick::axis, app::mcp::align_tools.
//!
//! Provenance: our own fixtures and assertions; no vendor material.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use camcraft_app::cl::ArcPlane;
use camcraft_app::commands::{CreateSetup, History, ImportModel};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, cylinder, transform_point};
use camcraft_app::mcp::Server;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{
    self, CircleRefusal, EdgeUnit, PickBody, PickKinds, PickQuery, PickRay, PickTarget, PlaneLay, PointsLay,
    Projection, centre_of, lay_on_points, pick_bodies,
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
const R: f64 = 4.0;

/// A `2*HALF` square plate, z 0..TOP, centred on (CX, CY), with a round
/// through hole of radius `R` in [`SEGS`] chords (watertight: every face
/// shares the hole's angular subdivision).
fn holed_plate() -> Mesh {
    let ang = |k: usize| std::f64::consts::TAU * (k % SEGS) as f64 / SEGS as f64;
    let outer = |k: usize, z: f64| {
        let (s, c) = ang(k).sin_cos();
        let m = HALF / c.abs().max(s.abs());
        Point3::new(CX + m * c, CY + m * s, z)
    };
    let inner = |k: usize, z: f64| {
        let (s, c) = ang(k).sin_cos();
        Point3::new(CX + R * c, CY + R * s, z)
    };
    let mut f = Vec::new();
    let mut quad = |a: Point3, b: Point3, c: Point3, d: Point3| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    for k in 0..SEGS {
        let n = k + 1;
        quad(outer(k, TOP), outer(n, TOP), inner(n, TOP), inner(k, TOP));
        quad(inner(k, 0.0), inner(n, 0.0), outer(n, 0.0), outer(k, 0.0));
        quad(outer(k, 0.0), outer(n, 0.0), outer(n, TOP), outer(k, TOP));
        quad(inner(k, TOP), inner(n, TOP), inner(n, 0.0), inner(k, 0.0));
    }
    Mesh::from_facets(f)
}

fn plate_world() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", holed_plate()));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    (world, h)
}

fn near(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol)
}

/// Straight down, ortho, 0.3 mm off the hole's axis.
fn down_the_hole(kinds: PickKinds) -> PickQuery {
    PickQuery {
        ray: PickRay { origin: [CX + 0.3, CY, 50.0], dir: [0.0, 0.0, -1.0], projection: Projection::Ortho },
        radius: 1.0,
        radius_per_depth: 0.0,
        kinds,
        through: false,
    }
}

const POINTS_AND_CENTRES: PickKinds = PickKinds { centres: true, ..PickKinds::POINTS };

/// The run (one straight side or one round rim) of `plate` whose every
/// vertex satisfies `on`, as a stored edge pick.
fn run_where(world: &World, on: impl Fn([f64; 3]) -> bool) -> GeoSel {
    let g = pick::graph_of(world, "plate").expect("graph");
    let run = g
        .runs()
        .iter()
        .find(|r| {
            r.edges.iter().all(|&e| g.edge_ends(e).is_some_and(|(a, b)| on(a) && on(b)))
                && r.edges.iter().all(|e| g.edges.binary_search(e).is_ok())
        })
        .map(|r| r.id)
        .expect("a run there");
    GeoSel { entity: "plate".into(), feature: FeatureSel::Chain(run) }
}

fn top_rim(p: [f64; 3]) -> bool {
    (p[2] - TOP).abs() < 1e-9 && ((p[0] - CX).hypot(p[1] - CY) - R).abs() < 1e-6
}

/// The plate's straight top side at y = CY - HALF.
fn top_front_side(p: [f64; 3]) -> bool {
    (p[2] - TOP).abs() < 1e-9 && (p[1] - (CY - HALF)).abs() < 1e-6
}

fn corner(world: &World, at: [f64; 3]) -> GeoSel {
    let g = pick::graph_of(world, "plate").expect("graph");
    let p = g.points.iter().find(|p| near(p.at, at, 1e-6)).expect("a corner there");
    GeoSel { entity: "plate".into(), feature: FeatureSel::Vertex(p.id) }
}

#[test]
fn a_round_edge_yields_its_centre_when_the_picker_opts_in() {
    let (world, _h) = plate_world();
    let (pick, sel) = pick::pick_selection(&world, &down_the_hole(POINTS_AND_CENTRES), EdgeUnit::Chain, |_| false)
        .expect("the rim's centre is under the cursor");
    assert!(matches!(pick.target, PickTarget::Centre { .. }), "{pick:?}");
    assert!(near(pick.at, [CX, CY, TOP], 1e-6), "the top rim (nearer the eye) wins: {pick:?}");
    // Stored as the round run it derives from, read back through #759.
    let feature = sel.expect("a centre stores its round run");
    assert!(matches!(feature, FeatureSel::Chain(_)), "{feature:?}");
    let stored = GeoSel { entity: pick.entity, feature };
    assert_eq!(stored, run_where(&world, top_rim), "the top rim's run");
    let c = centre_of(&world, &stored).expect("a round run has a centre");
    assert!(near(c, [CX, CY, TOP], 1e-6), "{c:?}");
}

#[test]
fn a_picker_that_does_not_opt_in_never_gets_a_centre() {
    let (world, _h) = plate_world();
    for kinds in [PickKinds::POINTS, PickKinds::ALL, PickKinds::EDGES, PickKinds { faces: true, ..PickKinds::EDGES }] {
        let got = pick::pick_selection(&world, &down_the_hole(kinds), EdgeUnit::Chain, |_| false);
        assert!(got.as_ref().is_none_or(|(p, _)| !matches!(p.target, PickTarget::Centre { .. })), "{kinds:?}: {got:?}");
    }
    // Over the rim itself an edge taker gets the edge, never its centre.
    let mut q = down_the_hole(PickKinds::ALL);
    q.ray.origin = [CX + R + 0.2, CY, 50.0];
    let (p, _) = pick::pick_selection(&world, &q, EdgeUnit::Chain, |_| false).expect("the rim");
    assert!(matches!(p.target, PickTarget::Edge { .. }), "{p:?}");
}

#[test]
fn a_straight_edge_is_refused_by_name() {
    let (world, _h) = plate_world();
    let side = run_where(&world, top_front_side);
    assert_eq!(centre_of(&world, &side), Err(CircleRefusal::Straight));
    let why = pick::centre_point(&world, &side).expect_err("no centre");
    assert!(why.contains("is an edge with no centre") && why.contains("straight"), "{why}");
    // A point slot (three points) refuses it by the same name.
    let picks = [corner(&world, [CX - HALF, CY + HALF, TOP]), corner(&world, [CX + HALF, CY + HALF, TOP]), side];
    let lay = PointsLay { lay: PlaneLay { plane: ArcPlane::Xy, flip: false, place: true }, frame: false };
    let e = lay_on_points(&world, &picks, lay).expect_err("refused");
    assert!(e.contains("straight"), "{e}");
}

#[test]
fn three_points_read_a_round_edge_as_its_centre() {
    let (world, _h) = plate_world();
    let picks = [
        run_where(&world, top_rim),
        corner(&world, [CX + HALF, CY + HALF, TOP]),
        corner(&world, [CX - HALF, CY + HALF, TOP]),
    ];
    let lay = PointsLay { lay: PlaneLay { plane: ArcPlane::Xy, flip: false, place: true }, frame: true };
    let held = lay_on_points(&world, &picks, lay).expect("a centre is a point");
    let o = transform_point(Point3::new(CX, CY, TOP), held.pos, held.rot);
    assert!(near([o.x, o.y, o.z], [0.0; 3], 1e-6), "3-2-1 puts point 1, the hole's centre, on the origin: {o:?}");
}

/// The readout's coordinates (`pick_coords`): a round edge reports its
/// centre, a straight one its two ends.
#[test]
fn pick_coords_reports_a_round_edge_as_its_centre() {
    let (world, _h) = plate_world();
    let c = pick::pick_coords(&world, &run_where(&world, top_rim)).expect("a round edge has coordinates");
    assert_eq!(c.len(), 1, "one point, the centre: {c:?}");
    assert!(near(c[0], [CX, CY, TOP], 1e-6), "{c:?}");
    let ends = pick::pick_coords(&world, &run_where(&world, top_front_side)).expect("a straight edge");
    assert_eq!(ends.len(), 2, "a straight edge reads as its ends: {ends:?}");
}

/// The three-points check refuses a straight edge by name before it
/// counts the picks: two picks, one a straight edge, name the edge - not
/// "2 picked".
#[test]
fn a_straight_edge_is_named_before_the_pick_count() {
    let (world, _h) = plate_world();
    let picks = [corner(&world, [CX + HALF, CY + HALF, TOP]), run_where(&world, top_front_side)];
    let lay = PointsLay { lay: PlaneLay { plane: ArcPlane::Xy, flip: false, place: true }, frame: false };
    let e = lay_on_points(&world, &picks, lay).expect_err("refused");
    assert!(e.contains("is an edge with no centre") && e.contains("straight"), "{e}");
}

/// A rod (radius 10, z 0..10) under a slab (z 12..20) that hides it from
/// above: neither rim is in sight.
fn hidden_rod() -> Vec<PickBody> {
    let slab = cube(Point3::new(-30.0, -30.0, 12.0), Point3::new(30.0, 30.0, 20.0)).facets.to_vec();
    let rod = cylinder(20.0, 10.0, 48).facets.to_vec();
    vec![
        PickBody::new("slab", Arc::from(slab), Location::default()),
        PickBody::new("rod", Arc::from(rod), Location::default()),
    ]
}

#[test]
fn a_hidden_round_edge_offers_its_centre_only_when_picking_through() {
    let bodies = hidden_rod();
    for projection in [Projection::Ortho, Projection::Perspective] {
        let mut q = PickQuery {
            ray: PickRay { origin: [0.3, 0.0, 60.0], dir: [0.0, 0.0, -1.0], projection },
            radius: 1.0,
            radius_per_depth: 0.0,
            kinds: PickKinds::CENTRES,
            through: false,
        };
        assert_eq!(pick_bodies(&bodies, &q), None, "a hidden rim offers no centre ({projection:?})");
        q.through = true;
        let p = pick_bodies(&bodies, &q).expect("picking through finds it");
        assert_eq!(p.entity, "rod");
        assert!(matches!(p.target, PickTarget::Centre { .. }), "{p:?}");
        assert!(near(p.at, [0.0, 0.0, 10.0], 1e-6), "the nearer (top) rim's centre: {p:?}");
    }
    // In sight - the slab gone - the same centre is offered without it.
    let q = PickQuery {
        ray: PickRay { origin: [0.3, 0.0, 60.0], dir: [0.0, 0.0, -1.0], projection: Projection::Ortho },
        radius: 1.0,
        radius_per_depth: 0.0,
        kinds: PickKinds::CENTRES,
        through: false,
    };
    let p = pick_bodies(&hidden_rod()[1..], &q).expect("a visible rim's centre");
    assert!(near(p.at, [0.0, 0.0, 10.0], 1e-6), "{p:?}");
}

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
    p.push(format!("camcraft-centre-pick-{}", std::process::id()));
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

#[test]
fn mcp_point_picks_take_a_round_edge_centre_and_refuse_a_straight_one_by_name() {
    let dir = tmp_dir();
    let path = dir.join("plate.stl");
    write_stl(&path, &holed_plate());
    let mut s = Server::new();
    s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }));
    let (err, _) = call(&mut s, "import_model", json!({ "path": path.display().to_string() }));
    assert!(!err);
    let (err, _) = call(&mut s, "add_setup", json!({}));
    assert!(!err);
    let (err, f) = call(&mut s, "list_features", json!({ "model": "plate", "kinds": ["chains", "vertices"] }));
    assert!(!err, "{f}");
    let chains = f["chains"].as_array().cloned().unwrap_or_default();
    let b3 = |c: &Value, k: &str| {
        let n = |i: usize| c["bounds"][k][i].as_f64().unwrap_or(f64::NAN);
        [n(0), n(1), n(2)]
    };
    let at_top = |c: &Value| (b3(c, "min")[2] - TOP).abs() < 1e-6 && (b3(c, "max")[2] - TOP).abs() < 1e-6;
    let rim = chains
        .iter()
        .find(|c| c["closed"] == json!(true) && at_top(c) && (b3(c, "max")[0] - b3(c, "min")[0] - 2.0 * R).abs() < 1e-6)
        .expect("the top rim chain");
    let side = chains
        .iter()
        .find(|c| at_top(c) && (b3(c, "min")[1] - (CY - HALF)).abs() < 1e-6 && (b3(c, "max")[1] - (CY - HALF)).abs() < 1e-6)
        .expect("a straight top side");
    let vertex = |at: [f64; 3]| -> Value {
        let id = f["vertices"]
            .as_array()
            .and_then(|vs| {
                vs.iter().find(|v| {
                    let n = |i: usize| v["at"][i].as_f64().unwrap_or(f64::NAN);
                    near([n(0), n(1), n(2)], at, 1e-6)
                })
            })
            .and_then(|v| v["id"].as_str())
            .expect("a vertex there");
        json!({ "model": "plate", "id": id })
    };
    let edge = |c: &Value| json!({ "model": "plate", "id": c["id"] });
    let a = vertex([CX + HALF, CY + HALF, TOP]);
    let b = vertex([CX - HALF, CY + HALF, TOP]);

    let (err, out) = call(&mut s, "lay_part_on_points", json!({ "points": [edge(rim), a, b], "frame": true }));
    assert!(!err, "a round edge is its centre: {out}");
    let (err, _) = call(&mut s, "undo", json!({}));
    assert!(!err);
    let (err, msg) = call(&mut s, "lay_part_on_points", json!({ "points": [edge(side), a, b] }));
    assert!(err && msg.as_str().is_some_and(|m| m.contains("straight") && m.contains("is an edge")), "{msg}");
    // The axis by two points takes the centre too, and refuses a straight edge.
    let (err, out) = call(&mut s, "set_part_axes", json!({ "y": [edge(rim), a] }));
    assert!(!err, "{out}");
    let (err, msg) = call(&mut s, "set_part_axes", json!({ "y": [edge(side), a] }));
    assert!(err && msg.as_str().is_some_and(|m| m.contains("straight")), "{msg}");
    let _ = std::fs::remove_dir_all(&dir);
}
