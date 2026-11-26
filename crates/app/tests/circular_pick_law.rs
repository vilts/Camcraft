//! #1550: a picked Circular is judged against the part it was picked
//! off (`DepthLaw::Section`, the one `authored_law` the Slot shares).
//! A ring typed below the picked floor is a blocking `Warning::Gouge`
//! on the operation itself - and the post is refused even where the
//! program-level stock sim cannot judge (a rotated stock), the case
//! that let the 3 mm overcut through before. A picked circle cut to its
//! floor stays clean.
//!
//! Provenance: our own fixtures (`pocket_support::block_with_pocket`)
//! over the depth-aware verify (the section and solid laws); no vendor
//! material consulted.
//!
//! Gate map: covers app::operation::generate (depth_law_of, authored_law,
//! Circular arm), app::posting, app::posting_sim (uncovered).

mod pocket_support;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::feature::FaceKind;
use camcraft_app::geometry::{Mesh, Point3};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, STOCK_NAME};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";
/// The round pocket's radius and its floor; the block is 40 x 40 x 8.
const R: f64 = 15.0;
const FLOOR: f64 = 4.0;

/// The 40 x 40 x 8 block with a radius-[`R`] round pocket (128 chords)
/// sunk to [`FLOOR`].
fn pocket_block() -> Mesh {
    let ring: Vec<(f64, f64)> = (0..128)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 128.0;
            (R * a.cos(), R * a.sin())
        })
        .collect();
    pocket_support::block_with_pocket(20.0, 20.0, &ring, FLOOR, 8.0)
}

/// [`pocket_block`] as "part", and its stock: axis-aligned, or the same box
/// turned [`pocket_support::OBLIQUE`] about Z (the same footprint within
/// 1e-4 mm, but a turn the program sim refuses to model). #2177: a
/// quarter turn is no such device - the sim judges a square-turned box.
fn job(rotated: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", pocket_block()));
    // A box stock stands centred on its place, so pos at the origin
    // puts [-20, 20]^2 over the part and a turn about Z keeps the
    // footprint.
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: pocket_support::stock_turn(rotated) };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 8.0 }, at).expect("stock"));
    let b = model::stock_world_aabb_for(&world, None).expect("stock box");
    // The oblique turn moves the corners by 20 * sqrt(2) * 1e-6 mm.
    let (on, on_z) = (|v: f64, want: f64| (v - want).abs() < 1e-4, |v: f64, want: f64| (v - want).abs() < 1e-9);
    assert!(
        on(b.min.x, -20.0) && on(b.min.y, -20.0) && on_z(b.min.z, 0.0)
            && on(b.max.x, 20.0) && on(b.max.y, 20.0) && on_z(b.max.z, 8.0),
        "stock at {:?}..{:?}",
        b.min,
        b.max
    );
    (world, h)
}

/// The pocket's floor face, picked.
fn floor_pick(world: &World) -> SrcRef {
    floor_pick_on(world, "part")
}

/// The pocket's floor face on entity `on`, picked.
fn floor_pick_on(world: &World, on: &str) -> SrcRef {
    let e = model::find_by_name(world, on).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar) && fa.normal[2] > 0.5)
        .find(|fa| (fa.offset - FLOOR).abs() < 1e-6)
        .map(|fa| fa.id)
        .expect("the floor face");
    SrcRef::Selection(vec![GeoSel { entity: on.into(), feature: FeatureSel::Face(id) }])
}

/// The circle the pick hands (the pocket's), 0.5 mm left, cut from the
/// top to `bottom`.
fn circular(bottom: f64) -> OpParams {
    OpParams::Circular {
        pass_start: Default::default(),
        links: camcraft_app::links::Links::PLANAR,
        center: Point2::new(0.0, 0.0),
        radius: R,
        vault: false,
        stepover: 2.0,
        stock_to_leave: 0.5,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(8.0),
        bottom_z: Height::fixed(bottom),
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

fn warnings_of(world: &World, h: &mut History, src: SrcRef, params: OpParams) -> Vec<Warning> {
    h.execute(world, CreateOperation::new(world, "C1", TOOL, src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "C1").expect("generate"));
    let e = model::find_by_name(world, "C1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "the rings were generated");
    tp.warnings.clone()
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

#[test]
fn a_picked_circle_cut_below_its_floor_gouges_and_never_posts_where_the_sim_cannot_judge() {
    // Bottom Z 1 under a floor at Z 4: the rings sink 3 mm into the
    // part. With the stock turned off the axes, the program sim refuses
    // to model it, so the operation's own verdict is the only judge -
    // before #1550 it was empty (law None) and the overcut could post.
    // #2177: a quarter turn is judged by the sim, so it is no device.
    let (world, mut h) = job(true);
    let src = floor_pick(&world);
    let warns = warnings_of(&world, &mut h, src, circular(1.0));
    assert!(gouges(&warns) > 0, "the floor is convicted on the op: {warns:?}");
    let nc = std::env::temp_dir().join(format!("camcraft-circular-pick-law-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&nc);
    let err = posting::post_setup(&world, &nc).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{err}");
    // Held by the operation's own gouge, not only by some other finding.
    assert!(err.contains("C1: move") && err.contains("cuts past the finish surface"), "{err}");
    assert!(pocket_support::oblique_refused(&world), "the device holds: the program sim does not judge this stock");
    assert!(!nc.exists(), "blocked output is never written");
}

#[test]
fn a_picked_circle_cut_to_its_floor_stays_clean() {
    for rotated in [false, true] {
        let (world, mut h) = job(rotated);
        let src = floor_pick(&world);
        let warns = warnings_of(&world, &mut h, src, circular(FLOOR));
        assert_eq!(gouges(&warns), 0, "a legal picked circle is not convicted (rotated {rotated}): {warns:?}");
    }
}

#[test]
fn a_circle_picked_off_the_stock_answers_for_the_part() {
    // The stock made from the part carries the same pocket; picking ITS
    // floor names the stock, the material to remove (`part_solid`
    // false) - but the part standing in it is kept (#1607): the rings
    // cut 3 mm below that floor enter the part, judged against the
    // setup's parts, never against the stock's own solid.
    let (world, mut h) = job(false);
    h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "part".into() }, Location::default()).expect("stock"));
    let src = floor_pick_on(&world, STOCK_NAME);
    let warns = warnings_of(&world, &mut h, src, circular(1.0));
    assert!(gouges(&warns) > 0, "the part in the stock is kept: {warns:?}");
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

/// The pocket job over MCP: a Circular on the part, the floor face
/// picked through `set_operation_geometry`, cut to `bottom`; verify's
/// reply.
fn mcp_verify(tag: &str, bottom: f64) -> Value {
    let dir = std::env::temp_dir().join(format!("camcraft-circular-pick-law-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("part.stl");
    pocket_support::write_stl(&pocket_block(), &stl);
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": stl.display().to_string() }));
    call(&mut s, &mut id, "set_stock", json!({ "from_model": "part" }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": TOOL }));
    let params = json!({ "center": [0.0, 0.0], "radius": R, "stock_to_leave": 0.5, "top_z": 8.0, "bottom_z": bottom });
    call(&mut s, &mut id, "add_operation", json!({ "kind": "Circular", "tool": TOOL, "source": "part", "name": "C1", "params": params }));
    let f = call(&mut s, &mut id, "list_features", json!({ "model": "part", "kinds": ["faces"] }));
    let floor: Vec<Value> = f["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .filter(|fa| {
            fa["facing"] == json!("up")
                && fa["bounds"]["max"][2].as_f64().is_some_and(|z| (z - FLOOR).abs() < 1e-6)
        })
        .map(|fa| json!({ "model": "part", "id": fa["id"] }))
        .collect();
    assert_eq!(floor.len(), 1, "the pocket floor: {f}");
    call(&mut s, &mut id, "set_operation_geometry", json!({ "operation": "C1", "picks": floor }));
    let g = call(&mut s, &mut id, "generate", json!({}));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
    let ver = call(&mut s, &mut id, "verify", json!({}));
    let _ = std::fs::remove_dir_all(&dir);
    ver
}

/// The operation's own blocking gouge findings (not the program gate's).
fn op_gouges(ver: &Value) -> usize {
    ver["findings"]
        .as_array()
        .map_or(0, |f| f.iter().filter(|x| x["kind"] == json!("gouge") && x["op"] == json!("C1") && x["blocks_post"] == json!(true)).count())
}

#[test]
fn over_mcp_the_picked_circle_below_its_floor_is_judged_the_same() {
    let bad = mcp_verify("below", 1.0);
    assert!(op_gouges(&bad) > 0, "the operation's own gouge blocks the post: {bad}");
    assert_eq!(bad["ok"], json!(false), "{bad}");
    let good = mcp_verify("floor", FLOOR);
    assert_eq!(op_gouges(&good), 0, "{good}");
    assert_eq!(good["ok"], json!(true), "a legal picked circle verifies ok: {good}");
}

#[test]
fn fixture_stock_covers_the_part_in_both_cases() {
    for rotated in [false, true] {
        let (world, _) = job(rotated);
        let b = model::stock_world_aabb_for(&world, None).expect("stock box");
        // The oblique turn moves the corners by 20 * sqrt(2) * 1e-6 mm.
        let (on, on_z) = (|v: f64, want: f64| (v - want).abs() < 1e-4, |v: f64, want: f64| (v - want).abs() < 1e-9);
        assert!(
            on(b.min.x, -20.0)
                && on(b.min.y, -20.0)
                && on_z(b.min.z, 0.0)
                && on(b.max.x, 20.0)
                && on(b.max.y, 20.0)
                && on_z(b.max.z, 8.0),
            "rotated {rotated}: stock at {:?}..{:?}",
            b.min,
            b.max
        );
    }
}
