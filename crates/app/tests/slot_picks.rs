//! A plain Slot picked on the part (#1504): two opposite pocket edges, or
//! the pocket's floor face, seed the slot between them (centre line,
//! length, width) in the pick's own undoable command; a slot too long,
//! too wide or placed past the picks is refused by name with its
//! numbers and never previewed; a picked slot cut below the pocket floor
//! is convicted against the part and holds the post; and with nothing
//! picked the typed endpoints rule exactly as before. The MCP pick tool
//! rides the same command.
//!
//! Gate map (#1504): covers app::operation (slot_pick, job, generate,
//! material_source), app::commands (SetOperationGeometry,
//! CreateOperation, EditOperationParams), app::mcp (pick, edit),
//! motion::machining2d (slot_room).

mod pocket_support;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, History, ImportModel, SetOperationGeometry, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Point3, feature};
use camcraft_app::links::Links;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use pocket_support::{block_with_pocket, rounded_rect, write_stl};
use serde_json::{Value, json};

const TOOL: &str = "EM-1000-4FL";
const EPS: f64 = 1e-6;

/// A 50 x 50 x [0, 20] block with a 30 x 16 pocket sunk to z 10.
fn pocket_mesh() -> camcraft_app::geometry::Mesh {
    block_with_pocket(25.0, 25.0, &rounded_rect(15.0, 8.0, 0.0, 1), 10.0, 20.0)
}

fn job() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", pocket_mesh()));
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 50.0, dz: 20.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    (world, h)
}

fn slot(a: (f64, f64), b: (f64, f64), width: f64, bottom: f64) -> OpParams {
    OpParams::Slot {
        links: Links::PLANAR,
        a: Point2::new(a.0, a.1),
        b: Point2::new(b.0, b.1),
        width,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(bottom),
        stepdown: 5.0,
        stepover: 3.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0, blend_turns: false, blend_radius: None,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

/// The pocket floor's two long edges (y = -8 and y = +8, z 10).
fn long_edges(world: &World) -> Vec<GeoSel> {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    [-8.0, 8.0]
        .into_iter()
        .map(|y| {
            let c = feature::chains(&f)
                .into_iter()
                .find(|c| {
                    let p = feature::chain_points(&f, c);
                    !c.closed && p.len() >= 2 && p.iter().all(|q| (q[2] - 10.0).abs() < EPS && (q[1] - y).abs() < EPS)
                })
                .unwrap_or_else(|| panic!("the floor edge at y {y} is its own open chain"));
            GeoSel { entity: "part".into(), feature: FeatureSel::Chain(c.id) }
        })
        .collect()
}

/// The pocket's floor face.
fn floor_face(world: &World) -> Vec<GeoSel> {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    let face = f
        .faces
        .iter()
        .find(|fa| {
            fa.normal[2] > 0.99
                && fa.loops.iter().any(|l| l.verts.iter().filter_map(|&v| f.verts.get(v as usize)).all(|p| (p[2] - 10.0).abs() < EPS))
        })
        .expect("a floor face");
    vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(face.id) }]
}

fn op(world: &World, name: &str) -> camcraft_app::schema::Operation {
    model::find_by_name(world, name).and_then(|e| model::operation_of(world, &e)).expect("op")
}

fn generate(world: &World, name: &str) -> Result<(Toolpath, Vec<Warning>), String> {
    operation::generate(world, &op(world, name), name).map_err(|e| e.to_string())
}

fn ends(p: &OpParams) -> (Point2, Point2, f64) {
    match p {
        OpParams::Slot { a, b, width, .. } => (*a, *b, *width),
        other => panic!("not a slot: {other:?}"),
    }
}

/// A Slot named S1 created on the part with typed numbers, then picked.
fn picked(world: &World, h: &mut History, picks: Vec<GeoSel>) {
    let typed = slot((-20.0, 0.0), (20.0, 0.0), 12.0, 10.0);
    h.execute(world, CreateOperation::new(world, "S1", TOOL, SrcRef::Entity("part".into()), typed).expect("create"));
    h.execute(world, SetOperationGeometry::new(world, "S1", picks).expect("pick"));
}

fn edit(world: &World, h: &mut History, p: OpParams) {
    h.execute(world, EditOperationParams::new(world, "S1", p).expect("edit"));
}

fn assert_fitted(p: &OpParams) {
    let (a, b, w) = ends(p);
    let (lo, hi) = if a.x < b.x { (a, b) } else { (b, a) };
    assert!((w - 16.0).abs() < EPS, "the width is the edges' distance: {p:?}");
    assert!((lo.x + 7.0).abs() < EPS && (hi.x - 7.0).abs() < EPS, "round ends tangent to the pocket's ends: {p:?}");
    assert!(lo.y.abs() < EPS && hi.y.abs() < EPS, "on the centre line: {p:?}");
}

/// Every feed stays inside the pocket: a 10 mm cutter's centre within
/// x +-10, y +-3, never below the floor.
fn assert_inside_pocket(t: &Toolpath) {
    let feeds: Vec<Point3> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed).flat_map(|m| [m.from, m.to]).collect();
    assert!(!feeds.is_empty());
    for p in feeds {
        assert!(p.x.abs() <= 10.0 + EPS && p.y.abs() <= 3.0 + EPS && p.z >= 10.0 - EPS, "feed leaves the pocket at {p:?}");
    }
}

#[test]
fn two_pocket_edges_give_a_slot_inside_the_pocket_and_undo_takes_it_back() {
    let (world, mut h) = job();
    picked(&world, &mut h, long_edges(&world));
    assert_fitted(&op(&world, "S1").params);
    let (t, warns) = generate(&world, "S1").expect("the fitted slot generates");
    assert!(warns.iter().all(|w| !w.blocks_post()), "the fitted slot verifies clean: {warns:?}");
    assert_inside_pocket(&t);
    assert!(operation::region_of(&world, &op(&world, "S1")).is_some(), "the preview draws the fitted slot");
    // One undo takes back the pick and the seed together.
    assert!(h.undo(&world));
    let back = op(&world, "S1");
    assert_eq!(back.src, SrcRef::Entity("part".into()));
    assert_eq!(ends(&back.params), (Point2::new(-20.0, 0.0), Point2::new(20.0, 0.0), 12.0));
}

#[test]
fn the_floor_face_gives_the_slot_that_fits_it() {
    let (world, mut h) = job();
    picked(&world, &mut h, floor_face(&world));
    assert_fitted(&op(&world, "S1").params);
    let (t, warns) = generate(&world, "S1").expect("generates");
    assert!(warns.iter().all(|w| !w.blocks_post()), "{warns:?}");
    assert_inside_pocket(&t);
}

#[test]
fn a_slot_longer_than_the_picked_edges_is_refused_by_name_and_not_previewed() {
    let (world, mut h) = job();
    picked(&world, &mut h, long_edges(&world));
    edit(&world, &mut h, slot((-20.0, 0.0), (20.0, 0.0), 16.0, 10.0));
    let e = generate(&world, "S1").expect_err("refused");
    assert!(e.contains("does not fit the selection") && e.contains("56.000 mm long end to end") && e.contains("share only 30.000 mm"), "{e}");
    assert!(operation::region_of(&world, &op(&world, "S1")).is_none(), "never previewed across the wall");
}

#[test]
fn a_slot_wider_than_the_picked_edges_is_refused_by_name() {
    let (world, mut h) = job();
    picked(&world, &mut h, long_edges(&world));
    edit(&world, &mut h, slot((-5.0, 0.0), (5.0, 0.0), 20.0, 10.0));
    let e = generate(&world, "S1").expect_err("refused");
    assert!(e.contains("20.000 mm wide") && e.contains("only 16.000 mm apart"), "{e}");
    assert!(operation::region_of(&world, &op(&world, "S1")).is_none());
}

#[test]
fn a_slot_through_the_pocket_wall_is_refused_and_one_below_the_floor_holds_the_post() {
    let (world, mut h) = job();
    picked(&world, &mut h, long_edges(&world));
    // Moved 4 mm sideways: its side runs into the wall.
    edit(&world, &mut h, slot((-5.0, 4.0), (5.0, 4.0), 16.0, 10.0));
    let e = generate(&world, "S1").expect_err("refused");
    assert!(e.contains("runs 4.000 mm outside the picked edges"), "{e}");
    // The fitted slot cut 5 mm below the floor enters the part: the
    // depth-aware gouge law convicts it, and the warning holds the post.
    edit(&world, &mut h, slot((-7.0, 0.0), (7.0, 0.0), 16.0, 5.0));
    let (_, warns) = generate(&world, "S1").expect("generates, convicted");
    assert!(warns.iter().any(|w| matches!(w, Warning::Gouge { .. }) && w.blocks_post()), "below the floor is a gouge: {warns:?}");
}

#[test]
fn with_nothing_picked_the_typed_endpoints_rule_as_before() {
    let (world, mut h) = job();
    // Across the whole part, through the pocket's walls at x 15, nothing
    // picked: the typed numbers rule (no pick law), and the model gouge
    // check decides. A Stock source is the material to remove - the
    // typed slot is cut as typed - but the part standing in the stock is
    // kept (#1607): through its pocket walls is a gouge.
    let typed = slot((-20.0, 0.0), (20.0, 0.0), 12.0, 10.0);
    h.execute(&world, CreateOperation::new(&world, "K1", TOOL, SrcRef::Stock, typed.clone()).expect("create"));
    assert_eq!(ends(&op(&world, "K1").params), (Point2::new(-20.0, 0.0), Point2::new(20.0, 0.0), 12.0));
    let (t, warns) = generate(&world, "K1").unwrap_or_else(|e| panic!("K1: {e}"));
    assert!(warns.iter().any(|w| matches!(w, Warning::Gouge { .. }) && w.blocks_post()), "K1: {warns:?}");
    let reach = t.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to.x.abs()).fold(0.0, f64::max);
    assert!(reach >= 20.0 - EPS, "K1: the typed slot is cut as typed, through the pocket wall at x 15: {reach}");
    assert!(operation::region_of(&world, &op(&world, "K1")).is_some());
}

#[test]
fn a_whole_part_slot_through_the_wall_is_convicted_and_holds_the_post() {
    // #1537: the source names the finished part, so the part around the
    // slot is kept - a typed slot through the pocket's wall at x 15 is a
    // blocking gouge, not a silent cut.
    let (world, mut h) = job();
    let typed = slot((-20.0, 0.0), (20.0, 0.0), 12.0, 10.0);
    h.execute(&world, CreateOperation::new(&world, "E1", TOOL, SrcRef::Entity("part".into()), typed).expect("create"));
    assert_eq!(ends(&op(&world, "E1").params), (Point2::new(-20.0, 0.0), Point2::new(20.0, 0.0), 12.0));
    let (_, warns) = generate(&world, "E1").expect("generates, convicted");
    assert!(warns.iter().any(|w| matches!(w, Warning::Gouge { .. }) && w.blocks_post()), "through the wall is a gouge: {warns:?}");
    // The same law passes a whole-part slot that stays inside the pocket.
    let inside = slot((-5.0, 0.0), (5.0, 0.0), 12.0, 10.0);
    h.execute(&world, CreateOperation::new(&world, "E2", TOOL, SrcRef::Entity("part".into()), inside).expect("create"));
    let (_, warns) = generate(&world, "E2").expect("generates");
    assert!(warns.iter().all(|w| !w.blocks_post()), "inside the pocket verifies clean: {warns:?}");
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a response");
        let result = &reply["result"];
        let is_err = result["isError"] == json!(true);
        (is_err, if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() })
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }
}

#[test]
fn mcp_picks_seed_and_confine_the_slot_through_the_same_command() {
    let dir = std::env::temp_dir().join(format!("camcraft-slot-picks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("part.stl");
    write_stl(&pocket_mesh(), &stl);
    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 50.0, "dy": 50.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": TOOL }));
    c.ok("add_operation", json!({ "kind": "slot", "tool": TOOL, "name": "S1", "source": "part", "params": {
        "width": 12.0, "top_z": 20.0, "bottom_z": 10.0, "stepdown": 5.0 } }));
    let f = c.ok("list_features", json!({ "model": "part", "kinds": ["chains"] }));
    let long: Vec<Value> = f["chains"]
        .as_array()
        .expect("chains")
        .iter()
        .filter(|ch| {
            ch["closed"] == json!(false)
                && ch["length"].as_f64().is_some_and(|l| (l - 30.0).abs() < 1e-6)
                && ch["bounds"]["max"][2].as_f64().is_some_and(|z| (z - 10.0).abs() < 1e-6)
        })
        .map(|ch| json!({ "model": "part", "id": ch["id"] }))
        .collect();
    assert_eq!(long.len(), 2, "the floor's two long edges: {f}");
    c.ok("set_operation_geometry", json!({ "operation": "S1", "picks": long }));
    let g = c.ok("generate", json!({ "name": "S1" }));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(true), "{ver}");
    let e = c.ok("edit_operation", json!({ "name": "S1", "params": { "width": 20.0 } }));
    assert!(e["params"].is_object(), "{e}");
    let (err, body) = c.call("generate", json!({ "name": "S1" }));
    let text = body.to_string();
    assert!(err || g != body, "a too-wide slot does not generate: {body}");
    assert!(text.contains("20.000 mm wide") && text.contains("16.000 mm apart"), "refused by name: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}
