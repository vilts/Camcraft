//! #1558: a Circular ring posts as true arcs (two half turns, G3 for
//! climb), not a fixed 512-chord polygon - one Ø6 mm hole was 74,131
//! moves. Pins the move count of the issue's repro (and each of its
//! parameter rows) under a ceiling per ring and level, and that the
//! judged path still verifies clean inside the modeled hole.
//!
//! Provenance: our own tests over the circular generator and the
//! verify; no vendor material consulted.
//!
//! Gate map: covers app::operation::generate (Circular arm),
//! camcraft_motion::machining2d::circular.

mod pocket_support;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{seed_params, KindInfo, OpParams, SrcRef};
use camcraft_app::project::{params_from_json, params_to_json};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";
const N: usize = 256;
/// The modeled through hole's radius (the issue's Ø6), about the origin.
const HOLE: f64 = 3.0;

/// A 40 x 40 x 10 plate on the origin with one Ø6 through hole.
fn plate() -> Mesh {
    let ang = |i: usize| std::f64::consts::TAU * (i % N) as f64 / N as f64;
    let rim = |i: usize, z: f64| Point3::new(HOLE * ang(i).cos(), HOLE * ang(i).sin(), z);
    let outer = |i: usize, z: f64| {
        let (x, y) = (ang(i).cos(), ang(i).sin());
        let s = x.abs().max(y.abs());
        Point3::new(x * 20.0 / s, y * 20.0 / s, z)
    };
    let mut f = Vec::new();
    let mut quad = |a: Point3, b: Point3, c: Point3, d: Point3| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    for i in 0..N {
        let j = i + 1;
        quad(rim(j, 10.0), rim(i, 10.0), outer(i, 10.0), outer(j, 10.0));
        quad(rim(i, 10.0), rim(j, 10.0), rim(j, 0.0), rim(i, 0.0));
        quad(outer(j, 10.0), outer(i, 10.0), outer(i, 0.0), outer(j, 0.0));
        quad(rim(i, 0.0), rim(j, 0.0), outer(j, 0.0), outer(i, 0.0));
    }
    Mesh::from_facets(f)
}

fn job() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", plate()));
    let at = Location { pos: Point3::new(-20.0, -20.0, 0.0), ..Location::default() };
    let stock = StockShape::Box { dx: 40.0, dy: 40.0, dz: 10.508 };
    h.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    (world, h)
}

/// The issue's repro params (its MCP `add_operation` patch, in mm),
/// with `over` merged on top - the seeded Circular, patched as the MCP
/// patches it.
fn repro(over: &Value) -> OpParams {
    let kind = KindInfo::ALL.iter().position(|k| k.label == "Circular").expect("Circular kind");
    let Value::Object(tagged) = params_to_json(&seed_params(kind)) else { panic!("seed json") };
    let (key, mut fields) = tagged.into_iter().next().expect("one kind key");
    let base = json!({
        "center": [0.0, 0.0], "radius": 3.0, "stepover": 0.5,
        "top_z": {"reference": "top_of_stock", "offset": -0.508},
        "bottom_z": {"reference": "top_of_stock", "offset": -11.524},
        "stepdown": 1.0, "feed": 500.0, "plunge_feed": 250.0, "climb": true,
        "entry": "helix", "helix_radius": 1.0
    });
    for patch in [&base, over] {
        for (k, v) in patch.as_object().expect("patch object") {
            fields[k] = v.clone();
        }
    }
    params_from_json(&json!({ key: fields })).expect("repro params")
}

fn run(over: &Value) -> (Toolpath, Vec<Warning>) {
    let (world, mut h) = job();
    let src = SrcRef::Entity("part".into());
    h.execute(&world, CreateOperation::new(&world, "c", TOOL, src, repro(over)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "c").expect("generate"));
    let e = model::find_by_name(&world, "c").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    (tp.path.clone(), tp.warnings.clone())
}

struct Count {
    moves: usize,
    rapids: usize,
    arcs: usize,
}

fn count(p: &Toolpath) -> Count {
    Count {
        moves: p.moves.len(),
        rapids: p.moves.iter().filter(|m| m.kind == MoveKind::Rapid).count(),
        arcs: p.moves.iter().filter(|m| m.arc.is_some()).count(),
    }
}

/// The issue's table, one row per variation (its own labels).
fn rows() -> Vec<(&'static str, Value)> {
    vec![
        ("repro (helix R1, stepover 0.5)", json!({})),
        ("entry: plunge", json!({"entry": "plunge"})),
        ("helix_radius: 0.5", json!({"helix_radius": 0.5})),
        ("stepover: 1.0", json!({"stepover": 1.0})),
        ("stepover: 2.0", json!({"stepover": 2.0})),
        ("radius: 4.0", json!({"radius": 4.0})),
        ("radius: 6.0", json!({"radius": 6.0})),
        ("stepdown: 2.0", json!({"stepdown": 2.0})),
    ]
}

/// Every row of the issue's table, regenerated: the in-hole rows post a
/// few hundred moves (the 512-gon took 13,416 to 74,131) with their
/// rings as arcs and verify exactly as before; the rows that cut past
/// the modeled Ø6 wall (radius 4 and 6) are still gouges that hold the
/// post - the arcs are judged, not waved through.
#[test]
fn the_issue_table_posts_arcs_and_verifies_the_same() {
    // (row, ceiling on moves, judged clean). The 512-gon counts, same
    // fixture: 74131, 13416, 13446, 26937, 26937, 26858, 53690, 67989.
    let ceilings = [500, 200, 250, 200, 200, 400, 700, 400];
    for ((label, over), cap) in rows().into_iter().zip(ceilings) {
        let (p, w) = run(&over);
        let c = count(&p);
        println!("ROW | {label} | {} | {} rapids | {} arcs | {} warnings", c.moves, c.rapids, c.arcs, w.len());
        assert!(c.moves <= cap, "{label}: {} moves over {cap}", c.moves);
        // Every ring-level is two half-turn arcs at least.
        assert!(c.arcs >= 2 * 13, "{label}: {} arcs", c.arcs);
        let gouges = w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count();
        let outside = over.get("radius").is_some();
        assert_eq!(gouges > 0, outside, "{label}: {gouges} gouges");
        if !outside && over.get("helix_radius").is_none() {
            assert!(w.is_empty(), "{label}: {w:?}");
        }
    }
}
