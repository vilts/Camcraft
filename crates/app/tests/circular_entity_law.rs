//! #1543: a Circular whose source is the finished part (a whole model,
//! `SrcRef::Entity`) is judged against that part. A circle typed through
//! the hole's wall is a blocking `Warning::Gouge` and the post is held;
//! a circle inside the modeled hole stays clean; a Stock source answers
//! for the part standing in it the same way (#1607) (a picked Circular is judged too:
//! `circular_pick_law.rs`, #1550). The same job driven over MCP is
//! judged the same.
//!
//! Provenance: our own tests over the depth-aware verify (the section
//! and solid laws); no vendor material consulted.
//!
//! Gate map: covers app::operation::generate (depth_law_of, authored_law,
//! Circular arm), app::posting, app::mcp (add_operation, generate, verify).

mod pocket_support;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";
const N: usize = 128;
/// The modeled through hole's radius, about the block's centre.
const HOLE: f64 = 10.0;

/// A 40 x 40 x 8 block on the origin with one round through hole of
/// radius [`HOLE`] (128 chords), outward-wound.
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
        quad(rim(j, 8.0), rim(i, 8.0), outer(i, 8.0), outer(j, 8.0));
        quad(rim(i, 8.0), rim(j, 8.0), rim(j, 0.0), rim(i, 0.0));
        quad(outer(j, 8.0), outer(i, 8.0), outer(i, 0.0), outer(j, 0.0));
        quad(rim(i, 0.0), rim(j, 0.0), outer(j, 0.0), outer(i, 0.0));
    }
    Mesh::from_facets(f)
}

fn job() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", holed_block()));
    // A box stock stands centred on its place: pos at the origin puts
    // [-20, 20]^2 over the part.
    let at = Location::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 8.0 }, at).expect("stock"));
    let b = model::stock_world_aabb_for(&world, None).expect("stock box");
    let on = |v: f64, want: f64| (v - want).abs() < 1e-9;
    assert!(
        on(b.min.x, -20.0) && on(b.min.y, -20.0) && on(b.min.z, 0.0)
            && on(b.max.x, 20.0) && on(b.max.y, 20.0) && on(b.max.z, 8.0),
        "stock at {:?}..{:?}",
        b.min,
        b.max
    );
    (world, h)
}

/// The circle about the hole's centre, `radius` typed, 0.5 mm left on
/// the wall, cut from the top to Z 1.
fn circular(radius: f64) -> OpParams {
    OpParams::Circular {
        pass_start: Default::default(),
        links: camcraft_app::links::Links::PLANAR,
        center: Point2::new(0.0, 0.0),
        radius,
        vault: false,
        stepover: 2.0,
        stock_to_leave: 0.5,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(8.0),
        bottom_z: Height::fixed(1.0),
        stepdown: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
        picks: Default::default(),
    }
}

fn run(world: &World, h: &mut History, name: &str, src: SrcRef, params: OpParams) -> (Toolpath, Vec<Warning>) {
    h.execute(world, CreateOperation::new(world, name, TOOL, src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    (tp.path.clone(), tp.warnings.clone())
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

fn assert_blocked(world: &World, tag: &str) {
    let nc = std::env::temp_dir().join(format!("camcraft-circular-entity-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&nc);
    let err = posting::post_setup(world, &nc).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{err}");
    assert!(!nc.exists(), "blocked output is never written");
}

#[test]
fn a_whole_model_circle_typed_through_the_hole_wall_is_a_gouge_and_never_posts() {
    // Radius 12 in a radius-10 hole: the outer ring's tool edge stands
    // at 12 - 0.5 = 11.5, 1.5 mm into the wall. Before #1543 this ran
    // with no warning at all and posted.
    let (world, mut h) = job();
    let (path, warns) = run(&world, &mut h, "C1", SrcRef::Entity("part".into()), circular(12.0));
    assert!(!path.moves.is_empty(), "the rings were generated");
    assert!(gouges(&warns) > 0, "the wall is convicted: {warns:?}");
    assert_blocked(&world, "wall");
}

#[test]
fn a_whole_model_circle_inside_the_modeled_hole_stays_clean() {
    // Radius 10 with 0.5 left: the tool edge stands 0.5 off the wall.
    let (world, mut h) = job();
    let (path, warns) = run(&world, &mut h, "C2", SrcRef::Entity("part".into()), circular(HOLE));
    assert!(!path.moves.is_empty(), "the rings were generated");
    assert_eq!(gouges(&warns), 0, "a legal circle is not convicted: {warns:?}");
    // The law only judges: the path is the Stock source's, move for move.
    let (world2, mut h2) = job();
    let (stock_path, _) = run(&world2, &mut h2, "C2", SrcRef::Stock, circular(HOLE));
    assert_eq!(path, stock_path, "judging never moves the tool");
}

#[test]
fn a_stock_source_circle_through_the_hole_wall_answers_for_the_part() {
    // The stock IS the material to remove, but the part standing in it
    // is kept (#1607, `keep_law::parts_in_stock`): the same circle
    // through the hole's wall is convicted off a Stock source too.
    let (world, mut h) = job();
    let (path, warns) = run(&world, &mut h, "C3", SrcRef::Stock, circular(12.0));
    assert!(!path.moves.is_empty(), "the rings were generated");
    assert!(gouges(&warns) > 0, "the wall is convicted: {warns:?}");
    assert_blocked(&world, "stock-wall");
}

// ---- MCP parity -----------------------------------------------------

fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    assert_ne!(result["isError"], json!(true), "{name} refused: {}", result["content"][0]["text"]);
    result["structuredContent"].clone()
}

/// The same job over MCP: the part as its source, `radius` typed, the
/// stock fitted to the part. Returns (generate's reply, verify's): the
/// findings ride verify, generate's `status` is freshness only.
fn mcp_job(tag: &str, radius: f64) -> (Value, Value) {
    let dir = std::env::temp_dir().join(format!("camcraft-circular-entity-mcp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("part.stl");
    pocket_support::write_stl(&holed_block(), &stl);
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": stl.display().to_string() }));
    call(&mut s, &mut id, "set_stock", json!({ "from_model": "part" }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": TOOL }));
    let params = json!({ "center": [0.0, 0.0], "radius": radius, "stock_to_leave": 0.5 });
    call(&mut s, &mut id, "add_operation", json!({ "kind": "Circular", "tool": TOOL, "source": "part", "name": "C1", "params": params }));
    let reply = call(&mut s, &mut id, "generate", json!({}));
    let ver = call(&mut s, &mut id, "verify", json!({}));
    let _ = std::fs::remove_dir_all(&dir);
    (reply, ver)
}

/// The operation's own blocking gouge findings (not the program gate's).
fn op_gouges(ver: &Value) -> usize {
    ver["findings"]
        .as_array()
        .map_or(0, |f| f.iter().filter(|x| x["kind"] == json!("gouge") && x["op"] == json!("C1") && x["blocks_post"] == json!(true)).count())
}

#[test]
fn over_mcp_the_whole_model_circle_is_judged_the_same() {
    let (reply, bad) = mcp_job("wall", 12.0);
    assert_eq!(reply["generated"][0]["status"], json!("ok"), "generated (status is freshness): {reply}");
    assert!(op_gouges(&bad) > 0, "the operation's own gouge blocks the post: {bad}");
    assert_eq!(bad["ok"], json!(false), "a gouging circle never verifies ok");
    assert_eq!(bad["held_by"], json!(["C1"]), "the circle holds the post");
    let (reply, good) = mcp_job("inside", HOLE);
    assert_eq!(reply["generated"][0]["status"], json!("ok"), "{reply}");
    assert_eq!(op_gouges(&good), 0, "{good}");
    assert_eq!(good["ok"], json!(true), "a legal circle verifies ok: {good}");
}

#[test]
fn fixture_stock_covers_the_part() {
    let (world, _) = job();
    let b = model::stock_world_aabb_for(&world, None).expect("stock box");
    let on = |v: f64, want: f64| (v - want).abs() < 1e-9;
    assert!(
        on(b.min.x, -20.0)
            && on(b.min.y, -20.0)
            && on(b.min.z, 0.0)
            && on(b.max.x, 20.0)
            && on(b.max.y, 20.0)
            && on(b.max.z, 8.0),
        "stock at {:?}..{:?}",
        b.min,
        b.max
    );
}
