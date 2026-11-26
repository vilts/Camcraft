//! #1759: arc turns and their radius as operation parameters of Face,
//! 2D Contour and Slot - set through the MCP `edit_operation` door (the
//! undoable EditOperationParams command), kept by the project file,
//! absent from a file that never set them, refused by name where no arc
//! can be cut, read by generation (the radius changes the path and the
//! cache stamp), and never changing a default operation.
//!
//! Gate map: covers app::mcp (edit_operation, save_project, open_project,
//! undo), app::commands (EditOperationParams, GenerateOperation, History),
//! app::operation (params, sane, generate), app::project (params,
//! params_impl).
//!
//! Provenance: general machining knowledge (a corner blended on a tangent
//! arc no larger than the cutter radius); our own command and project-file
//! laws for the rest. No vendor material.

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";
const KINDS: [&str; 3] = ["Face", "2D Contour", "Slot"];

fn mcp_call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
    (is_err, body)
}

/// A server with the test part, the tool and one operation of `kind`.
fn server_with(kind: &str) -> Server {
    let model = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/Milling-test.step")
        .to_string_lossy()
        .into_owned();
    let mut s = Server::new();
    let steps = [
        ("import_model", json!({ "path": model, "name": "part" })),
        ("add_tool", json!({ "catalog": TOOL })),
        ("add_operation", json!({ "kind": kind, "tool": TOOL, "source": "part", "name": "OP" })),
    ];
    for (i, (name, args)) in steps.into_iter().enumerate() {
        let (err, body) = mcp_call(&mut s, i as i64 + 1, name, args);
        assert!(!err, "{kind}: {name} refused: {body}");
    }
    s
}

/// The kind's field object out of a reply's tagged `params`.
fn fields(body: &Value) -> Value {
    body["params"].as_object().and_then(|m| m.values().next()).cloned().unwrap_or(Value::Null)
}

fn job_text(s: &mut Server, id: i64) -> String {
    let (err, body) = mcp_call(s, id, "get_job", json!({}));
    assert!(!err, "get_job refused: {body}");
    body.to_string()
}

#[test]
fn face_contour_and_slot_take_arc_turns_and_a_radius_through_edit_operation_and_undo() {
    for kind in KINDS {
        let mut s = server_with(kind);
        assert!(!job_text(&mut s, 5).contains("blend_radius"), "{kind}: a new operation sets no radius");
        let (err, body) =
            mcp_call(&mut s, 10, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": true, "blend_radius": 1.0 } }));
        assert!(!err, "{kind}: edit_operation refused arc turns: {body}");
        let f = fields(&body);
        assert_eq!(f["blend_turns"], json!(true), "{kind}: {body}");
        assert_eq!(f["blend_radius"], json!(1.0), "{kind}: {body}");

        let (err, body) = mcp_call(&mut s, 11, "undo", json!({}));
        assert!(!err, "{kind}: undo refused: {body}");
        let job = job_text(&mut s, 12);
        assert!(!job.contains("blend_radius") && !job.contains("blend_turns"), "{kind}: undo restores the operation without the option: {job}");
    }
}

#[test]
fn a_radius_no_control_can_cut_is_refused_by_name_on_each_kind() {
    for kind in KINDS {
        let mut s = server_with(kind);
        for bad in [json!(0.0), json!(-1.0), json!(0.001)] {
            let (err, body) =
                mcp_call(&mut s, 10, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": true, "blend_radius": bad } }));
            assert!(err, "{kind}: radius {bad} is refused, not cut: {body}");
            assert!(body.to_string().contains("corner arc radius"), "{kind}: the refusal names the radius: {body}");
        }
        // The flag is written only when ON; turning it off is still a
        // kept edit.
        let (err, body) = mcp_call(&mut s, 21, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": true } }));
        assert!(!err, "{kind}: turn on refused: {body}");
        let (err, body) = mcp_call(&mut s, 22, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": false } }));
        assert!(!err, "{kind}: turn off refused: {body}");
        assert!(fields(&body).get("blend_turns").is_none(), "{kind}: the option is off again: {body}");
    }
}

#[test]
fn the_project_file_keeps_the_radius_and_a_default_file_has_no_key() {
    for kind in KINDS {
        let mut s = server_with(kind);
        let tag = kind.replace(' ', "-");
        let file = std::env::temp_dir().join(format!("camcraft-arc-turn-kinds-{tag}-{}.camproj", std::process::id()));
        let path = file.to_string_lossy().into_owned();

        let (err, body) = mcp_call(&mut s, 10, "save_project", json!({ "path": path }));
        assert!(!err, "{kind}: save refused: {body}");
        let saved = std::fs::read_to_string(&file).expect("read saved project");
        assert!(!saved.contains("blend_radius") && !saved.contains("blend_turns"), "{kind}: a default file states neither key");

        let (err, body) =
            mcp_call(&mut s, 11, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": true, "blend_radius": 1.5 } }));
        assert!(!err, "{kind}: edit refused: {body}");
        let (err, body) = mcp_call(&mut s, 12, "save_project", json!({ "path": path }));
        assert!(!err, "{kind}: save refused: {body}");
        let saved = std::fs::read_to_string(&file).expect("read saved project");
        assert!(saved.contains("\"blend_radius\": 1.5") && saved.contains("\"blend_turns\": true"), "{kind}: the option is in the file: {saved}");

        let mut loaded = Server::new();
        let (err, body) = mcp_call(&mut loaded, 1, "open_project", json!({ "path": path }));
        let _ = std::fs::remove_file(&file);
        assert!(!err, "{kind}: open refused: {body}");
        let job = job_text(&mut loaded, 2);
        assert!(job.contains("blend_radius"), "{kind}: the loaded operation keeps the radius: {job}");
    }
}

// ---- generation reads the radius ------------------------------------------

const MILL: &str = "EM-0400-2FL";

fn face_params(blend: bool, radius: Option<f64>) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: blend,
        blend_radius: radius,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

fn slot_params(blend: bool, radius: Option<f64>) -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: camcraft_app::geo2d::Point2::new(15.0, 30.0),
        b: camcraft_app::geo2d::Point2::new(45.0, 30.0),
        width: 12.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(18.0),
        stepdown: 2.0,
        stepover: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: blend,
        blend_radius: radius,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

/// 60x60x20 box stock, a 4 mm end mill, one generated operation.
fn generated(params: OpParams) -> World {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, MILL, tool).expect("add tool"));
    history.execute(&world, CreateOperation::new(&world, "OP", MILL, SrcRef::Stock, params).expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    world
}

/// The arc radii of the generated path, to the micron.
fn arc_radii(world: &World) -> Vec<i64> {
    let e = model::find_by_name(world, "OP").expect("op");
    assert_eq!(operation::status_of(world, &e), OpStatus::Ok);
    let cache = model::toolpath_of(world, &e).expect("cached path");
    assert!(cache.warnings.is_empty(), "{:?}", cache.warnings);
    cache
        .path
        .moves
        .iter()
        .filter_map(|m| m.arc.map(|a| ((m.from.x - a.center.x).hypot(m.from.y - a.center.y) * 1000.0).round() as i64))
        .collect()
}

/// The Slot's arcs split by kind, to the micron: the #2906 wall lap's arcs
/// (centered on a slot end, `slot_params`' `a` and `b`) and the #1759 turn
/// arcs (all others).
fn slot_arcs(world: &World) -> (Vec<i64>, Vec<i64>) {
    let e = model::find_by_name(world, "OP").expect("op");
    assert_eq!(operation::status_of(world, &e), OpStatus::Ok);
    let cache = model::toolpath_of(world, &e).expect("cached path");
    assert!(cache.warnings.is_empty(), "{:?}", cache.warnings);
    let on_end = |x: f64, y: f64| [(15.0, 30.0), (45.0, 30.0)].iter().any(|&(ex, ey)| (x - ex).abs() < 1e-9 && (y - ey).abs() < 1e-9);
    let (mut lap, mut turns) = (Vec::new(), Vec::new());
    for m in &cache.path.moves {
        let Some(a) = m.arc else { continue };
        let r = ((m.from.x - a.center.x).hypot(m.from.y - a.center.y) * 1000.0).round() as i64;
        if on_end(a.center.x, a.center.y) { lap.push(r) } else { turns.push(r) }
    }
    (lap, turns)
}

#[test]
fn generation_cuts_the_face_and_the_slot_turns_at_the_radius_asked() {
    // Face: 3 mm rows, the last one snapped 2 mm off. No radius is the
    // semicircle every earlier file turns on (1.5 mm, 1 mm at the last
    // turn); 1 mm is two quarter arcs a 3 mm turn, and the last turn's
    // semicircle.
    assert!(arc_radii(&generated(face_params(false, Some(1.0)))).is_empty());
    let semi = arc_radii(&generated(face_params(true, None)));
    let wide = semi.iter().filter(|r| **r == 1500).count();
    assert!(wide > 0 && semi.iter().all(|r| *r == 1500 || *r == 1000), "{semi:?}");
    let corners = arc_radii(&generated(face_params(true, Some(1.0))));
    assert_eq!(corners.len(), semi.len() + wide, "two quarter arcs a 3 mm turn");
    assert!(corners.iter().all(|r| *r == 1000), "{corners:?}");

    // Slot: off is the sharp rows and the #2906 wall lap; on, no turn arc
    // is larger than the radius, and the wall lap stays, arc for arc.
    let (lap, sharp) = slot_arcs(&generated(slot_params(false, Some(0.5))));
    assert!(sharp.is_empty() && !lap.is_empty(), "lap {lap:?}, turns {sharp:?}");
    let (lap_on, turns) = slot_arcs(&generated(slot_params(true, Some(0.5))));
    assert_eq!(lap_on, lap, "the arc turns lost the wall lap");
    assert!(!turns.is_empty() && turns.iter().all(|r| *r <= 500) && turns.contains(&500), "{turns:?}");
    let (lap_tool, tool) = slot_arcs(&generated(slot_params(true, None)));
    assert_eq!(lap_tool, lap, "the arc turns lost the wall lap");
    assert!(!tool.is_empty() && tool.iter().all(|r| *r <= 2000), "{tool:?}");
}

/// The generator owns the bound by the tool radius. A radius past it
/// goes through the parameter door and is refused at generation, by
/// name, on each kind: generation reads the radius it was given.
#[test]
fn generation_refuses_a_radius_past_the_tool_radius_by_name_on_each_kind() {
    for kind in KINDS {
        let mut s = server_with(kind);
        let (err, body) = mcp_call(&mut s, 9, "set_stock", json!({ "box": { "dx": 200.0, "dy": 200.0, "dz": 60.0 } }));
        assert!(!err, "{kind}: set_stock refused: {body}");
        let (err, body) =
            mcp_call(&mut s, 10, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": true, "blend_radius": 100.0 } }));
        assert!(!err, "{kind}: the parameter door does not know the tool: {body}");
        let (err, body) = mcp_call(&mut s, 11, "generate", json!({ "name": "OP" }));
        assert!(err, "{kind}: a 100 mm corner arc under a 6 mm tool is refused, not cut: {body}");
        let why = body.to_string();
        assert!(why.contains("corner arc radius") && why.contains("tool radius"), "{kind}: {why}");
        // Off, the radius is not read: the same operation generates as
        // it does without one.
        let (err, body) = mcp_call(&mut s, 12, "edit_operation", json!({ "name": "OP", "params": { "blend_turns": false } }));
        assert!(!err, "{kind}: turn off refused: {body}");
        let (_, off) = mcp_call(&mut s, 13, "generate", json!({ "name": "OP" }));
        assert!(!off.to_string().contains("corner arc radius"), "{kind}: the radius was read with the option off: {off}");
    }
}

fn contour_params(blend: bool, radius: Option<f64>) -> OpParams {
    OpParams::Contour {
        chain_overrides: Vec::new(),
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Height::fixed(22.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(18.0),
        stepdown: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: camcraft_app::machining2d::Side::Auto,
        lead: camcraft_app::machining2d::Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: blend,
        blend_radius: radius,
        links: camcraft_app::links::Links::PLANAR,
        picks: Default::default(),
    }
}

#[test]
fn the_stamp_tells_a_radius_from_none_and_a_default_op_keeps_its_stamp() {
    let world = generated(face_params(false, None));
    let stamp = |p: &OpParams| operation::current_stamp(&world, MILL, &SrcRef::Stock, p, "OP", None);
    for (off, on, at) in [
        (face_params(false, None), face_params(true, None), face_params(true, Some(1.0))),
        (slot_params(false, None), slot_params(true, None), slot_params(true, Some(1.0))),
        (contour_params(false, None), contour_params(true, None), contour_params(true, Some(1.0))),
    ] {
        assert!(stamp(&off).is_some());
        assert_ne!(stamp(&off), stamp(&on), "a sharp cache never answers for a blended op");
        assert_ne!(stamp(&on), stamp(&at), "a cache at one radius never answers for another");
    }
    // Off, the radius is not read: the stamp is the default operation's.
    assert_eq!(stamp(&face_params(false, None)), stamp(&face_params(false, Some(1.0))));
    assert_eq!(stamp(&slot_params(false, None)), stamp(&slot_params(false, Some(1.0))));
    assert_eq!(stamp(&contour_params(false, None)), stamp(&contour_params(false, Some(1.0))));
}
