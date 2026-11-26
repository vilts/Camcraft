//! The contour's user-set start point through the app (#1448): set at
//! command creation and by `EditOperationParams` (undo restores the
//! automatic seam), nonsense and the kinds that do not take it refused at
//! the door by name, the project file writing `start_point` only when set
//! and reading it back, the stamp telling a pick apart, the MCP
//! `edit_operation` patch reaching it, and the generated path over the
//! stock locked by a golden CL dump. #1473: the exit point through the
//! same doors (command, undo, project file, stamp, MCP, other kinds).
//!
//! Gate map (#1448): covers app::operation (sane, params digest),
//! app::project (approach DTO), app::commands (CreateOperation,
//! EditOperationParams), app::mcp (edit), motion::machining2d
//! (contour_start).

mod common;

use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::links::Links;
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef, Tilt};
use camcraft_app::path_picks::{PathPick, PathPicks, PickRole};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, project};
use flecs_ecs::prelude::World;
use serde_json::{json, Value};

fn contour_op(pick: Option<Point2>) -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 10.0),
        retract_z: Height::of(HeightReference::TopOfStock, 5.0),
        top_z: Height::of(HeightReference::TopOfStock, 0.0),
        bottom_z: Height::of(HeightReference::TopOfStock, -4.0),
        stepdown: 2.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::Perpendicular { length: 5.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        tilt: None,
        links: Links::PLANAR,
        chain_overrides: Vec::new(),
        picks: PathPicks { start: pick.map(PathPick::xy).into_iter().collect(), exit: Vec::new() },
    }
}

/// A 60 x 40 x 20 box stock (centred on the origin) and a 10 mm end mill.
fn job() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 40.0, dz: 20.0 }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "T", tool).expect("tool"));
    (world, h)
}

fn params_of(world: &World) -> OpParams {
    let e = model::find_by_name(world, "Op1").expect("op");
    model::operation_of(world, &e).expect("op").params
}

fn pick_of(p: &OpParams) -> Option<Point2> {
    p.path_picks().first(PickRole::Start).map(|k| k.xy)
}

#[test]
fn nonsense_and_other_kinds_are_refused_at_the_door() {
    let (world, _h) = job();
    for bad in [Point2::new(f64::NAN, 0.0), Point2::new(f64::INFINITY, 1.0), Point2::new(0.0, 2e6)] {
        let r = CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, contour_op(Some(bad)));
        assert!(r.is_err(), "{bad:?} refused at creation");
    }
    // A tilted contour does not take a pick yet.
    let mut tilted = contour_op(Some(Point2::new(35.0, 0.0)));
    if let OpParams::Contour { tilt, .. } = &mut tilted {
        *tilt = Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg: 90.0, origin: [0.0, 0.0, 0.0] });
    }
    match CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, tilted) {
        Ok(_) => panic!("a tilted contour with a pick is refused"),
        Err(e) => assert!(format!("{e}").contains("start point"), "{e}"),
    }
}

#[test]
fn edit_sets_the_pick_and_undo_restores_the_automatic_seam() {
    let (world, mut h) = job();
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, contour_op(None)).expect("create"));
    let q = Point2::new(35.0, 0.0);
    h.execute(&world, EditOperationParams::new(&world, "Op1", contour_op(Some(q))).expect("edit"));
    assert_eq!(pick_of(&params_of(&world)), Some(q));
    h.undo(&world);
    assert_eq!(pick_of(&params_of(&world)), None);
    h.redo(&world);
    assert_eq!(pick_of(&params_of(&world)), Some(q));
}

#[test]
fn the_project_file_writes_the_pick_only_when_set_and_reads_it_back() {
    for pick in [None, Some(Point2::new(35.0, 0.0))] {
        let (world, mut h) = job();
        h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, contour_op(pick)).expect("create"));
        let path = std::env::temp_dir().join(format!("camcraft-start-point-{}-{}.camproj", pick.is_some(), std::process::id()));
        project::save(&world, &path).expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(text.contains("\"picks\""), pick.is_some(), "the key only when set");
        assert!(!text.contains("\"start_point\""), "legacy key is never written");
        let back = new_world();
        project::load(&back, &path).expect("load");
        let _ = std::fs::remove_file(&path);
        assert_eq!(pick_of(&params_of(&back)), pick);
    }
}

#[test]
fn the_stamp_tells_a_pick_apart() {
    let (world, _h) = job();
    let stamp = |p: Option<Point2>| operation::current_stamp(&world, "T", &SrcRef::Stock, &contour_op(p), "X", None);
    let base = stamp(None);
    assert!(base.is_some());
    assert_ne!(stamp(Some(Point2::new(35.0, 0.0))), base);
    assert_ne!(stamp(Some(Point2::new(35.0, 0.0))), stamp(Some(Point2::new(35.0, 1.0))));
}

#[test]
fn the_picked_start_over_the_stock_matches_golden() {
    let (world, mut h) = job();
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, contour_op(Some(Point2::new(36.0, 0.0)))).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    // The cutter-centre ring sits 5 mm off the right wall (x = 35; the box is centred): the
    // lead-in ends at the pick's foot on it.
    let on = tp.path.moves.iter().any(|m| (m.to.x - 35.0).abs() < 1e-6 && m.to.y.abs() < 1e-6);
    assert!(on, "the ring starts at (35, 0): {:?}", tp.path.notes);
    common::write_golden("contour_start_point.cl", &common::gen_to_cltext(&tp.path));
}

/// #3326 review: a pick is stated in the setup's frame, as the geometry is.
/// Under a work offset turned 90 degrees the generator works in a turned
/// frame; the start and the exit pick enter it with the ring. Without that
/// the cut enters at (0, 25) and leaves at (-35, 0), on the wrong walls.
#[test]
fn a_rotated_work_offset_reads_the_contour_picks_in_the_setup_frame() {
    use camcraft_app::commands::SetWcsOrigin;
    use camcraft_app::geometry::{quat_from_euler, Point3};
    use camcraft_app::schema::Location;

    let (world, mut h) = job();
    let placement = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: quat_from_euler(0.0, 0.0, 90f64.to_radians()) };
    h.execute(&world, SetWcsOrigin::new(&world, placement).expect("yaw WCS"));
    let mut op = contour_op(Some(Point2::new(36.0, 0.0)));
    if let Some(picks) = op.path_picks_mut() {
        picks.exit = vec![PathPick::xy(Point2::new(0.0, 26.0))];
    }
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, op).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let near = |v: f64, want: f64| (v - want).abs() < 1e-6;
    // The cutter-centre ring is 5 mm off the walls: x = 35 on the right, y = 25 on the top.
    let on = tp.path.moves.iter().any(|m| near(m.to.x, 35.0) && near(m.to.y, 0.0) && m.from.x > 35.0);
    assert!(on, "the lead-in ends at the start pick's foot (35, 0): {:?}", tp.path.notes);
    let leaves = tp.path.moves.iter().any(|m| near(m.from.x, 0.0) && near(m.from.y, 25.0) && m.to.y > 25.0);
    assert!(leaves, "the lead-out leaves at the exit pick's foot (0, 25): {:?}", tp.path.notes);
}

fn mcp(s: &mut Server, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("an answer");
    let r = reply["result"].clone();
    (r["isError"] == json!(true), r)
}

#[test]
fn mcp_edit_operation_reaches_the_start_point() {
    let mut s = Server::new();
    assert!(!mcp(&mut s, "set_stock", json!({ "box": { "dx": 60.0, "dy": 40.0, "dz": 20.0 } })).0);
    assert!(!mcp(&mut s, "add_tool", json!({ "catalog": "EM-1000-4FL" })).0);
    let (err, r) = mcp(&mut s, "add_operation", json!({ "kind": "Contour", "tool": "EM-1000-4FL", "name": "C" }));
    assert!(!err, "{r}");
    let patch = json!({ "name": "C", "params": { "picks": { "start": [[35.0, 0.0]] } } });
    let (err, r) = mcp(&mut s, "edit_operation", patch);
    assert!(!err, "the patch is taken: {r}");
    assert!(r.to_string().contains("\"picks\""), "the edited params carry it: {r}");
    // Clearing via empty array round-trips cleanly.
    let (err, r) = mcp(&mut s, "edit_operation", json!({ "name": "C", "params": { "picks": { "start": [] } } }));
    assert!(!err, "clearing picks is accepted: {r}");
    // A Pocket does not take a pick: refused in the command's words.
    assert!(!mcp(&mut s, "add_operation", json!({ "kind": "Pocket", "tool": "EM-1000-4FL", "name": "F" })).0);
    let face = json!({ "name": "F", "params": { "picks": { "start": [[35.0, 0.0]] } } });
    let (err, r) = mcp(&mut s, "edit_operation", face);
    assert!(err && r.to_string().contains("start point"), "{r}");
}

fn exit_op(exit: Option<Point2>) -> OpParams {
    let mut op = contour_op(None);
    if let Some(picks) = op.path_picks_mut() {
        picks.exit = exit.map(PathPick::xy).into_iter().collect();
    }
    op
}

fn exit_of(p: &OpParams) -> Option<Point2> {
    p.path_picks().first(PickRole::Exit).map(|k| k.xy)
}

#[test]
fn the_exit_point_rides_the_command_undo_the_file_and_the_stamp() {
    let (world, mut h) = job();
    assert!(CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, exit_op(Some(Point2::new(f64::NAN, 0.0)))).is_err());
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, exit_op(None)).expect("create"));
    let q = Point2::new(0.0, 25.0);
    h.execute(&world, EditOperationParams::new(&world, "Op1", exit_op(Some(q))).expect("edit"));
    assert_eq!(exit_of(&params_of(&world)), Some(q));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    // The cutter-centre ring runs 5 mm off the top wall (y = 25): the cut leaves at (0, 25).
    let leaves = tp.path.moves.iter().any(|m| m.from.x.abs() < 1e-6 && (m.from.y - 25.0).abs() < 1e-6 && m.to.y > 25.0);
    assert!(leaves, "the lead-out leaves at the exit's foot: {:?}", tp.path.notes);
    let path = std::env::temp_dir().join(format!("camcraft-exit-point-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"picks\""), "the key when set");
    assert!(!text.contains("\"exit_point\""), "legacy key is never written");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(exit_of(&params_of(&back)), Some(q));
    let stamp = |p: OpParams| operation::current_stamp(&world, "T", &SrcRef::Stock, &p, "X", None);
    assert_ne!(stamp(exit_op(Some(q))), stamp(exit_op(None)));
    h.undo(&world);
    h.undo(&world);
    assert_eq!(exit_of(&params_of(&world)), None);
}

#[test]
fn mcp_edit_operation_reaches_the_exit_point() {
    let mut s = Server::new();
    assert!(!mcp(&mut s, "set_stock", json!({ "box": { "dx": 60.0, "dy": 40.0, "dz": 20.0 } })).0);
    assert!(!mcp(&mut s, "add_tool", json!({ "catalog": "EM-1000-4FL" })).0);
    assert!(!mcp(&mut s, "add_operation", json!({ "kind": "Contour", "tool": "EM-1000-4FL", "name": "C" })).0);
    let patch = json!({ "name": "C", "params": { "picks": { "exit": [[0.0, 25.0]] } } });
    let (err, r) = mcp(&mut s, "edit_operation", patch);
    assert!(!err && r.to_string().contains("\"picks\""), "the patch is taken: {r}");
    assert!(!mcp(&mut s, "add_operation", json!({ "kind": "Pocket", "tool": "EM-1000-4FL", "name": "F" })).0);
    let face = json!({ "name": "F", "params": { "picks": { "exit": [[0.0, 25.0]] } } });
    let (err, r) = mcp(&mut s, "edit_operation", face);
    assert!(err && r.to_string().contains("exit point"), "{r}");
}

#[test]
fn legacy_links_approach_start_and_exit_points_load_and_migrate_to_picks() {
    let (world, mut h) = job();
    let mut op = contour_op(Some(Point2::new(35.0, 0.0)));
    if let Some(picks) = op.path_picks_mut() {
        picks.exit = vec![PathPick::xy(Point2::new(0.0, 25.0))];
    }
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, op.clone()).expect("create"));
    let expected_stamp = operation::current_stamp(&world, "T", &SrcRef::Stock, &op, "Op1", None);
    let mut doc: Value = serde_json::to_value(project::collect(&world)).expect("collect");
    let contour = doc["entities"]
        .as_array_mut()
        .expect("entities")
        .iter_mut()
        .find_map(|row| row["operation"]["params"]["contour"].as_object_mut())
        .expect("contour op");
    contour.remove("picks");
    contour.insert(
        "links".into(),
        json!({ "approach": { "start_point": [35.0, 0.0], "exit_point": [0.0, 25.0] } }),
    );
    let path = std::env::temp_dir().join(format!("camcraft-legacy-approach-picks-{}.camproj", std::process::id()));
    std::fs::write(&path, serde_json::to_string(&doc).expect("json")).expect("write");
    let back = new_world();
    project::load(&back, &path).expect("load legacy file");
    let loaded = params_of(&back);
    assert_eq!(pick_of(&loaded), Some(Point2::new(35.0, 0.0)));
    assert_eq!(exit_of(&loaded), Some(Point2::new(0.0, 25.0)));
    assert_eq!(operation::current_stamp(&back, "T", &SrcRef::Stock, &loaded, "Op1", None), expected_stamp);
    project::save(&back, &path).expect("resave");
    let resaved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(resaved.contains("\"picks\""), "rewrites under picks: {resaved}");
    assert!(!resaved.contains("start_point") && !resaved.contains("exit_point"), "drops legacy keys");
}

#[test]
fn legacy_wire_start_key_loads_and_migrates_to_picks() {
    let world = new_world();
    let mut op = operation::seed_params(operation::OpType::WireContour.ordinal());
    if let Some(picks) = op.path_picks_mut() {
        picks.start = vec![PathPick::xy(Point2::new(2.5, -1.5))];
    }
    let expected_stamp = operation::current_stamp(&world, "EDM-W025", &SrcRef::Stock, &op, "Wire1", None);
    let mut doc: Value = serde_json::to_value(project::collect(&world)).expect("collect");
    doc["entities"] = json!([{
        "name": "Wire1",
        "kind": "operation",
        "translation": [0.0, 0.0, 0.0],
        "operation": {
            "tool": "EDM-W025",
            "src": "stock",
            "params": {
                "wire_contour": {
                    "top_z": { "reference": "top_of_stock", "offset": 0.0 },
                    "bottom_z": { "reference": "bottom_of_stock", "offset": 0.0 },
                    "side": "die",
                    "start": [2.5, -1.5]
                }
            }
        }
    }]);
    let path = std::env::temp_dir().join(format!("camcraft-legacy-wire-start-{}.camproj", std::process::id()));
    std::fs::write(&path, serde_json::to_string(&doc).expect("json")).expect("write");
    let back = new_world();
    project::load(&back, &path).expect("load legacy wire file");
    let e = model::find_by_name(&back, "Wire1").expect("wire op");
    let loaded = model::operation_of(&back, &e).expect("op").params;
    assert_eq!(pick_of(&loaded), Some(Point2::new(2.5, -1.5)));
    assert_eq!(operation::current_stamp(&back, "EDM-W025", &SrcRef::Stock, &loaded, "Wire1", None), expected_stamp);
    project::save(&back, &path).expect("resave");
    let resaved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    let resaved_val: Value = serde_json::from_str(&resaved).expect("json");
    let wire = &resaved_val["entities"][0]["operation"]["params"]["wire_contour"];
    assert!(wire.get("picks").is_some(), "rewrites under picks: {resaved}");
    assert!(wire.get("start").is_none(), "legacy wire start key is never written: {resaved}");
}
