//! The Face's start and exit picks through the app (#3326, design #1448):
//! the registry says the Face takes them, `EditOperationParams` sets them
//! (undo and redo put the previous path back), the generated raster enters
//! at the picked corner and leaves on the picked side, an exit that the row
//! count cannot reach is refused by name, a tilted face refuses a pick at
//! the door, and the MCP `edit_operation` patch reaches the picks.
//!
//! Gate map (#3326): covers app::operation (pick_kinds, generate Face arm,
//! boundary_turn), app::commands (EditOperationParams, GenerateOperation),
//! app::mcp (edit).

mod common;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, KindInfo, OpParams, OpType, PickCapability, SrcRef, Tilt};
use camcraft_app::path_picks::{PathPick, PathPicks, PickRole};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{json, Value};

type Pt = Option<(f64, f64)>;

/// A Face over the stock: six rows at stepover 6, five at 7.5.
fn face_op(stepover: f64, start: Pt, exit: Pt) -> OpParams {
    let of = |q: Pt| q.map(|(x, y)| PathPick::xy(Point2::new(x, y))).into_iter().collect();
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        blend_radius: None,
        tilt: None,
        stepover,
        clearance_z: Height::of(HeightReference::TopOfStock, 10.0),
        cut_z: Height::of(HeightReference::TopOfStock, -1.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: PathPicks { start: of(start), exit: of(exit) },
    }
}

/// A 60 x 40 x 20 box stock (centred on the origin) and a 10 mm end mill:
/// the rows run Y -15 .. 15 between X -35 and 35.
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

fn path_of(world: &World) -> Toolpath {
    let e = model::find_by_name(world, "Op1").expect("op");
    model::toolpath_of(world, &e).expect("cached path").path
}

fn generate(world: &World, h: &mut History) -> Toolpath {
    h.execute(world, GenerateOperation::new(world, "Op1").expect("generate"));
    path_of(world)
}

fn entry(t: &Toolpath) -> (f64, f64) {
    let m = t.moves.iter().find(|m| m.kind == MoveKind::Plunge).expect("a plunge");
    (m.to.x, m.to.y)
}

fn leave(t: &Toolpath) -> (f64, f64) {
    let m = t.moves.iter().rev().find(|m| m.kind == MoveKind::Feed).expect("a feed");
    (m.to.x, m.to.y)
}

#[test]
fn the_registry_says_the_face_takes_a_start_and_an_exit() {
    let cap = KindInfo::ALL.iter().find(|k| k.ty == OpType::Face).map(KindInfo::pick);
    assert_eq!(cap, Some(PickCapability::Raster));
    assert!(PickCapability::Raster.takes(PickRole::Start) && PickCapability::Raster.takes(PickRole::Exit));
}

#[test]
fn the_raster_enters_at_each_picked_corner() {
    for (pick, at, out) in [
        ((-29.0, -19.0), (-35.0, -15.0), (-35.0, 15.0)),
        ((29.0, -19.0), (35.0, -15.0), (35.0, 15.0)),
        ((-29.0, 19.0), (-35.0, 15.0), (-35.0, -15.0)),
        ((29.0, 19.0), (35.0, 15.0), (35.0, -15.0)),
    ] {
        let (world, mut h) = job();
        h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, face_op(6.0, Some(pick), None)).expect("create"));
        let t = generate(&world, &mut h);
        assert_eq!(entry(&t), at, "the start pick {pick:?} takes its corner");
        assert_eq!(leave(&t), out, "six rows from {pick:?} end on its side, at the far edge");
    }
}

#[test]
fn the_picked_corner_over_the_stock_matches_golden() {
    let (world, mut h) = job();
    let op = face_op(6.0, Some((29.0, 19.0)), Some((29.0, -19.0)));
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, op).expect("create"));
    let t = generate(&world, &mut h);
    assert_eq!((entry(&t), leave(&t)), ((35.0, 15.0), (35.0, -15.0)));
    common::write_golden("face_start_exit.cl", &common::gen_to_cltext(&t));
}

#[test]
fn an_exit_the_rows_cannot_reach_is_refused_by_name_and_a_reachable_one_is_cut() {
    // Five rows from the low corner end across from it; six on its own side.
    for (stepover, reached, refused, rows) in [(7.5, (29.0, 19.0), (-29.0, 19.0), "5 rows"), (6.0, (-29.0, 19.0), (29.0, 19.0), "6 rows")] {
        let (world, mut h) = job();
        let start = Some((-29.0, -19.0));
        h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, face_op(stepover, start, Some(reached))).expect("create"));
        let t = generate(&world, &mut h);
        assert_eq!(leave(&t).0, if reached.0 > 0.0 { 35.0 } else { -35.0 }, "the last row ends on the exit's side");
        h.execute(&world, EditOperationParams::new(&world, "Op1", face_op(stepover, start, Some(refused))).expect("edit"));
        let why = GenerateOperation::new(&world, "Op1").map(|_| ()).expect_err("refused").to_string();
        assert!(why.contains("exit point") && why.contains(rows), "{why}");
    }
}

/// #3326 review: a pick is stated in the setup's frame. Under a work
/// offset turned 90 degrees about (5, 7) the rows run along the setup's Y:
/// ten rows from the picked corner, which end on the start corner's own
/// side. The entry is beside the picked corner, the exit pick is read in
/// the same frame, and the refusal names the exit.
#[test]
fn a_rotated_work_offset_reads_the_picks_in_the_setup_frame() {
    use camcraft_app::commands::SetWcsOrigin;
    use camcraft_app::geometry::{quat_from_euler, Point3};
    use camcraft_app::schema::Location;

    let near = |got: (f64, f64), want: (f64, f64)| (got.0 - want.0).abs() < 1e-6 && (got.1 - want.1).abs() < 1e-6;
    let (world, mut h) = job();
    let placement = Location { pos: Point3::new(5.0, 7.0, 0.0), rot: quat_from_euler(0.0, 0.0, 90f64.to_radians()) };
    h.execute(&world, SetWcsOrigin::new(&world, placement).expect("yaw WCS"));
    let start = Some((29.0, 19.0));
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, face_op(6.0, start, Some((-29.0, 19.0)))).expect("create"));
    let t = generate(&world, &mut h);
    assert!(near(entry(&t), (25.0, 25.0)), "the entry is beside the picked corner (30, 20): {:?}", entry(&t));
    assert!(near(leave(&t), (-25.0, 25.0)), "ten rows end on the exit's side, high Y: {:?}", leave(&t));
    h.execute(&world, EditOperationParams::new(&world, "Op1", face_op(6.0, start, Some((-29.0, -19.0)))).expect("edit"));
    let why = GenerateOperation::new(&world, "Op1").map(|_| ()).expect_err("refused").to_string();
    assert!(why.contains("exit point") && why.contains("10 rows"), "{why}");
}

#[test]
fn undo_and_redo_restore_the_previous_path() {
    let (world, mut h) = job();
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, face_op(6.0, None, None)).expect("create"));
    let auto = generate(&world, &mut h);
    assert_eq!(entry(&auto), (-35.0, -15.0), "the automatic corner");
    let picked_op = face_op(6.0, Some((29.0, 19.0)), None);
    h.execute(&world, EditOperationParams::new(&world, "Op1", picked_op.clone()).expect("edit"));
    let picked = generate(&world, &mut h);
    assert_eq!(entry(&picked), (35.0, 15.0));
    // Undo the generation and the edit: the automatic face and its path.
    h.undo(&world);
    h.undo(&world);
    assert!(params_of(&world).path_picks().is_empty(), "undo clears the pick");
    assert_eq!(path_of(&world), auto, "undo restores the previous path");
    h.redo(&world);
    h.redo(&world);
    assert_eq!(params_of(&world), picked_op, "redo sets the pick again");
    assert_eq!(path_of(&world), picked, "redo restores the picked path");
}

#[test]
fn a_tilted_face_refuses_a_pick_at_the_door() {
    let (world, _h) = job();
    for (start, exit, what) in [(Some((29.0, 19.0)), None, "start point"), (None, Some((29.0, 19.0)), "exit point")] {
        let mut tilted = face_op(6.0, start, exit);
        if let OpParams::Face { tilt, .. } = &mut tilted {
            *tilt = Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg: 90.0, origin: [0.0, 0.0, 0.0] });
        }
        match CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, tilted) {
            Ok(_) => panic!("a tilted face with a {what} is refused"),
            Err(e) => assert!(e.to_string().contains(what) && e.to_string().contains("tilted"), "{e}"),
        }
    }
}

/// The air entry law keeps its place under a pick: a 20 x 20 part in the
/// middle of the stock, faced from its high X, high Y corner by a tool
/// that is not centre-cutting. The entry runs on along the picked first
/// row, out past the stock's +X edge - never along the automatic row.
#[test]
fn the_air_entry_runs_on_along_the_picked_first_row() {
    use camcraft_app::commands::ImportModel;
    use camcraft_app::geometry::{cube, Point3};
    use camcraft_app::schema::Operation;
    use camcraft_app::tools::Tool;

    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == "EM-0800-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "CC", t.clone()).expect("tool"));
    h.execute(&world, AddTool::new(&world, "NC", Tool { center_cutting: false, ..t }).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-10.0, -10.0, 0.0), Point3::new(10.0, 10.0, 18.0))));
    let mut params = face_op(4.0, Some((9.0, 9.0)), None);
    if let OpParams::Face { clearance_z, cut_z, .. } = &mut params {
        *clearance_z = Height::fixed(30.0);
        *cut_z = Height::fixed(18.0);
    }
    let on_part = |tool: &str| Operation {
        tool: tool.into(),
        src: SrcRef::Entity("part".into()),
        params: params.clone(),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let (cc, _) = camcraft_app::operation::generate(&world, &on_part("CC"), "F").expect("centre-cutting face");
    assert_eq!(entry(&cc), (14.0, 6.0), "the picked corner, a radius out and a radius in");
    let (nc, _) = camcraft_app::operation::generate(&world, &on_part("NC"), "F").expect("the entry runs out to air");
    let at = entry(&nc);
    assert_eq!(at.1, 6.0, "the run-on stays on the picked first row");
    assert!(at.0 > 30.0 + 4.0, "and stands past the stock's +X edge, in air: {at:?}");
    assert_eq!(nc.moves.len(), cc.moves.len(), "only the entry point moved");
    assert_eq!(leave(&nc), leave(&cc), "the exit is the picked raster's own");
}

/// The first object under `v` that holds the key `key`.
fn holder<'a>(v: &'a mut Value, key: &str) -> Option<&'a mut Value> {
    if v.get(key).is_some() {
        return Some(v);
    }
    match v {
        Value::Object(m) => m.values_mut().find_map(|c| holder(c, key)),
        Value::Array(a) => a.iter_mut().find_map(|c| holder(c, key)),
        _ => None,
    }
}

/// A project file that carries a pick on a tilted face (written by hand,
/// or by a later version): it is refused by name before a path is cut -
/// the `sane` door reads every operation at load and at generation.
#[test]
fn a_file_with_a_pick_on_a_tilted_face_is_refused_by_name() {
    let (world, mut h) = job();
    let mut tilted = face_op(6.0, None, None);
    if let OpParams::Face { tilt, .. } = &mut tilted {
        *tilt = Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg: 0.0, origin: [0.0, 0.0, 0.0] });
    }
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, tilted).expect("create"));
    let path = std::env::temp_dir().join(format!("camcraft-face-tilt-pick-{}.camproj", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let mut file: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    let params = holder(&mut file, "stepover").expect("the face params");
    params["picks"] = json!({ "start": [[29.0, 19.0]] });
    std::fs::write(&path, file.to_string()).expect("write");
    let back = new_world();
    let loaded = camcraft_app::project::load(&back, &path);
    let _ = std::fs::remove_file(&path);
    let why = match loaded {
        // A door at load is as good as the generator's.
        Err(e) => e.to_string(),
        Ok(_) => GenerateOperation::new(&back, "Op1").map(|_| ()).expect_err("refused").to_string(),
    };
    assert!(why.contains("start point") && why.contains("tilted"), "{why}");
}

fn mcp(s: &mut Server, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("an answer");
    let r = reply["result"].clone();
    (r["isError"] == json!(true), r)
}

#[test]
fn mcp_edit_operation_sets_the_face_picks() {
    let mut s = Server::new();
    assert!(!mcp(&mut s, "set_stock", json!({ "box": { "dx": 60.0, "dy": 40.0, "dz": 20.0 } })).0);
    assert!(!mcp(&mut s, "add_tool", json!({ "catalog": "EM-1000-4FL" })).0);
    let (err, r) = mcp(&mut s, "add_operation", json!({ "kind": "Face", "tool": "EM-1000-4FL", "name": "F" }));
    assert!(!err, "{r}");
    let patch = json!({ "name": "F", "params": { "picks": { "start": [[29.0, 19.0]], "exit": [[-29.0, -19.0]] } } });
    let (err, r) = mcp(&mut s, "edit_operation", patch);
    assert!(!err, "the Face takes the picks: {r}");
    let text = r.to_string();
    assert!(text.contains("\"picks\"") && text.contains("29.0") && text.contains("-19.0"), "the edited params carry them: {text}");
    // The schema and the kind list name the Face among the kinds that take a pick.
    let reply = s.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} })).expect("an answer");
    let doc = reply["result"]["tools"]
        .as_array()
        .and_then(|t| t.iter().find(|t| t["name"] == "edit_operation"))
        .map(|t| t["inputSchema"]["properties"]["params"]["description"].as_str().unwrap_or_default().to_owned())
        .unwrap_or_default();
    assert!(doc.contains("Face: the start is the corner"), "{doc}");
    // Clearing puts the automatic raster back.
    let (err, r) = mcp(&mut s, "edit_operation", json!({ "name": "F", "params": { "picks": {} } }));
    assert!(!err, "clearing is taken: {r}");
}
