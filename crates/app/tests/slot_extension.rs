//! The Slot's end extensions (#1731): the pass runs past each authored
//! end by its own distance along the slot, set through the undoable
//! parameter command and the MCP `edit_operation` patch; an extension
//! that drives the cutter into a part the job keeps is a blocking Gouge
//! and the program is refused; zero (the default) leaves the slot, its
//! stamp and its project file as they were.
//!
//! Provenance: our own fixtures (the channelled block of `slot_caps`),
//! specced from the maintainer's request on #1731 and the shop rule that
//! a slot may run out past the part edge but never into a kept wall; no
//! vendor material.
//!
//! Gate map (#1731): covers app::operation (generate Slot arm, material
//! stadium, sane, params digest), app::project (Slot DTO), app::commands
//! (EditOperationParams), app::mcp (edit), motion::machining2d
//! (slot_extended_ends), sim::verify, app::posting.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project};
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

const TOOL: &str = "EM-0600-4FL";

/// A full-width (6 mm) slot from (-15, 0) to (15, 0), one row on the
/// centerline, z 20 down to 10, with the given end extensions.
fn slot(start: f64, end: f64) -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: Point2::new(-15.0, 0.0),
        b: Point2::new(15.0, 0.0),
        width: 6.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 5.0,
        stepover: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
        extend_start: start,
        extend_end: end,
    }
}

/// An 80 x 60 x [0, 20] block with a straight 6 mm channel along X from
/// x -18 to 18 (the slot's own stadium squared off at the ends - the
/// cutter's caps stay inside the box ends), sunk from z 20 to 10.
/// Outward-wound.
fn channelled_block() -> Mesh {
    let (ex, ey, cx, cy) = (40.0, 30.0, 18.0, 3.0);
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let block = cube(Point3::new(-ex, -ey, 0.0), Point3::new(ex, ey, 20.0));
    let mut f: Vec<Triangle> = block.facets[0..2].to_vec(); // bottom
    f.extend(block.facets[4..12].iter().cloned()); // outer walls
    let quad = |f: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    // The top, around the channel opening, up.
    quad(&mut f, p(-ex, -ey, 20.0), p(ex, -ey, 20.0), p(ex, -cy, 20.0), p(-ex, -cy, 20.0));
    quad(&mut f, p(-ex, cy, 20.0), p(ex, cy, 20.0), p(ex, ey, 20.0), p(-ex, ey, 20.0));
    quad(&mut f, p(-ex, -cy, 20.0), p(-cx, -cy, 20.0), p(-cx, cy, 20.0), p(-ex, cy, 20.0));
    quad(&mut f, p(cx, -cy, 20.0), p(ex, -cy, 20.0), p(ex, cy, 20.0), p(cx, cy, 20.0));
    // The channel floor, up.
    quad(&mut f, p(-cx, -cy, 10.0), p(cx, -cy, 10.0), p(cx, cy, 10.0), p(-cx, cy, 10.0));
    // The channel walls, facing the void.
    quad(&mut f, p(-cx, -cy, 10.0), p(-cx, -cy, 20.0), p(cx, -cy, 20.0), p(cx, -cy, 10.0)); // +y
    quad(&mut f, p(cx, cy, 10.0), p(cx, cy, 20.0), p(-cx, cy, 20.0), p(-cx, cy, 10.0)); // -y
    quad(&mut f, p(cx, -cy, 10.0), p(cx, -cy, 20.0), p(cx, cy, 20.0), p(cx, cy, 10.0)); // -x
    quad(&mut f, p(-cx, cy, 10.0), p(-cx, cy, 20.0), p(-cx, -cy, 20.0), p(-cx, -cy, 10.0)); // +x
    Mesh::from_facets(f)
}

/// The job: an 80 x 60 x 20 stock box (top at z 20) and the 6 mm end
/// mill; with `part`, the channelled block is imported and is the op's
/// source, else the stock is.
fn job(part: bool, params: OpParams) -> (World, History) {
    job_on(part, part, params)
}

/// The job with the channelled block imported when `import`, and the op
/// cut off the part when `part_src`, else off the stock (#1607: the
/// setup's parts are kept from a stock source too).
fn job_on(import: bool, part_src: bool, params: OpParams) -> (World, History) {
    let (part, world) = (import, new_world());
    let mut h = History::default();
    if part {
        h.execute(&world, ImportModel::from_mesh("part", channelled_block()));
    }
    let stock = SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 60.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let src = if part_src { SrcRef::Entity("part".into()) } else { SrcRef::Stock };
    h.execute(&world, CreateOperation::new(&world, "Slot1", TOOL, src, params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Slot1").expect("generate"));
    (world, h)
}

fn cached(world: &World) -> (Toolpath, Vec<Warning>) {
    let e = model::find_by_name(world, "Slot1").expect("op");
    let c = model::toolpath_of(world, &e).expect("cached path");
    (c.path, c.warnings)
}

fn params_of(world: &World) -> OpParams {
    let e = model::find_by_name(world, "Slot1").expect("op");
    model::operation_of(world, &e).expect("op").params
}

/// The X span the feed moves cover.
fn feed_span(path: &Toolpath) -> (f64, f64) {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .flat_map(|m| [m.from.x, m.to.x])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(x), hi.max(x)))
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

fn nc_path(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-slot-ext-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn the_pass_extends_by_the_given_distance_at_each_end() {
    let (world, _h) = job(false, slot(0.0, 0.0));
    let (path, warns) = cached(&world);
    let (lo, hi) = feed_span(&path);
    assert!((lo + 15.0).abs() < 1e-9 && (hi - 15.0).abs() < 1e-9, "authored ends: {lo} .. {hi}");
    assert!(!warns.iter().any(Warning::blocks_post), "{warns:?}");

    // 5 mm past A and 8 mm past B, over the stock (no part to keep).
    let (world, _h) = job(false, slot(5.0, 8.0));
    let (path, warns) = cached(&world);
    let (lo, hi) = feed_span(&path);
    assert!((lo + 20.0).abs() < 1e-9, "5 mm past A: {lo}");
    assert!((hi - 23.0).abs() < 1e-9, "8 mm past B: {hi}");
    // The pass stays on the slot's line: every feed holds y = 0.
    assert!(path.moves.iter().filter(|m| m.kind == MoveKind::Feed).all(|m| m.to.y.abs() < 1e-9));
    assert_eq!(gouges(&warns), 0, "{warns:?}");
    assert!(!warns.iter().any(Warning::blocks_post), "{warns:?}");
    let nc = nc_path("air");
    posting::post_setup(&world, &nc).expect("an extension over the stock posts");
    let _ = std::fs::remove_file(&nc);
}

#[test]
fn an_extension_into_the_part_is_a_gouge_and_never_posts() {
    // The channel is cut to x +-18: the authored slot (caps to x +-18)
    // is clean on the part source.
    let (world, _h) = job(true, slot(0.0, 0.0));
    let (_, warns) = cached(&world);
    assert_eq!(gouges(&warns), 0, "the authored slot keeps the part: {warns:?}");
    assert!(!warns.iter().any(Warning::blocks_post), "{warns:?}");
    let nc = nc_path("clean");
    posting::post_setup(&world, &nc).expect("the authored slot posts");
    let _ = std::fs::remove_file(&nc);

    // 5 mm past B drives the cutter 5 mm into the channel's end wall.
    for (start, end) in [(0.0, 5.0), (5.0, 0.0)] {
        let (world, _h) = job(true, slot(start, end));
        let (_, warns) = cached(&world);
        assert!(gouges(&warns) > 0, "extension {start}/{end} into the wall convicts: {warns:?}");
        assert!(warns.iter().any(Warning::blocks_post), "{warns:?}");
        let nc = nc_path(&format!("wall-{start}-{end}"));
        assert!(posting::post_setup(&world, &nc).is_err(), "a gouging extension is refused at the post");
        assert!(!nc.exists(), "no program is written");
    }
}

#[test]
fn off_the_stock_an_extension_into_a_kept_part_is_a_gouge() {
    // The block stands in the stock and the slot is cut off the stock:
    // the setup's part is still kept (#1607), so the extended pass
    // answers to it the same way.
    let (world, _h) = job_on(true, false, slot(0.0, 0.0));
    let (_, warns) = cached(&world);
    assert_eq!(gouges(&warns), 0, "the authored slot keeps the part: {warns:?}");
    let (world, _h) = job_on(true, false, slot(0.0, 5.0));
    let (_, warns) = cached(&world);
    assert!(gouges(&warns) > 0, "the extension into the wall convicts: {warns:?}");
    let nc = nc_path("stock-wall");
    assert!(posting::post_setup(&world, &nc).is_err(), "a gouging extension is refused at the post");
    assert!(!nc.exists(), "no program is written");
}

#[test]
fn the_extensions_ride_the_command_undo_the_file_and_the_stamp() {
    // Refused at the door: a negative or a non-finite extension.
    let (world, mut h) = job(false, slot(0.0, 0.0));
    assert!(EditOperationParams::new(&world, "Slot1", slot(-1.0, 0.0)).is_err());
    assert!(EditOperationParams::new(&world, "Slot1", slot(0.0, f64::NAN)).is_err());
    // Zero is the earlier slot: the stamp does not move, and the file
    // carries no key for it.
    let stamp = |p: OpParams| operation::current_stamp(&world, TOOL, &SrcRef::Stock, &p, "X", None);
    assert_ne!(stamp(slot(0.0, 2.0)), stamp(slot(0.0, 0.0)));
    assert_ne!(stamp(slot(2.0, 0.0)), stamp(slot(0.0, 2.0)));
    let text = project::save_text(&world).expect("save");
    assert!(!text.contains("extend_start") && !text.contains("extend_end"), "no keys at zero");

    h.execute(&world, EditOperationParams::new(&world, "Slot1", slot(4.0, 6.0)).expect("edit"));
    let ends = |p: &OpParams| match p {
        OpParams::Slot { extend_start, extend_end, .. } => (*extend_start, *extend_end),
        _ => (f64::NAN, f64::NAN),
    };
    assert_eq!(ends(&params_of(&world)), (4.0, 6.0));
    let path = std::env::temp_dir().join(format!("camcraft-slot-ext-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(ends(&params_of(&back)), (4.0, 6.0));
    h.undo(&world);
    assert_eq!(ends(&params_of(&world)), (0.0, 0.0));
}

fn mcp(s: &mut Server, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("an answer");
    let r = reply["result"].clone();
    (r["isError"] == json!(true), r)
}

#[test]
fn mcp_edit_operation_reaches_the_slot_extensions() {
    let mut s = Server::new();
    assert!(!mcp(&mut s, "set_stock", json!({ "box": { "dx": 80.0, "dy": 60.0, "dz": 20.0 } })).0);
    assert!(!mcp(&mut s, "add_tool", json!({ "catalog": TOOL })).0);
    let (err, r) = mcp(&mut s, "add_operation", json!({ "kind": "Slot", "tool": TOOL, "name": "S" }));
    assert!(!err, "{r}");
    let patch = json!({ "name": "S", "params": { "extend_start": 3.0, "extend_end": 7.5 } });
    let (err, r) = mcp(&mut s, "edit_operation", patch);
    assert!(!err, "the patch is taken: {r}");
    let text = r.to_string();
    assert!(text.contains("extend_start") && text.contains("extend_end"), "the edited params carry them: {r}");
    let bad = json!({ "name": "S", "params": { "extend_end": -2.0 } });
    let (err, r) = mcp(&mut s, "edit_operation", bad);
    assert!(err, "a negative extension is refused: {r}");
}
