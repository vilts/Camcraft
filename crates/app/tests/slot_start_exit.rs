//! The Slot's start and exit points through the app (#3325, design
//! #1448): a start pick chooses the slot end where the pass starts, an
//! exit pick the end where it stops; a pick that the pass cannot hold is
//! refused by name and nothing is cut; the picks ride the undoable
//! parameter command, the project file, the stamp and the MCP
//! `edit_operation` patch; a slot without a pick is the earlier slot.
//!
//! Provenance: our own fixtures (the stock box of `slot_extension`),
//! specced from #3325's table row for the Slot; no vendor material.
//!
//! Gate map (#3325): covers app::operation (generate Slot arm,
//! pick_kinds, sane, params digest), app::project (path picks DTO),
//! app::commands (EditOperationParams), app::mcp (edit, job kind list),
//! motion::machining2d (slot_picks).

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SetStock, SetWcsOrigin,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Point3, quat_from_euler};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{self, Height, KindInfo, OpParams, OpType, PickCapability, SrcRef, Tilt};
use camcraft_app::path_picks::{PathPick, PathPicks, PickRole};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, project};
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

const TOOL: &str = "EM-0600-4FL";
const A: Point2 = Point2 { x: -15.0, y: 0.0 };
const B: Point2 = Point2 { x: 15.0, y: 0.0 };

fn picks(start: Option<Point2>, exit: Option<Point2>) -> PathPicks {
    PathPicks {
        start: start.map(PathPick::xy).into_iter().collect(),
        exit: exit.map(PathPick::xy).into_iter().collect(),
    }
}

/// A full-width (6 mm) slot from A to B, one row on the centerline,
/// z 20 down to 10 in two levels, climb: the earlier pass runs A -> B.
fn slot(picks: PathPicks) -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: A,
        b: B,
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
        blend_turns: false,
        blend_radius: None,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
        picks,
    }
}

/// An 80 x 60 x 20 stock box (top at z 20), the 6 mm end mill and the
/// slot `Slot1` over the stock, not generated yet.
fn job(params: OpParams) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 60.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Slot1", TOOL, SrcRef::Stock, params).expect("create"));
    (world, h)
}

fn generate(world: &World, h: &mut History) -> Result<Toolpath, String> {
    let g = GenerateOperation::new(world, "Slot1").map_err(|e| e.to_string())?;
    h.execute(world, g);
    let e = model::find_by_name(world, "Slot1").ok_or("no op")?;
    model::toolpath_of(world, &e).map(|c| c.path).ok_or_else(|| "no cached path".to_owned())
}

fn params_of(world: &World) -> OpParams {
    let e = model::find_by_name(world, "Slot1").expect("op");
    model::operation_of(world, &e).expect("op").params
}

/// Where the cutter first goes down and where its last feed stops (XY).
fn entry_and_exit(path: &Toolpath) -> ((f64, f64), (f64, f64)) {
    let first = path.moves.iter().find(|m| m.kind != MoveKind::Rapid).expect("a first descent");
    let last = path.moves.iter().rev().find(|m| m.kind == MoveKind::Feed).expect("a last feed");
    ((first.to.x, first.to.y), (last.to.x, last.to.y))
}

fn at(p: (f64, f64), q: Point2) -> bool {
    (p.0 - q.x).abs() < 1e-9 && (p.1 - q.y).abs() < 1e-9
}

#[test]
fn the_slot_takes_a_start_and_an_exit_pick() {
    let cap = KindInfo::ALL.iter().find(|k| k.ty == OpType::Slot).map(KindInfo::pick).expect("the Slot");
    assert_eq!(cap, PickCapability::Ends, "a slot pick is one of its two ends");
    assert!(cap.takes(PickRole::Start) && cap.takes(PickRole::Exit));
}

#[test]
fn the_entry_is_at_the_end_nearest_the_start_pick() {
    // No pick: the earlier slot, A -> B.
    let (world, mut h) = job(slot(PathPicks::default()));
    let (entry, exit) = entry_and_exit(&generate(&world, &mut h).expect("the earlier slot"));
    assert!(at(entry, A) && at(exit, B), "no pick runs A -> B: {entry:?} .. {exit:?}");

    // A start pick beside B: the cutter goes down at B and runs to A.
    let (world, mut h) = job(slot(picks(Some(Point2::new(14.0, 1.0)), None)));
    let path = generate(&world, &mut h).expect("a start pick at B is taken");
    let (entry, exit) = entry_and_exit(&path);
    assert!(at(entry, B), "the entry is at B: {entry:?}");
    assert!(at(exit, A), "the pass stops at the other end: {exit:?}");
    // The pass stays on the slot's line and between its ends.
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        assert!(m.to.y.abs() < 1e-9 && m.to.x >= -15.0 - 1e-9 && m.to.x <= 15.0 + 1e-9, "{m:?}");
    }

    // A start pick beside A is the earlier slot, to the move.
    let (plain, mut hp) = job(slot(PathPicks::default()));
    let (picked, mut hk) = job(slot(picks(Some(Point2::new(-16.0, 0.5)), None)));
    let (p0, p1) = (generate(&plain, &mut hp).expect("plain"), generate(&picked, &mut hk).expect("picked"));
    assert_eq!(p0.moves, p1.moves, "a pick at the end the pass already starts on changes no move");
}

#[test]
fn the_exit_is_at_the_end_nearest_the_exit_pick() {
    // An exit pick beside A alone: the pass must stop at A, so it starts at B.
    let (world, mut h) = job(slot(picks(None, Some(Point2::new(-14.0, -1.0)))));
    let (entry, exit) = entry_and_exit(&generate(&world, &mut h).expect("an exit pick at A is taken"));
    assert!(at(exit, A), "the exit is at A: {exit:?}");
    assert!(at(entry, B), "so the entry is at B: {entry:?}");

    // A start at B with an exit at A: both hold.
    let both = picks(Some(Point2::new(15.0, 0.0)), Some(Point2::new(-15.0, 0.0)));
    let (world, mut h) = job(slot(both));
    let (entry, exit) = entry_and_exit(&generate(&world, &mut h).expect("both picks hold"));
    assert!(at(entry, B) && at(exit, A), "{entry:?} .. {exit:?}");
}

fn picked_slot_with_wcs_yaw(yaw_deg: f64, start: bool) {
    let model_picks = if start {
        picks(Some(B), None)
    } else {
        picks(None, Some(A))
    };
    // Exercise rotation about the origin and about a translated work offset.
    for origin in [Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, -3.0, 0.0)] {
        let (world, mut h) = job(slot(model_picks.clone()));
        let rot = quat_from_euler(0.0, 0.0, yaw_deg.to_radians());
        h.execute(
            &world,
            SetWcsOrigin::new(&world, Location { pos: origin, rot }).expect("place G54"),
        );
        let path =
            generate(&world, &mut h).expect("the model-frame pick remains in reach under WCS yaw");
        let (entry, exit) = entry_and_exit(&path);
        assert!(
            at(entry, B) && at(exit, A),
            "yaw {yaw_deg}, origin {origin:?}: {entry:?} .. {exit:?}"
        );
        let OpParams::Slot { a, b, picks, .. } = params_of(&world) else {
            panic!("the operation stays a Slot");
        };
        assert_eq!(
            (a, b),
            (A, B),
            "generation keeps the stored model-frame endpoints"
        );
        assert_eq!(
            picks, model_picks,
            "generation keeps the stored model-frame picks"
        );
    }
}

#[test]
fn the_start_pick_holds_under_90_degree_wcs_yaw() {
    picked_slot_with_wcs_yaw(90.0, true);
}

#[test]
fn the_start_pick_holds_under_180_degree_wcs_yaw() {
    picked_slot_with_wcs_yaw(180.0, true);
}

#[test]
fn the_exit_pick_holds_under_90_degree_wcs_yaw() {
    picked_slot_with_wcs_yaw(90.0, false);
}

#[test]
fn the_exit_pick_holds_under_180_degree_wcs_yaw() {
    picked_slot_with_wcs_yaw(180.0, false);
}

#[test]
fn a_pick_the_pass_cannot_hold_is_refused_by_name() {
    // One row: the pass that starts at B stops at A. An exit at B is impossible.
    let both = picks(Some(Point2::new(15.0, 0.0)), Some(Point2::new(14.0, 0.0)));
    let (world, mut h) = job(slot(both));
    let why = generate(&world, &mut h).expect_err("a start and an exit at the same end of a one-row slot");
    assert!(why.contains("exit point") && why.contains("stops"), "{why}");

    // A pick far from the slot is not moved to it.
    let (world, mut h) = job(slot(picks(Some(Point2::new(15.0, 25.0)), None)));
    let why = generate(&world, &mut h).expect_err("a start pick 25 mm off the slot");
    assert!(why.contains("start point") && why.contains("farther"), "{why}");

    // A pick at the middle names no end.
    let (world, mut h) = job(slot(picks(None, Some(Point2::new(0.0, 0.0)))));
    let why = generate(&world, &mut h).expect_err("an exit pick at the middle");
    assert!(why.contains("exit point") && why.contains("same distance"), "{why}");

    // A tilted (3+2) slot does not take a pick yet: refused at the door.
    let mut tilted = slot(picks(Some(Point2::new(15.0, 0.0)), None));
    if let OpParams::Slot { tilt, .. } = &mut tilted {
        *tilt = Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg: 90.0, origin: [0.0, 0.0, 0.0] });
    }
    match EditOperationParams::new(&world, "Slot1", tilted) {
        Ok(_) => panic!("a tilted slot with a pick is refused"),
        Err(e) => assert!(e.to_string().contains("start point") && e.to_string().contains("tilted"), "{e}"),
    }
}

#[test]
fn the_picks_ride_the_command_undo_the_file_and_the_stamp() {
    let (world, mut h) = job(slot(PathPicks::default()));
    let before = generate(&world, &mut h).expect("plain");
    let picked = picks(Some(Point2::new(15.0, 0.0)), Some(Point2::new(-15.0, 0.0)));
    let stamp = |p: OpParams| operation::current_stamp(&world, TOOL, &SrcRef::Stock, &p, "X", None);
    assert_ne!(stamp(slot(picked.clone())), stamp(slot(PathPicks::default())), "the stamp tells a pick apart");
    // A pick that no planner can read is refused at the door.
    assert!(EditOperationParams::new(&world, "Slot1", slot(picks(Some(Point2::new(f64::NAN, 0.0)), None))).is_err());

    h.execute(&world, EditOperationParams::new(&world, "Slot1", slot(picked.clone())).expect("the slot takes the picks"));
    assert_eq!(params_of(&world).path_picks(), picked);
    let after = generate(&world, &mut h).expect("picked");
    assert!(at(entry_and_exit(&after).0, B), "the picked path enters at B");

    let file = std::env::temp_dir().join(format!("camcraft-slot-picks-{}.camproj", std::process::id()));
    project::save(&world, &file).expect("save");
    let back = new_world();
    project::load(&back, &file).expect("load");
    let _ = std::fs::remove_file(&file);
    assert_eq!(params_of(&back).path_picks(), picked, "the file keeps the picks");

    // Undo the generation and the edit: the earlier params and the earlier path.
    h.undo(&world);
    h.undo(&world);
    assert!(params_of(&world).path_picks().is_empty(), "undo clears the picks");
    let e = model::find_by_name(&world, "Slot1").expect("op");
    let restored = model::toolpath_of(&world, &e).expect("the earlier path is back").path;
    assert_eq!(restored.moves, before.moves, "undo restores the previous path");
    h.redo(&world);
    assert_eq!(params_of(&world).path_picks(), picked, "redo sets the picks again");
}

fn mcp(s: &mut Server, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("an answer");
    let r = reply["result"].clone();
    (r["isError"] == json!(true), r)
}

#[test]
fn mcp_edit_operation_sets_the_slot_picks() {
    let mut s = Server::new();
    assert!(!mcp(&mut s, "set_stock", json!({ "box": { "dx": 80.0, "dy": 60.0, "dz": 20.0 } })).0);
    assert!(!mcp(&mut s, "add_tool", json!({ "catalog": TOOL })).0);
    let (err, r) = mcp(&mut s, "add_operation", json!({ "kind": "Slot", "tool": TOOL, "name": "S" }));
    assert!(!err, "{r}");
    let patch = json!({ "name": "S", "params": { "picks": { "start": [[15.0, 0.0]], "exit": [[-15.0, 0.0]] } } });
    let (err, r) = mcp(&mut s, "edit_operation", patch);
    assert!(!err, "the Slot takes a start and an exit pick: {r}");
    // `list_operation_types` says that the Slot takes both picks, in the `picks` field.
    let (err, kinds) = mcp(&mut s, "list_operation_types", json!({ "kind": "slot" }));
    assert!(!err, "{kinds}");
    let text = kinds.to_string().replace("\\\"", "\"").replace(' ', "");
    for want in ["\"start\":true", "\"exit\":true", "\"field\":\"picks\""] {
        assert!(text.contains(want), "the Slot's start_exit says {want}: {text:.600}");
    }
    let (err, r) = mcp(&mut s, "edit_operation", json!({ "name": "S", "params": { "picks": {} } }));
    assert!(!err, "an empty object clears the picks: {r}");
}
