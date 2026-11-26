//! #1443: the V-Carve operation, end to end. A V-bit carves the top of
//! a letter-shaped part: the operation is created by the undoable
//! command and by the MCP tool, generates, is judged by the mark law
//! (`operation::mark_law`) and the program sim, and posts. The path of
//! the letter is pinned by a golden CL file.
//!
//! Provenance: our own fixture (the letter L as a prism, built in
//! code); no vendor material consulted.
//!
//! Gate map: covers app::operation::vcarve_op, app::operation::kinds
//! (the V-Carve row), app::operation::generate (the V-Carve arm,
//! depth_law_of), app::operation::mark_law, app::project (the VCarve
//! arm), app::operation::kept_rejudge, app::stock_sim, app::posting, app::mcp (list_operation_types,
//! add_operation, generate, verify).

mod common;
mod pocket_support;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpType, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

/// A 10 mm, 90 degree chamfer mill: the V-bit. Its cone is 5 mm high.
const VBIT: &str = "CM-1000-90";
const END_MILL: &str = "EM-0600-4FL";
const TOP: f64 = 8.0;

/// The letter L, strokes 4 mm wide, centred on the origin.
const LETTER: [(f64, f64); 6] = [(-6.0, -7.0), (6.0, -7.0), (6.0, -3.0), (-2.0, -3.0), (-2.0, 7.0), (-6.0, 7.0)];

/// The letter as a prism from Z0 to [`TOP`], closed and wound outward.
fn letter_part() -> Mesh {
    let ring: Vec<Point2> = LETTER.iter().map(|&(x, y)| Point2::new(x, y)).collect();
    let region = Region::new(vec![Loop::new(ring.clone()).expect("loop")]).expect("region");
    let mut f: Vec<Triangle> = Vec::new();
    for t in region.triangles().expect("triangles") {
        let at = |p: Point2, z: f64| Point3::new(p.x, p.y, z);
        // The region's triangles are counter-clockwise: up at the top.
        f.push(Triangle::new(at(t[0], TOP), at(t[1], TOP), at(t[2], TOP)));
        f.push(Triangle::new(at(t[0], 0.0), at(t[2], 0.0), at(t[1], 0.0)));
    }
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        let (a0, b0) = (Point3::new(a.x, a.y, 0.0), Point3::new(b.x, b.y, 0.0));
        let (a1, b1) = (Point3::new(a.x, a.y, TOP), Point3::new(b.x, b.y, TOP));
        f.push(Triangle::new(a0, b0, b1));
        f.push(Triangle::new(a0, b1, a1));
    }
    Mesh::from_facets(f)
}

fn job() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    for name in [VBIT, END_MILL] {
        let tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("tool");
        h.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    }
    h.execute(&world, ImportModel::from_mesh("part", letter_part()));
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 12.0, dy: 14.0, dz: TOP }, at).expect("stock"));
    (world, h)
}

/// A V-carve from `top` with the maximum depth `depth`, one level.
fn carve(top: f64, depth: f64, flat_clear: bool) -> OpParams {
    OpParams::VCarve {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(top),
        bottom_z: Height::fixed(top - depth),
        stepdown: 10.0,
        feed: 800.0,
        plunge_feed: 200.0,
        flat_clear,
        flat_stepover: 0.25,
        tilt: None,
        picks: Default::default(),
    }
}

fn whole() -> SrcRef {
    SrcRef::Entity("part".into())
}

/// Create and generate "V1"; its cached path and warnings.
fn generated(world: &World, h: &mut History, params: OpParams) -> camcraft_app::schema::Toolpath {
    generated_from(world, h, whole(), params)
}

fn generated_from(world: &World, h: &mut History, source: SrcRef, params: OpParams) -> camcraft_app::schema::Toolpath {
    h.execute(world, CreateOperation::new(world, "V1", VBIT, source, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "V1").expect("generate"));
    let e = model::find_by_name(world, "V1").expect("op");
    model::toolpath_of(world, &e).expect("cached path").clone()
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-v-carve-{tag}-{}.nc", std::process::id()))
}

#[test]
fn a_sixty_degree_carve_deeper_than_the_bit_radius_is_legal() {
    deep_carve_posts(whole(), "sixty-degree-part");
}

#[test]
fn a_sixty_degree_stock_carve_deeper_than_the_bit_radius_posts() {
    deep_carve_posts(SrcRef::Stock, "sixty-degree-stock");
}

fn deep_carve_posts(source: SrcRef, tag: &str) {
    let world = new_world();
    let mut h = History::default();
    let mut tool = catalog().into_iter().find(|(n, _)| *n == VBIT).map(|(_, t)| t).expect("V-bit");
    tool.tip_angle = 60.0;
    tool.flute_len = 12.0;
    h.execute(&world, AddTool::new(&world, VBIT, tool).expect("60 degree V-bit"));
    let part = cube(Point3::new(-10.0, -10.0, 0.0), Point3::new(10.0, 10.0, TOP));
    h.execute(&world, ImportModel::from_mesh("part", part));
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 20.0, dy: 20.0, dz: TOP }, at).expect("stock"));
    let tp = generated_from(&world, &mut h, source, carve(TOP, 6.0, false));
    // Six millimetres is deeper than the 5 mm radius, but less than
    // the cone's 8.66 mm height. The remaining part is 2 mm thick.
    let floor = tp.path.moves.iter().filter(|m| m.kind != MoveKind::Rapid).map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    assert!((floor - (TOP - 6.0)).abs() < 0.005, "the 6 mm carve reaches {floor}");
    let blocking: Vec<&Warning> = tp.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "{blocking:?}");
    let dev = stock_sim::simulate_document(&world).expect("the deep carve is judged").dev;
    assert_eq!(dev.gouge_cells, 0, "worst {} at {:?}", dev.max_gouge, dev.max_gouge_at);
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    posting::post_setup(&world, &out).unwrap_or_else(|e| panic!("a deep legal V-carve posts: {e}"));
    assert!(out.exists(), "the legal program is written");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn a_letter_is_carved_verified_simulated_and_posted() {
    let (world, mut h) = job();
    let tp = generated(&world, &mut h, carve(TOP, 3.0, false));
    let blocking: Vec<&Warning> = tp.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "{blocking:?}");
    // The strokes are 4 mm wide: under a 90 degree cone the spine is
    // 2 mm deep, less the 0.01 mm wall allowance. The maximum depth is
    // not met.
    let cuts = || tp.path.moves.iter().filter(|m| m.kind != MoveKind::Rapid);
    let spine = cuts().filter(|m| m.to.y > 1.0).map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    assert!((spine - (TOP - 1.99)).abs() < 0.005, "the spine of the upright stroke is at {spine}");
    // The junction of the two strokes is the widest place. Its circle
    // touches the two outer walls and the reflex corner: the radius is
    // 4 * sqrt(2) / (1 + sqrt(2)) = 2.343 mm. The tip is deepest there.
    let widest = 4.0 * 2.0_f64.sqrt() / (1.0 + 2.0_f64.sqrt());
    let floor = cuts().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    assert!((floor - (TOP - (widest - 0.01))).abs() < 0.005, "the junction is at {floor}");
    // The program sim: the cut stays in the mark the operation states.
    let dev = stock_sim::simulate_document(&world).expect("the stock is judged").dev;
    assert_eq!(dev.gouge_cells, 0, "worst {} at {:?}", dev.max_gouge, dev.max_gouge_at);
    let out = nc("letter");
    let _ = std::fs::remove_file(&out);
    posting::post_setup(&world, &out).unwrap_or_else(|e| panic!("a legal V-carve posts: {e}"));
    let text = std::fs::read_to_string(&out).expect("the program");
    let _ = std::fs::remove_file(&out);
    assert!(text.contains("G01 X4. Y-5. Z6.01"), "the spine of the lower stroke is posted: {text}");
}

#[test]
fn the_letter_matches_its_golden_cl_file() {
    let (world, mut h) = job();
    let tp = generated(&world, &mut h, carve(TOP, 1.0, true));
    common::write_golden("v_carve_letter_l.cl", &common::gen_to_cltext(&tp.path));
}

#[test]
fn the_operation_is_undone_and_done_again() {
    let (world, mut h) = job();
    h.execute(&world, CreateOperation::new(&world, "V1", VBIT, whole(), carve(TOP, 3.0, false)).expect("create"));
    assert!(model::find_by_name(&world, "V1").is_some());
    assert!(h.undo(&world));
    assert!(model::find_by_name(&world, "V1").is_none(), "undo removes the operation");
    assert!(h.redo(&world));
    assert!(model::find_by_name(&world, "V1").is_some(), "redo restores it");
}

#[test]
fn a_tool_that_is_not_a_v_bit_is_refused_by_name() {
    let (world, _h) = job();
    let Err(err) = CreateOperation::new(&world, "V1", END_MILL, whole(), carve(TOP, 3.0, false)) else {
        panic!("an end mill is not a V-bit");
    };
    let msg = err.to_string();
    assert!(msg.contains("V-Carve"), "{msg}");
    // The row accepts the cone and nothing else.
    let row = OpType::VCarve.info();
    assert!(row.accepts(camcraft_app::tools::ToolKind::Taper));
    assert!(!row.accepts(camcraft_app::tools::ToolKind::EndMill) && !row.accepts(camcraft_app::tools::ToolKind::BallNose));
    // Past the tool law, the generator's own door refuses it too.
    let mut h = History::default();
    common::create_past_the_tool_law(&world, &mut h, "V1", END_MILL, whole(), carve(TOP, 3.0, false));
    let err = common::generate_past_the_tool_law(&world, "V1").expect_err("the generator refuses an end mill");
    assert!(err.to_string().contains("needs a chamfer mill (V-bit)"), "{err}");
}

#[test]
fn a_top_typed_under_the_surface_is_a_gouge_that_holds_the_post() {
    // Top Z7 on a face at Z8, 0.5 mm stated: the cone would cut the
    // walls of the letter 1 mm wider than the region. The mark law
    // convicts it, and the post is refused.
    let (world, mut h) = job();
    let tp = generated(&world, &mut h, carve(7.0, 0.5, false));
    assert!(tp.warnings.iter().any(|w| matches!(w, Warning::Gouge { .. })), "{:?}", tp.warnings);
    let out = nc("low-top");
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(&world, &out).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{err}");
    assert!(!out.exists(), "blocked output is never written");
}

#[test]
fn a_saved_v_carve_opens_with_its_numbers() {
    let (world, mut h) = job();
    let params = carve(TOP, 1.5, true);
    h.execute(&world, CreateOperation::new(&world, "V1", VBIT, whole(), params.clone()).expect("create"));
    let path = std::env::temp_dir().join(format!("camcraft-v-carve-{}.camproj", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let again = new_world();
    camcraft_app::project::load(&again, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    let e = model::find_by_name(&again, "V1").expect("the operation");
    let op = model::operation_of(&again, &e).expect("its record");
    assert_eq!(op.params, params);
}

// ---- MCP ------------------------------------------------------------

fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    assert_ne!(result["isError"], json!(true), "{name} refused: {}", result["content"][0]["text"]);
    result.clone()
}

#[test]
fn over_mcp_a_letter_is_carved_verified_and_undone() {
    let dir = std::env::temp_dir().join(format!("camcraft-v-carve-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("part.stl");
    pocket_support::write_stl(&letter_part(), &stl);
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    let kinds = call(&mut s, &mut id, "list_operation_types", json!({}));
    let listed = kinds["content"][0]["text"].as_str().unwrap_or_default().to_string() + &kinds["structuredContent"].to_string();
    assert!(listed.contains("V-Carve") && listed.contains("v_carve"), "the kind is listed: {listed}");
    assert!(listed.contains("flat_clear") && listed.contains("flat_stepover"), "with its fields: {listed}");
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": stl.display().to_string() }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 12.0, "dy": 14.0, "dz": TOP } }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": VBIT }));
    let params = json!({ "top_z": TOP, "bottom_z": TOP - 3.0, "stepdown": 10.0, "flat_clear": false });
    call(&mut s, &mut id, "add_operation", json!({ "kind": "V-Carve", "tool": VBIT, "source": "part", "name": "V1", "params": params }));
    let g = call(&mut s, &mut id, "generate", json!({}))["structuredContent"].clone();
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
    let ver = call(&mut s, &mut id, "verify", json!({}))["structuredContent"].clone();
    assert_eq!(ver["ok"], json!(true), "a legal V-carve verifies ok: {ver}");
    let out = dir.join("letter.nc");
    call(&mut s, &mut id, "post", json!({ "output": out.display().to_string() }));
    assert!(out.exists(), "the program was written");
    // The MCP tool calls the undoable command: undo removes the operation.
    call(&mut s, &mut id, "undo", json!({}));
    call(&mut s, &mut id, "undo", json!({}));
    let _ = std::fs::remove_dir_all(&dir);
}
