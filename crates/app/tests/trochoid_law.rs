//! #1593: a Trochoidal slot on the finished part is judged against it
//! (`DepthLaw::Section`, the one `authored_law` the Slot and the
//! Circular share). Its endpoints and width are typed by hand, so
//! nothing says the channel stays inside the part's own pocket or above
//! its floor: a whole-model slot typed through the pocket wall, or sunk
//! under the floor, is a blocking `Warning::Gouge` on the operation and
//! the post is refused by name on its moves - with the stock aligned and
//! with it turned (where the program sim refuses to judge). A legal
//! slot stays clean and posts; a source that is not the part (the
//! stock) is never judged.
//!
//! Provenance: our own fixtures (`pocket_support::block_with_pocket`)
//! over the depth-aware verify (the section and solid laws); no vendor
//! material consulted.
//!
//! Gate map: covers app::operation::generate (depth_law_of, authored_law,
//! TrochoidalSlot arm), app::posting, app::posting_sim (uncovered),
//! app::mcp (add_operation, verify).

mod pocket_support;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::feature::FaceKind;
use camcraft_app::geometry::{Mesh, Point3, quat_from_yaw};
use camcraft_app::machining2d::TrochoidReturn;
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

/// [`pocket_block`] as "part", and its stock: axis-aligned, or the same
/// box turned a quarter about Z (the same footprint, but a rotation the
/// program sim refuses to model).
fn job(rotated: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", pocket_block()));
    // A box stock stands centred on its place, so pos at the origin
    // puts [-20, 20]^2 over the part and a quarter turn about Z keeps
    // the footprint.
    let rot = if rotated { quat_from_yaw(std::f64::consts::FRAC_PI_2) } else { Location::default().rot };
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot };
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

/// A 10 mm channel along X from `a` to `b`, cut from the top to `bottom`.
fn trochoid(a: f64, b: f64, bottom: f64) -> OpParams {
    OpParams::TrochoidalSlot {
        a: Point2::new(a, 0.0),
        b: Point2::new(b, 0.0),
        width: 10.0,
        loop_width: None,
        step: 0.8,
        max_engagement: 90.0,
        ret: TrochoidReturn::Microlift { height: 0.3 },
        smoothing: 0.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Some(Height::fixed(15.0)),
        top_z: Height::fixed(8.0),
        bottom_z: Height::fixed(bottom),
        stepdown: 4.0,
        feed: 800.0,
        return_feed: 2000.0,
        plunge_feed: 200.0,
        climb: true,
    }
}

/// The review's through-wall case: x 4..20, 10 wide, at the floor - the
/// cutter reaches x 25, through the pocket's wall at R 15.
fn through_wall() -> OpParams {
    trochoid(4.0, 20.0, FLOOR)
}

/// The review's under-floor case: inside the pocket, 3 mm below its floor.
fn under_floor() -> OpParams {
    trochoid(-8.0, 8.0, 1.0)
}

/// Inside the pocket (x +-11, y +-5 against R 15), cut to the floor.
fn legal() -> OpParams {
    trochoid(-6.0, 6.0, FLOOR)
}

fn warnings_of(world: &World, h: &mut History, src: SrcRef, params: OpParams) -> Vec<Warning> {
    h.execute(world, CreateOperation::new(world, "T1", TOOL, src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "T1").expect("generate"));
    let e = model::find_by_name(world, "T1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "the loops were generated");
    tp.warnings.clone()
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-trochoid-law-{tag}-{}.nc", std::process::id()))
}

/// The post is refused, and nothing is written. `by_op`: the refusal
/// names the operation's own gouge (where the program sim judges first,
/// its clause may be the one the message leads with).
fn assert_post_refused(world: &World, tag: &str, by_op: bool) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(world, &out).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{tag}: {err}");
    if by_op {
        // Held by the operation's own gouge, not only by the program sim.
        assert!(err.contains("T1: move") && err.contains("cuts past the finish surface"), "{tag}: {err}");
    }
    assert!(!out.exists(), "{tag}: blocked output is never written");
}

#[test]
fn a_whole_model_trochoid_through_the_pocket_wall_gouges_and_never_posts() {
    // Aligned, the program sim sees the cut past R 15 and its clause
    // leads the refusal; turned, the program sim refuses to model it,
    // so only the op's own gouge holds it (before #1593, with the
    // turned stock and without DepthLaw::Section, it generated clean
    // and posted).
    for rotated in [false, true] {
        let (world, mut h) = job(rotated);
        let warns = warnings_of(&world, &mut h, SrcRef::Entity("part".into()), through_wall());
        assert!(gouges(&warns) > 0, "the wall is convicted on the op (rotated {rotated}): {warns:?}");
        assert_post_refused(&world, &format!("wall-{rotated}"), rotated);
    }
}

#[test]
fn a_whole_model_trochoid_under_the_floor_gouges_and_never_posts_where_the_sim_cannot_judge() {
    // Turned stock: the program sim refuses to model it, so the op's own
    // verdict is the only judge - before #1593 it was empty and posted.
    for rotated in [true, false] {
        let (world, mut h) = job(rotated);
        let warns = warnings_of(&world, &mut h, SrcRef::Entity("part".into()), under_floor());
        assert!(gouges(&warns) > 0, "the floor is convicted on the op (rotated {rotated}): {warns:?}");
        // Aligned, the program sim also sees the 3 mm and its clause
        // leads the refusal; turned, only the op's gouge holds it.
        assert_post_refused(&world, &format!("floor-{rotated}"), rotated);
    }
}

#[test]
fn a_picked_trochoid_that_fits_its_floor_but_sinks_under_it_never_posts() {
    // The floor pick confines the channel in XY (slot_fit, #1503/#1513)
    // and it fits; its depth is still typed - 3 mm under the floor.
    let (world, mut h) = job(true);
    let src = floor_pick_on(&world, "part");
    let warns = warnings_of(&world, &mut h, src, under_floor());
    assert!(gouges(&warns) > 0, "the picked floor is convicted on the op: {warns:?}");
    assert_post_refused(&world, "picked-floor", true);
}

#[test]
fn a_trochoid_inside_the_pocket_at_its_floor_stays_clean_and_posts() {
    for rotated in [false, true] {
        let (world, mut h) = job(rotated);
        let warns = warnings_of(&world, &mut h, SrcRef::Entity("part".into()), legal());
        assert_eq!(gouges(&warns), 0, "a legal slot is not convicted (rotated {rotated}): {warns:?}");
        let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
        assert!(blocking.is_empty(), "rotated {rotated}: {blocking:?}");
        if !rotated {
            let out = nc("legal");
            posting::post_setup(&world, &out).expect("a legal slot posts");
            let _ = std::fs::remove_file(&out);
        }
    }
}

#[test]
fn a_trochoid_on_the_stock_answers_for_the_part() {
    // The stock made from the part carries the same pocket; the Stock
    // source, or its floor picked, is the material to remove
    // (`part_solid` false) - but the part standing in it is kept (#1607,
    // replacing #1593's "never judged"): the channel 3 mm under the
    // floor enters the part, judged against the setup's parts, never
    // against the stock's own solid.
    for whole in [true, false] {
        let (world, mut h) = job(false);
        h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "part".into() }, Location::default()).expect("stock"));
        let src = if whole { SrcRef::Stock } else { floor_pick_on(&world, STOCK_NAME) };
        let warns = warnings_of(&world, &mut h, src, under_floor());
        assert!(gouges(&warns) > 0, "the part in the stock is kept (whole {whole}): {warns:?}");
    }
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

/// The pocket job over MCP: a whole-model Trochoidal slot from `a` to `b`
/// cut to `bottom`; verify's reply.
fn mcp_verify(tag: &str, a: f64, b: f64, bottom: f64) -> Value {
    let dir = std::env::temp_dir().join(format!("camcraft-trochoid-law-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("part.stl");
    pocket_support::write_stl(&pocket_block(), &stl);
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": stl.display().to_string() }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": 8.0 } }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": TOOL }));
    let params = json!({ "a": [a, 0.0], "b": [b, 0.0], "width": 10.0, "stock_to_leave": 0.0,
        "top_z": 8.0, "bottom_z": bottom, "stepdown": 4.0 });
    call(&mut s, &mut id, "add_operation", json!({ "kind": "Trochoidal slot", "tool": TOOL, "source": "part", "name": "T1", "params": params }));
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
        .map_or(0, |f| f.iter().filter(|x| x["kind"] == json!("gouge") && x["op"] == json!("T1") && x["blocks_post"] == json!(true)).count())
}

#[test]
fn over_mcp_the_whole_model_trochoid_is_judged_the_same() {
    let wall = mcp_verify("wall", 4.0, 20.0, FLOOR);
    assert!(op_gouges(&wall) > 0, "the operation's own gouge blocks the post: {wall}");
    assert_eq!(wall["ok"], json!(false), "{wall}");
    let good = mcp_verify("legal", -6.0, 6.0, FLOOR);
    assert_eq!(op_gouges(&good), 0, "{good}");
    assert_eq!(good["ok"], json!(true), "a legal slot verifies ok: {good}");
}

#[test]
fn fixture_stock_covers_the_part_in_both_cases() {
    for rotated in [false, true] {
        let (world, _) = job(rotated);
        let b = model::stock_world_aabb_for(&world, None).expect("stock box");
        let on = |v: f64, want: f64| (v - want).abs() < 1e-9;
        assert!(
            on(b.min.x, -20.0)
                && on(b.min.y, -20.0)
                && on(b.min.z, 0.0)
                && on(b.max.x, 20.0)
                && on(b.max.y, 20.0)
                && on(b.max.z, 8.0),
            "rotated {rotated}: stock at {:?}..{:?}",
            b.min,
            b.max
        );
    }
}
