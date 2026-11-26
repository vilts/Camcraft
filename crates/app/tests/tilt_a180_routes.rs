//! #2599: on an A-only machine a slot at A0, A90, A-90 and A0/B180 posts
//! the same local XY, by each route that builds or edits a tilt: a new
//! operation (basis 1), and an operation loaded from a file without a
//! `basis_version` (the legacy basis 0) whose angles the user then edits,
//! through the command door (the operation page) and through MCP.
//! Provenance: our own tests over our own commands, MCP server, project
//! file and frame law (#2133 / PR #2157).
//!
//! Gate map (M4.4): covers app::commands (EditOperationParams), app::mcp
//! (open_project, edit_operation, save_project), app::project,
//! app::posting, cl::frame, post::post (tormach.cps).

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, LoadMachine,
    SelectPost, SetStock,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef, Tilt};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, posting, project};
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

const TOOL: &str = "EM 6";

fn slot(tilt: Option<Tilt>) -> OpParams {
    let above = |mm: f64| Height::of(HeightReference::TopOfStock, mm);
    OpParams::Slot {
        a: Point2::new(10.0, 0.0),
        b: Point2::new(30.0, 0.0),
        width: 6.0,
        stock_to_leave: 0.0,
        clearance_z: above(10.0),
        retract_z: None,
        top_z: above(0.0),
        bottom_z: above(-0.5),
        stepdown: 0.5,
        stepover: 3.0,
        feed: 400.0,
        plunge_feed: 300.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0, blend_turns: false, blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt,
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

fn tilt(basis_version: u8, a_deg: f64, b_deg: f64) -> Option<Tilt> {
    Some(Tilt { basis_version, a_deg, b_deg, origin: [0.0; 3] })
}

/// A 50 mm bar along machine X on the Tormach 770MX + microARC 4 (A only).
fn bar_world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, "Tormach 770MX + microARC 4").expect("machine"));
    h.execute(
        &world,
        SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())).expect("post"),
    );
    let stock = SetStock::new(
        &world,
        StockShape::Box { dx: 60.0, dy: 50.0, dz: 50.0 },
        Location { pos: Point3::new(20.0, 0.0, -25.0), ..Location::default() },
    )
    .expect("stock");
    h.execute(&world, stock);
    let (_, mut t) = catalog().into_iter().next().expect("catalog tool");
    t.diameter = 6.0;
    t.shank_d = 6.0;
    t.flute_len = 20.0;
    t.rpm = 6000.0;
    t.feed = 400.0;
    h.execute(&world, AddTool::new(&world, TOOL, t).expect("tool"));
    (world, h)
}

fn create(world: &World, h: &mut History, name: &str, t: Option<Tilt>) {
    h.execute(world, CreateOperation::new(world, name, TOOL, SrcRef::Stock, slot(t)).expect(name));
}

fn generate_all(world: &World, h: &mut History) {
    for (name, _) in camcraft_app::setup::operation_order(world) {
        h.execute(world, GenerateOperation::new(world, &name).expect("generate"));
    }
}

/// The modal machine (X, Y) at each G1 line of each operation section,
/// by operation name.
fn posted_xy(program: &str) -> Vec<(String, Vec<(String, String)>)> {
    program
        .split("(OPERATION: ")
        .skip(1)
        .map(|section| {
            let name = section.split(" - ").next().unwrap_or_default().to_owned();
            let (mut x, mut y) = (String::new(), String::new());
            let mut feeds = Vec::new();
            for line in section.lines() {
                for word in line.split_whitespace() {
                    if let Some(v) = word.strip_prefix('X') {
                        x = v.to_owned();
                    } else if let Some(v) = word.strip_prefix('Y') {
                        y = v.to_owned();
                    }
                }
                if line.starts_with("G1 ") {
                    feeds.push((x.clone(), y.clone()));
                }
            }
            (name, feeds)
        })
        .collect()
}

/// Each section posts the untilted section's XY, and that XY runs the
/// slot from X10 Y0 to X30 Y0.
fn assert_same_local_xy(world: &World) {
    let out = std::env::temp_dir().join(format!("camcraft_2599_{}.nc", std::process::id()));
    let posted = posting::post_setup(world, &out).expect("posts on microARC 4");
    let _ = std::fs::remove_file(&out);
    let sections = posted_xy(&posted.program);
    let Some((_, reference)) = sections.iter().find(|(n, _)| n == "A0") else {
        panic!("no A0 section in:
{}", posted.program);
    };
    assert!(
        reference.contains(&("10".into(), "0".into())) && reference.contains(&("30".into(), "0".into())),
        "the untilted slot runs X10 Y0 -> X30 Y0: {reference:?}"
    );
    for (name, feeds) in &sections {
        assert_eq!(feeds, reference, "{name} must post the untilted XY");
    }
}

#[test]
fn a0_a90_a_minus_90_and_a0_b180_post_the_same_local_xy() {
    let (world, mut h) = bar_world();
    create(&world, &mut h, "A0", None);
    create(&world, &mut h, "A90", tilt(1, 90.0, 0.0));
    create(&world, &mut h, "A-90", tilt(1, -90.0, 0.0));
    create(&world, &mut h, "A0B180", tilt(1, 0.0, 180.0));
    generate_all(&world, &mut h);
    assert_same_local_xy(&world);
}

/// A saved project whose `Legacy` tilt has no `basis_version`: the file
/// shape that a pre-#2133 CamCraft wrote.
fn legacy_file(world: &World) -> String {
    let mut file: Value = serde_json::from_str(&project::save_text(world).expect("save")).expect("json");
    let entity = file["entities"]
        .as_array_mut()
        .expect("entities")
        .iter_mut()
        .find(|e| e["name"] == "Legacy")
        .expect("legacy operation");
    let tilt = entity["operation"]["params"]["slot"]["tilt"].as_object_mut().expect("tilt");
    assert!(tilt.remove("basis_version").is_some(), "the current file states the basis");
    file.to_string()
}

fn opened(text: &str) -> World {
    let world = camcraft_app::world::new_world();
    project::open(&world, text).expect("open saved project");
    world
}

fn legacy_tilt(world: &World) -> Tilt {
    let entity = model::find_by_name(world, "Legacy").expect("legacy operation");
    model::operation_of(world, &entity).expect("definition").params.tilt().expect("tilt")
}

#[test]
fn a_legacy_tilt_edited_to_a0_b180_on_the_operation_page_posts_the_same_local_xy() {
    let (world, mut h) = bar_world();
    create(&world, &mut h, "A0", None);
    create(&world, &mut h, "Legacy", tilt(1, 90.0, 0.0));
    let world = opened(&legacy_file(&world));
    assert_eq!(legacy_tilt(&world).basis_version, 0, "a file without the key loads the legacy basis");
    let mut h = History::default();
    // The page edits the angle fields of the loaded tilt in place.
    let mut edited = legacy_tilt(&world);
    edited.a_deg = 0.0;
    edited.b_deg = 180.0;
    h.execute(&world, EditOperationParams::new(&world, "Legacy", slot(Some(edited))).expect("edit"));
    assert_eq!(legacy_tilt(&world).basis_version, 1, "new angles take the current basis");
    h.undo(&world);
    assert_eq!(legacy_tilt(&world).basis_version, 0, "undo restores the legacy tilt");
    assert_eq!((legacy_tilt(&world).a_deg, legacy_tilt(&world).b_deg), (90.0, 0.0));
    h.redo(&world);
    generate_all(&world, &mut h);
    assert_same_local_xy(&world);
}

#[test]
fn a_legacy_tilt_keeps_its_basis_when_its_angles_do_not_change() {
    let (world, mut h) = bar_world();
    create(&world, &mut h, "Legacy", tilt(1, 0.0, 180.0));
    let world = opened(&legacy_file(&world));
    let mut h = History::default();
    let mut moved = legacy_tilt(&world);
    moved.origin = [1.0, 0.0, 0.0];
    h.execute(&world, EditOperationParams::new(&world, "Legacy", slot(Some(moved))).expect("edit"));
    assert_eq!(
        legacy_tilt(&world).basis_version,
        0,
        "an edit that keeps the angles keeps the saved plane coordinates"
    );
}

fn mcp_call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, String) {
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": args }
    });
    let reply = server.handle(&req).expect("mcp reply");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    let text = if is_err {
        result["content"][0]["text"].as_str().unwrap_or_default().to_owned()
    } else {
        result["structuredContent"].to_string()
    };
    (is_err, text)
}

#[test]
fn a_legacy_tilt_edited_to_a0_b180_over_mcp_posts_the_same_local_xy() {
    let (world, mut h) = bar_world();
    create(&world, &mut h, "A0", None);
    create(&world, &mut h, "Legacy", tilt(1, -90.0, 0.0));
    let dir = std::env::temp_dir();
    let old = dir.join(format!("camcraft_2599_legacy_{}.camproj", std::process::id()));
    let new = dir.join(format!("camcraft_2599_edited_{}.camproj", std::process::id()));
    std::fs::write(&old, legacy_file(&world)).expect("write legacy file");

    let mut server = Server::new();
    let (err, body) = mcp_call(&mut server, 1, "open_project", json!({ "path": old }));
    let _ = std::fs::remove_file(&old);
    assert!(!err, "open refused: {body}");
    // A partial patch: the merge keeps the loaded tilt's other fields.
    let (err, body) = mcp_call(
        &mut server,
        2,
        "edit_operation",
        json!({ "name": "Legacy", "params": { "tilt": { "a_deg": 0.0, "b_deg": 180.0 } } }),
    );
    assert!(!err, "edit refused: {body}");
    let (err, body) = mcp_call(&mut server, 3, "save_project", json!({ "path": new }));
    assert!(!err, "save refused: {body}");
    let text = std::fs::read_to_string(&new).expect("read saved project");
    let _ = std::fs::remove_file(&new);

    let world = opened(&text);
    let t = legacy_tilt(&world);
    assert_eq!((t.basis_version, t.a_deg, t.b_deg), (1, 0.0, 180.0));
    let mut h = History::default();
    generate_all(&world, &mut h);
    assert_same_local_xy(&world);
}
