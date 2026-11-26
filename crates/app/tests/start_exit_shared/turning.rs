//! Start and exit points of the turning kinds (#3331, slice 1: Facing,
//! Profile Rough, Profile Finish). A turning pick is a point in the lathe
//! (Z, X) plane: the first pass starts at the start pick, the last pass
//! leaves at the exit pick, and a pick that the laws cannot hold is refused
//! by name - it is never moved. Threading keeps its own start law and says
//! so by name. A program without a pick is not changed.
//!
//! Gate map (#3331): covers camcraft_app::operation (the Facing,
//! ProfileRough and ProfileFinish arms of generate, pick_kinds),
//! camcraft_app::commands (EditOperationParams undo / redo), app::mcp
//! (edit_operation picks on a turning kind).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, KindInfo, OpParams, OpStatus, OpType, PickCapability, SrcRef};
use camcraft_app::path_picks::{PathPick, PickRole};
use camcraft_app::schema::Location;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const INSERT: &str = "TN-1600-0R8";

/// Revolve a meridian polyline about Z (the turning suites' fixture law).
fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// The stage-A bar: wall r6 to z 2, neck r5 to the top, z -16..16.
fn bar_mesh() -> Mesh {
    revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)])
}

/// The bar in bar-sized stock (r 6, z -16..16), the insert, a lathe.
fn bar_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(
        &world,
        camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 32.0 },
        Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() },
    );
    history.execute(&world, stock.expect("bar stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == INSERT).map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, INSERT, tool).expect("catalog tool"));
    history.execute(&world, ImportModel::from_mesh("bar", bar_mesh()));
    let machine = Machine { model: "TestBench lathe".into(), kind: Some("lathe".into()), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine)).expect("a lathe"));
    (world, history)
}

fn finish() -> OpParams {
    OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(2.0),
        feed: 120.0,
        plunge_feed: 60.0,
        picks: Default::default(),
    }
}

fn rough() -> OpParams {
    OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(2.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
        picks: Default::default(),
    }
}

fn facing() -> OpParams {
    OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(15.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
        picks: Default::default(),
    }
}

/// A pick at lathe (Z, X): model X is the radius, model Z the station.
fn at(z: f64, x: f64) -> PathPick {
    PathPick { xy: Point2::new(x, 0.0), z: Some(z) }
}

fn picked(mut p: OpParams, start: Option<PathPick>, exit: Option<PathPick>) -> OpParams {
    let picks = p.path_picks_mut().expect("a path kind");
    picks.start = start.into_iter().collect();
    picks.exit = exit.into_iter().collect();
    p
}

/// Create and generate `params` as "Op" in a new bar world: the moves, or
/// the refusal text of the create door or of the generator.
fn run(params: OpParams) -> Result<Vec<Move>, String> {
    let (world, mut history) = bar_world();
    let create = CreateOperation::new(&world, "Op", INSERT, SrcRef::Entity("bar".into()), params).map_err(|e| e.to_string())?;
    history.execute(&world, create);
    moves(&world, &mut history)
}

fn moves(world: &World, history: &mut History) -> Result<Vec<Move>, String> {
    let generate = GenerateOperation::new(world, "Op").map_err(|e| e.to_string())?;
    history.execute(world, generate);
    let e = model::find_by_name(world, "Op").expect("the op");
    let status = camcraft_app::operation::status_of(world, &e);
    assert_eq!(status, OpStatus::Ok, "the picked path stands before the same verdict");
    Ok(model::toolpath_of(world, &e).expect("cached").path.moves)
}

/// A world point read as lathe (Z, X).
fn zx(q: Point3) -> (f64, f64) {
    (q.z, (q.x * q.x + q.y * q.y).sqrt())
}

fn near(q: Point3, (z, x): (f64, f64)) -> bool {
    let (qz, qx) = zx(q);
    (qz - z).abs() < 1e-6 && (qx - x).abs() < 1e-6
}

fn first_feed(m: &[Move]) -> usize {
    m.iter().position(|m| m.kind == MoveKind::Feed).expect("a feed")
}

fn last_feed(m: &[Move]) -> usize {
    m.iter().rposition(|m| m.kind == MoveKind::Feed).expect("a feed")
}

/// A kind's label, its parameters, a start and an exit, each as lathe (Z, X).
type Case = (&'static str, OpParams, (f64, f64), (f64, f64));

/// The three kinds of this slice, with a start and an exit that each holds:
/// in air, outside the bar (r 6) by more than the nose (0.8).
fn cases() -> [Case; 3] {
    [
        ("Profile Finish", finish(), (10.0, 9.0), (18.0, 10.0)),
        ("Profile Rough", rough(), (10.0, 9.5), (19.0, 8.0)),
        ("Facing", facing(), (20.0, 9.0), (22.0, 4.0)),
    ]
}

#[test]
fn the_first_pass_starts_at_the_start_pick() {
    for (label, params, start, _) in cases() {
        let plain = run(params.clone()).unwrap_or_else(|e| panic!("{label}: {e}"));
        let path = run(picked(params, Some(at(start.0, start.1)), None)).unwrap_or_else(|e| panic!("{label}: {e}"));
        let (i, j) = (first_feed(&path), first_feed(&plain));
        assert!(near(path[i].from, start), "{label}: the first feed leaves the pick: {:?}", zx(path[i].from));
        assert!(i > 0 && path[i - 1].kind == MoveKind::Rapid && near(path[i - 1].to, start), "{label}: a rapid brings the tool to the pick");
        assert!(near(path[i].to, zx(plain[j].to)), "{label}: the first feed ends where the automatic first feed ends");
        assert_eq!(path[i].feed, plain[j].feed, "{label}: at the same feed");
        // After the first feed, the picked path is the automatic path.
        assert_eq!(&path[i + 1..], &plain[j + 1..], "{label}: only the entry changed");
    }
}

#[test]
fn the_last_pass_leaves_at_the_exit_pick() {
    for (label, params, _, exit) in cases() {
        let plain = run(params.clone()).unwrap_or_else(|e| panic!("{label}: {e}"));
        let path = run(picked(params, None, Some(at(exit.0, exit.1)))).unwrap_or_else(|e| panic!("{label}: {e}"));
        let (i, j) = (last_feed(&path), last_feed(&plain));
        assert!(near(path[i].to, exit), "{label}: the last feed ends at the pick: {:?}", zx(path[i].to));
        assert!(near(path[i].from, zx(plain[j].to)), "{label}: it leaves from the end of the last pass");
        // Before it, the picked path is the automatic path; after it, one rapid home.
        assert_eq!(&path[..i], &plain[..=j], "{label}: only the exit changed");
        assert_eq!(path.len(), i + 2, "{label}: one move home after the pick");
        let (home, was) = (path[i + 1], plain[plain.len() - 1]);
        assert!(home.kind == MoveKind::Rapid && near(home.to, zx(was.to)), "{label}: the program ends home as before");
    }
}

#[test]
fn a_start_and_an_exit_pick_hold_together() {
    for (label, params, start, exit) in cases() {
        let path = run(picked(params, Some(at(start.0, start.1)), Some(at(exit.0, exit.1)))).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert!(near(path[first_feed(&path)].from, start), "{label}: the start");
        assert!(near(path[last_feed(&path)].to, exit), "{label}: the exit");
    }
}

/// The refusal of `params`, which must name the kind and each of `words`.
fn refused(label: &str, params: OpParams, words: &[&str]) {
    let Err(why) = run(params) else { panic!("{label}: the pick must be refused ({words:?})") };
    for w in std::iter::once(&label).chain(words) {
        assert!(why.contains(w), "{label}: the refusal says \"{w}\": {why}");
    }
}

#[test]
fn an_impossible_pick_is_refused_by_name() {
    for (label, params, start, exit) in cases() {
        let (start, exit) = (at(start.0, start.1), at(exit.0, exit.1));
        // In the bar: not in air.
        refused(label, picked(params.clone(), Some(at(5.0, 3.0)), None), &["start point", "inside the stock"]);
        refused(label, picked(params.clone(), Some(start), Some(at(5.0, 3.0))), &["exit point", "inside the stock"]);
        // Within the nose radius of the bar's skin: the nose would touch the stock.
        refused(label, picked(params.clone(), Some(at(5.0, 6.5)), None), &["start point", "inside the stock"]);
        // Behind the bar, inside its radius: the chuck side.
        refused(label, picked(params.clone(), Some(at(-20.0, 3.0)), None), &["start point", "chuck side"]);
        refused(label, picked(params.clone(), None, Some(at(-20.0, 3.0))), &["exit point", "chuck side"]);
        // No Z: not a point of the (Z, X) plane. The create door refuses it.
        let flat = PathPick::xy(Point2::new(9.0, 0.0));
        refused(label, picked(params.clone(), Some(flat), None), &["start point", "needs a Z"]);
        refused(label, picked(params.clone(), Some(start), Some(flat)), &["exit point", "needs a Z"]);
        let _ = exit;
    }
}

#[test]
fn a_pick_whose_move_crosses_the_part_or_the_stock_is_refused_by_name() {
    // Profile kinds: the first pass begins at the window's foot (z 2). A
    // start in front of the face near the axis feeds through the neck.
    for (label, params) in [("Profile Finish", finish()), ("Profile Rough", rough())] {
        refused(label, picked(params.clone(), Some(at(20.0, 1.0)), None), &["start point", "to the first pass", "crosses the part"]);
        // The last pass ends at the window's top (z 16) on the neck. An
        // exit far below, just outside the wall (r 6), grazes its corner.
        refused(label, picked(params, None, Some(at(-10.0, 6.9))), &["exit point", "from the last pass", "crosses the part"]);
    }
    // Facing opens on the axis at the clearance plane. The rapid from
    // there to a start beside the bar cuts the bar's corner.
    refused("Facing", picked(facing(), Some(at(10.0, 9.0)), None), &["start point", "rapid", "crosses the stock"]);
    // Facing ends its last pass on the axis, on the new face. An exit
    // along that face drags the nose across it.
    refused("Facing", picked(facing(), None, Some(at(15.8, 9.0))), &["exit point", "from the last pass", "the surface"]);
}

#[test]
fn threading_keeps_its_start_law_and_says_so_by_name() {
    let threading = KindInfo::ALL.iter().find(|k| k.ty == OpType::Threading).expect("the kind");
    let PickCapability::NotTaken(why) = threading.pick() else { panic!("Threading takes no pick") };
    assert!(why.contains("thread start law"), "the row names the law: {why}");
    for ty in [OpType::Facing, OpType::ProfileRough, OpType::ProfileFinish] {
        let cap = KindInfo::ALL.iter().find(|k| k.ty == ty).expect("the kind").pick();
        assert_eq!(format!("{cap:?}"), "Meridian", "{ty:?} takes a pick in the (Z, X) plane");
        assert!(cap.takes(PickRole::Start) && cap.takes(PickRole::Exit));
    }
}

#[test]
fn undo_and_redo_restore_the_path() {
    let (world, mut history) = bar_world();
    let create = CreateOperation::new(&world, "Op", INSERT, SrcRef::Entity("bar".into()), finish()).expect("create");
    history.execute(&world, create);
    let plain = moves(&world, &mut history).expect("the automatic path");
    let (start, exit) = ((10.0, 9.0), (18.0, 10.0));
    let edit = EditOperationParams::new(&world, "Op", picked(finish(), Some(at(start.0, start.1)), Some(at(exit.0, exit.1))));
    history.execute(&world, edit.expect("the picks are taken"));
    let live = |world: &World| {
        let e = model::find_by_name(world, "Op").expect("the op");
        model::operation_of(world, &e).expect("an operation").params.path_picks()
    };
    assert_eq!(live(&world).first(PickRole::Start), Some(at(start.0, start.1)));
    assert!(history.undo(&world), "undo the edit");
    assert!(live(&world).is_empty(), "undo clears the picks");
    assert!(history.redo(&world), "redo the edit");
    assert_eq!(live(&world).first(PickRole::Exit), Some(at(exit.0, exit.1)));
    let with = moves(&world, &mut history).expect("the picked path");
    assert!(near(with[first_feed(&with)].from, start) && near(with[last_feed(&with)].to, exit));
    assert_ne!(with, plain);
    // Undo the generate, then the edit: the previous parameters, and their path.
    assert!(history.undo(&world) && history.undo(&world));
    assert_eq!(moves(&world, &mut history).expect("after undo"), plain, "undo restores the automatic path");
}

/// The bar as an ASCII STL in a temp folder, for the MCP importer.
fn bar_stl(tag: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-turning-picks-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut s = String::from("solid bar\n");
    for t in bar_mesh().facets.iter() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid bar\n");
    let path = dir.join("bar.stl");
    std::fs::write(&path, s).expect("write stl");
    path
}

fn call(s: &mut camcraft_app::mcp::Server, name: &str, args: Value) -> (bool, String) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("an answer");
    (reply["result"]["isError"] == json!(true), reply["result"].to_string())
}

#[test]
fn mcp_edit_operation_sets_the_turning_picks_and_undo_clears_them() {
    let mut s = camcraft_app::mcp::Server::new();
    let lathe = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/lathe.mch");
    let stl = bar_stl("mcp");
    for (name, args) in [
        ("load_machine", json!({ "path": lathe.display().to_string() })),
        ("import_model", json!({ "path": stl.display().to_string() })),
        ("set_stock", json!({ "cylinder": { "diameter": 12.0, "height": 32.0 } })),
        ("add_tool", json!({ "catalog": INSERT })),
        ("add_operation", json!({ "kind": "Profile Finish", "tool": INSERT, "name": "Op" })),
    ] {
        let (err, r) = call(&mut s, name, args);
        assert!(!err, "{name}: {r}");
    }
    let (err, r) = call(&mut s, "edit_operation", json!({ "name": "Op", "params": { "picks": { "start": [[9.0, 0.0]] } } }));
    assert!(err && r.contains("Profile Finish") && r.contains("needs a Z"), "a pick with no Z is refused by name: {r}");
    let picks = json!({ "start": [[9.0, 0.0, 10.0]], "exit": [[10.0, 0.0, 18.0]] });
    let (err, r) = call(&mut s, "edit_operation", json!({ "name": "Op", "params": { "picks": picks } }));
    assert!(!err, "the picks are taken: {r}");
    let (_, job) = call(&mut s, "get_job", json!({}));
    assert!(job.contains("picks") && job.contains("[9.0,0.0,10.0]") && job.contains("[10.0,0.0,18.0]"), "{job}");
    assert!(!call(&mut s, "undo", json!({})).0);
    let (_, job) = call(&mut s, "get_job", json!({}));
    assert!(!job.contains("[9.0,0.0,10.0]"), "undo clears the picks: {job}");
    // Threading: the door names the thread start law.
    let (err, r) = call(&mut s, "list_operation_types", json!({ "kind": "Threading" }));
    assert!(!err && r.contains("thread start law"), "{r}");
}
