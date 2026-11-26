//! #2898: a Circular with several radial passes (R10 with a Ø6 cutter
//! and a 2 mm stepover: four rings) linked each ring's floor to the next
//! ring with one diagonal rapid through the core it had still to cut.
//! Generate convicted the rapids (`rapid move N travels through stock`)
//! and the post was held, so no program passed. These pin the issue's
//! job through generate, verify and post: no rapid is convicted and the
//! program posts, for the plunge and the ramp entry. The other half of
//! "Done" - a spliced unsafe link is still convicted - is pinned in
//! `crates/motion/tests/circular_ring_links.rs`. The helix entry still
//! meets the crossing law between rings (its own follow-up issue).
//!
//! Provenance: our own fixture (a 40 x 40 x 20 block with a R10.2
//! through hole, the issue's tool, steps and heights), specced from the
//! Prime Directive; no vendor material.
//!
//! Gate map: covers app::operation::generate (Circular arm, verify
//! wiring), app::posting (the gate), app::mcp (edit_operation), and
//! app::project (Circular parameters).
//!
//! #3359: use the same stock and tool to check the emitted flat/vault
//! passes, their inter-ring transfers, and the saved pass order.

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{OpParams, SrcRef};
use camcraft_app::{posting, project};
use camcraft_app::schema::{Location, Toolpath};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";
const N: usize = 128;
/// The modeled hole's radius: the issue's envelope.
const HOLE: f64 = 10.2;
const TOP: f64 = 20.0;
const CLEAR: f64 = 30.0;
const STEPDOWN: f64 = 2.0;
const EPS: f64 = 1e-6;

/// A 40 x 40 x [`TOP`] block about the origin with one round through
/// hole of radius [`HOLE`] (128 chords), outward-wound.
fn holed_block() -> Mesh {
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
        quad(rim(j, TOP), rim(i, TOP), outer(i, TOP), outer(j, TOP));
        quad(rim(i, TOP), rim(j, TOP), rim(j, 0.0), rim(i, 0.0));
        quad(outer(j, TOP), outer(i, TOP), outer(i, 0.0), outer(j, 0.0));
        quad(rim(i, 0.0), rim(j, 0.0), outer(j, 0.0), outer(i, 0.0));
    }
    Mesh::from_facets(f)
}

/// The block as the part, a full 40 x 40 x 20 box as the stock: the
/// hole's core is uncut stock when the Circular starts.
fn job() -> (World, History) {
    // Keep the 8 mm shank over the 20 mm cut.
    job_with_flutes(25.0)
}

fn job_with_flutes(flute_len: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let mut tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    tool.flute_len = flute_len;
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", holed_block()));
    let stock = StockShape::Box { dx: 40.0, dy: 40.0, dz: TOP };
    h.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    (world, h)
}

/// The issue's Circular: R10, stepover 2, stepdown 2, Z20 to Z0,
/// clearance Z30, nothing left on the wall.
fn circular(entry: &str, vault: bool, pass_start: Option<&str>) -> OpParams {
    let mut wire = json!({ "circular": {
        "center": [0.0, 0.0],
        "radius": 10.0,
        "vault": vault,
        "stepover": 2.0,
        "stock_to_leave": 0.0,
        "clearance_z": CLEAR,
        "top_z": TOP,
        "bottom_z": 0.0,
        "stepdown": STEPDOWN,
        "feed": 800.0,
        "plunge_feed": 200.0,
        "climb": true,
        "entry": entry,
        "helix_radius": 1.0,
    } });
    if let Some(start) = pass_start {
        wire["circular"]["pass_start"] = json!(start);
    }
    project::params_from_json(&wire).expect("Circular parameters")
}

fn generated(world: &World, h: &mut History, params: OpParams) -> Toolpath {
    h.execute(world, CreateOperation::new(world, "C1", TOOL, SrcRef::Entity("part".into()), params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "C1").expect("generate"));
    let e = model::find_by_name(world, "C1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "the rings were generated");
    tp
}

#[test]
fn a_multi_ring_circular_links_its_rings_over_the_stock_and_posts() {
    for entry in ["plunge", "ramp"] {
        let (world, mut h) = job();
        let path = generated(&world, &mut h, circular(entry, false, None));
        assert!(path.warnings.is_empty(), "{entry}: verification must find nothing: {:?}", path.warnings);
        let posted = posting::post_setup_unwritten(&world);
        assert!(posted.is_ok(), "{entry:?}: the program must post: {:?}", posted.err());
    }
}

fn call(server: &mut Server, name: &str, arguments: Value) -> Value {
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    });
    server.handle(&request).expect("MCP response")["result"].clone()
}

fn ok(server: &mut Server, name: &str, arguments: Value) -> Value {
    let reply = call(server, name, arguments);
    assert_eq!(reply["isError"], json!(false), "{name}: {reply}");
    reply["structuredContent"].clone()
}

#[test]
fn mcp_circular_pass_start_is_an_undoable_project_setting() {
    let mut server = Server::new();
    ok(&mut server, "set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": TOP } }));
    ok(&mut server, "add_tool", json!({ "catalog": TOOL }));
    let wire = project::params_to_json(&circular("plunge", false, None));
    ok(&mut server, "add_operation", json!({ "kind": "Circular", "tool": TOOL, "name": "C1", "params": wire["circular"] }));
    let job_start = |s: &mut Server| {
        ok(s, "get_job", json!({}))["operations"][0]["params"]["circular"].get("pass_start").cloned()
    };
    assert_eq!(job_start(&mut server), None, "a new Circular omits Auto");

    let edit = |s: &mut Server, start| call(s, "edit_operation", json!({
        "name": "C1", "params": { "pass_start": start }
    }));
    let inside = edit(&mut server, "inside");
    assert!(inside["isError"] == json!(false), "Circular pass_start edit failed: {inside}");
    assert_eq!(inside["structuredContent"]["params"]["circular"]["pass_start"], json!("inside"));
    let outside = edit(&mut server, "outside");
    assert_eq!(outside["isError"], json!(false), "{outside}");
    assert_eq!(job_start(&mut server), Some(json!("outside")));
    let bad = edit(&mut server, "sideways");
    assert_eq!(bad["isError"], json!(true), "{bad}");
    assert_eq!(job_start(&mut server), Some(json!("outside")), "a refused edit leaves the setting alone");

    let auto = edit(&mut server, "auto");
    assert_eq!(auto["isError"], json!(false), "{auto}");
    assert_eq!(job_start(&mut server), None, "explicit Auto returns to the omitted default");
    ok(&mut server, "undo", json!({}));
    assert_eq!(job_start(&mut server), Some(json!("outside")));
    ok(&mut server, "undo", json!({}));
    assert_eq!(job_start(&mut server), Some(json!("inside")));
    ok(&mut server, "redo", json!({}));
    assert_eq!(job_start(&mut server), Some(json!("outside")));
}

#[test]
fn circular_pass_start_reopens_from_project_text() {
    for start in [None, Some("auto"), Some("inside"), Some("outside")] {
        let (world, mut h) = job();
        let params = circular("plunge", false, start);
        h.execute(&world, CreateOperation::new(&world, "C1", TOOL, SrcRef::Entity("part".into()), params).expect("create"));
        let saved = project::save_text(&world).expect("save");
        let expected = start.filter(|choice| *choice != "auto").map(|choice| json!(choice));
        let reopened = new_world();
        project::open(&reopened, &saved).expect("reopen");
        let entity = model::find_by_name(&reopened, "C1").expect("reopened operation");
        let op = model::operation_of(&reopened, &entity).expect("operation definition");
        let readback = project::params_to_json(&op.params);
        assert_eq!(readback["circular"].get("pass_start"), expected.as_ref(), "{saved}");
        assert_eq!(saved.contains("\"pass_start\""), expected.is_some(), "{saved}");
    }
}

fn radius(p: Point3) -> f64 {
    p.x.hypot(p.y)
}

fn lateral(m: &Move) -> bool {
    (m.from.x - m.to.x).hypot(m.from.y - m.to.y) > EPS
}

/// The two planar arcs of each complete cut, with the emitted move
/// index for checking the transfers between them.
#[derive(Debug)]
struct Pass {
    first: usize,
    last: usize,
    radius: f64,
    z: f64,
}

fn passes(moves: &[Move]) -> Vec<Pass> {
    let arcs: Vec<_> = moves.iter().enumerate().filter(|(_, m)| {
        m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < EPS
            && m.arc.is_some_and(|a| a.center.x.hypot(a.center.y) < EPS)
    }).collect();
    let (pairs, remainder) = arcs.as_chunks::<2>();
    assert!(remainder.is_empty(), "a ring is missing its second arc");
    pairs.iter().map(|[(first, a), (last, b)]| {
        assert_eq!(*last, *first + 1, "the half-turn arcs must be consecutive");
        assert_eq!(a.to, b.from);
        assert!((a.from.x - b.to.x).hypot(a.from.y - b.to.y) < EPS, "the cut must close at its own start");
        assert!((a.from.z - b.to.z).abs() < EPS, "the cut must close at its own depth");
        assert!(a.arc.unwrap().ccw && b.arc.unwrap().ccw, "climb keeps its direction");
        assert!((radius(a.from) - radius(a.to)).abs() < EPS, "arcs must stay on their ring");
        assert!(
            (a.from.x + a.to.x).hypot(a.from.y + a.to.y) < EPS,
            "the first arc must reach the opposite side of the ring: {a:?}"
        );
        Pass { first: *first, last: *last, radius: radius(a.from), z: a.from.z }
    }).collect()
}

fn assert_clear_transfers(moves: &[Move], passes: &[Pass]) {
    for w in passes.windows(2).filter(|w| (w[0].radius - w[1].radius).abs() > EPS) {
        let link = &moves[w[0].last + 1..w[1].first];
        let lift = link.first().expect("a lift before moving to the next ring");
        assert_eq!(lift.kind, MoveKind::Rapid);
        assert!(!lateral(lift) && (lift.to.z - CLEAR).abs() < EPS, "lift straight to clearance: {lift:?}");
        let traverses: Vec<_> = link.iter().filter(|m| m.kind == MoveKind::Rapid && lateral(m)).collect();
        assert!(!traverses.is_empty(), "a clearance traverse to the next ring");
        assert!(
            traverses.iter().all(|m| m.from.z >= CLEAR - EPS && m.to.z >= CLEAR - EPS),
            "every lateral rapid between rings stays over clearance: {traverses:?}"
        );
    }
}

fn assert_first_entry(moves: &[Move], first: &Pass, entry: &str) {
    let descents: Vec<_> = moves[..first.first].iter()
        .filter(|m| m.kind != MoveKind::Rapid && m.from.z - m.to.z > EPS)
        .collect();
    assert!(!descents.is_empty(), "there must be cutting descent into the stock");
    match entry {
        "plunge" => assert!(descents.iter().all(|m| !lateral(m) && m.kind == MoveKind::Plunge), "{descents:?}"),
        "ramp" => assert!(descents.iter().any(|m| lateral(m)), "{descents:?}"),
        "helix" => assert!(descents.iter().any(|m| m.arc.is_some() && lateral(m)), "{descents:?}"),
        _ => panic!("unexpected entry in fixture: {entry}"),
    }
}

fn assert_passes(actual: &[Pass], expected: &[(f64, f64)]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (p, &(r, z)) in actual.iter().zip(expected) {
        assert!((p.radius - r).abs() < EPS && (p.z - z).abs() < EPS, "expected R{r} at Z{z}, found {p:?}");
    }
}

fn assert_posts(world: &World, path: &Toolpath) {
    assert!(!path.warnings.iter().any(|w| w.blocks_post()), "verification refused: {:?}", path.warnings);
    let posted = posting::post_setup_unwritten(world).expect("a verified Circular posts");
    assert_eq!(posted.ops, 1);
    assert_eq!(posted.not_judged, None, "the filled stock must be judged");
}

#[test]
fn circular_flat_pass_start_controls_every_ring_for_each_entry() {
    for entry in ["plunge", "ramp", "helix"] {
        for (start, radii) in [
            (None, [7.0, 5.0, 3.0, 1.0]),
            (Some("outside"), [7.0, 5.0, 3.0, 1.0]),
            (Some("inside"), [1.0, 3.0, 5.0, 7.0]),
        ] {
            let (world, mut h) = job();
            let t = generated(&world, &mut h, circular(entry, false, start));
            let moves = &t.path.moves;
            let actual = passes(moves);
            let expected: Vec<_> = radii.into_iter()
                .flat_map(|r| (0..10).rev().map(move |z| (r, STEPDOWN * f64::from(z))))
                .collect();
            assert_passes(&actual, &expected);
            assert_clear_transfers(moves, &actual);
            assert_first_entry(moves, &actual[0], entry);
            assert!(!t.warnings.iter().any(|w| w.blocks_post()), "{entry}/{start:?}: {:?}", t.warnings);
            if start == Some("inside") {
                assert_posts(&world, &t);
            }
        }
    }
}

fn bowl_at(radius: f64) -> f64 {
    TOP * (radius.min(10.0) / 10.0).powi(2)
}

#[test]
fn circular_vault_inside_cuts_full_ladders_to_the_existing_ring_floors() {
    for (start, entry, radii) in [
        (None, "plunge", [5.0, 3.0, 1.0]),
        (Some("outside"), "plunge", [5.0, 3.0, 1.0]),
        (Some("inside"), "plunge", [1.0, 3.0, 5.0]),
        (Some("inside"), "ramp", [1.0, 3.0, 5.0]),
        (Some("inside"), "helix", [1.0, 3.0, 5.0]),
    ] {
        let (world, mut h) = job();
        let t = generated(&world, &mut h, circular(entry, true, start));
        let moves = &t.path.moves;
        let actual = passes(moves);
        assert_clear_transfers(moves, &actual);
        let rings: Vec<_> = actual.chunk_by(|a, b| (a.radius - b.radius).abs() < EPS).collect();
        assert_eq!(rings.len(), radii.len(), "{actual:?}");
        for (ring, radius) in rings.into_iter().zip(radii) {
            let first = ring.first().expect("opening pass");
            let last = ring.last().expect("floor pass");
            assert!((first.radius - radius).abs() < EPS, "expected R{radius}, found {ring:?}");
            assert!((last.z - bowl_at(radius + 3.0)).abs() < EPS, "the ring must reach its band's floor: {ring:?}");
            if start == Some("inside") {
                // The full stock includes the axis and the material
                // outside every previously cut smaller ring.
                assert!((0.0..=STEPDOWN + EPS).contains(&(TOP - first.z)), "first cut into uncut stock: {first:?}");
            }
            for levels in ring.windows(2) {
                let step = levels[0].z - levels[1].z;
                assert!(step > EPS && step <= STEPDOWN + EPS, "no skipped level: {levels:?}");
            }
        }
        for m in moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
            for (a, b) in m.segments3(0.001) {
                for p in [a, b] {
                    assert!(p.z + EPS >= bowl_at(radius(p) + 3.0), "the cutter disk reaches below the vault: {m:?}");
                }
            }
        }
        assert!(!t.warnings.iter().any(|w| w.blocks_post()), "{entry}/{start:?}: {:?}", t.warnings);
        if start.is_none() {
            // Auto retains its former opening at the outer neighbor's floor.
            assert_passes(&actual, &[
                (5.0, 18.0), (5.0, 16.0), (5.0, 14.0), (5.0, 12.8),
                (3.0, 12.8), (3.0, 10.8), (3.0, 8.8), (3.0, 7.2),
                (1.0, 7.2), (1.0, 5.2), (1.0, 3.2),
            ]);
        } else if start == Some("inside") && entry == "plunge" {
            assert_posts(&world, &t);
        }
    }
}

#[test]
fn circular_inside_keeps_the_post_closed_on_a_shank_hit() {
    let (world, mut h) = job_with_flutes(15.0);
    let t = generated(&world, &mut h, circular("plunge", false, Some("inside")));
    assert!(
        t.warnings.iter().any(|w| matches!(w, camcraft_app::verify::Warning::ShankHit { .. })),
        "the 8 mm shank reaches beyond the outer ring's cut: {:?}", t.warnings
    );
    assert!(posting::post_setup_unwritten(&world).is_err(), "the shank hit holds the program");
}
