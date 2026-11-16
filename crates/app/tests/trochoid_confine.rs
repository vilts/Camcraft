//! #1503: a Trochoidal slot stays inside its selection or is refused.
//! The maintainer's screenshot setup - a pocket floor face picked, the
//! slot A (-20, 0) to B (20, 0), 16 mm wide, a 6 mm end mill - runs
//! 3 mm past each end wall of the 50 x 25 pocket: generation refuses it
//! by name with the numbers (GUI command and MCP alike), and nothing
//! posts. A slot that fits its selection generates and posts clean (the
//! stock sim judges it, no false refusal). With no selection nothing
//! confines the slot, and then the post's stock-sim gouge clause
//! refuses one that runs into the model.
//!
//! Provenance: our own fixtures (a 80 x 60 x 20 block with a 50 x 25
//! pocket to z 10, floor face 1250 mm2).
//!
//! Gate map (#1503): covers app::operation (slot_fit, generate,
//! trochoid_op), app::posting / posting_sim (the gouge clause),
//! app::mcp (generate, verify, post), motion::machining2d
//! (trochoid_slot).

mod pocket_support;

use camcraft_app::geometry::feature;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::machining2d::TrochoidReturn;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, recognize};
use flecs_ecs::prelude::World;
use pocket_support::{block_with_pocket, rounded_rect, write_stl};
use serde_json::{json, Value};

const TOOL: &str = "EM-0600-4FL";

/// 80 x 60 x 20 block, a sharp 50 x 25 pocket to z 10.
fn part() -> camcraft_app::geometry::Mesh {
    block_with_pocket(40.0, 30.0, &rounded_rect(25.0, 12.5, 0.0, 1), 10.0, 20.0)
}

fn trochoid(a: f64, b: f64) -> OpParams {
    OpParams::TrochoidalSlot {
        a: Point2::new(a, 0.0),
        b: Point2::new(b, 0.0),
        width: 16.0,
        loop_width: None,
        step: 0.8,
        max_engagement: 90.0,
        ret: TrochoidReturn::Microlift { height: 0.3 },
        smoothing: 0.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Some(Height::fixed(25.0)),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 5.0,
        feed: 800.0,
        return_feed: 2000.0,
        plunge_feed: 200.0,
        climb: true,
    }
}

/// The part, a stock box round it, the 6 mm end mill, and "T1" from
/// `a` to `b` along X - on the pocket floor face when `pick`, else on
/// the whole model.
fn job(a: f64, b: f64, pick: bool) -> (World, History) {
    job_on(a, b, if pick { Pick::Floor } else { Pick::Model })
}

enum Pick {
    Floor,
    Rim,
    Model,
}

fn job_on(a: f64, b: f64, pick: Pick) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", part()));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 90.0, dy: 70.0, dz: 20.0 }, Default::default()).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, t).expect("tool"));
    let src = match pick {
        Pick::Floor => {
            let p = recognize::pockets_of(&world, "part").expect("recognised").into_iter().next().expect("the pocket");
            assert!((p.area - 1250.0).abs() < 1e-6, "the screenshot's 1250 mm2 floor: {}", p.area);
            SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(p.floor) }])
        }
        Pick::Rim => SrcRef::Selection(floor_rim(&world)),
        Pick::Model => SrcRef::Entity("part".into()),
    };
    h.execute(&world, CreateOperation::new(&world, "T1", TOOL, src, trochoid(a, b)).expect("create"));
    (world, h)
}

/// The pocket floor's rim at z 10: its four edge chains (the 50 x 25
/// outline, picked edge by edge).
fn floor_rim(world: &World) -> Vec<GeoSel> {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    let rim: Vec<_> = feature::chains(&f)
        .into_iter()
        .filter(|c| {
            let p = feature::chain_points(&f, c);
            p.iter().all(|q| (q[2] - 10.0).abs() < 1e-6 && q[0].abs() <= 25.0 + 1e-6 && q[1].abs() <= 12.5 + 1e-6)
        })
        .map(|c| GeoSel { entity: "part".into(), feature: FeatureSel::Chain(c.id) })
        .collect();
    assert_eq!(rim.len(), 4, "the rim's four edges");
    rim
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-1503-{tag}-{}.nc", std::process::id()))
}

#[test]
fn a_slot_longer_than_its_selected_pocket_face_is_refused_by_name() {
    let (world, _h) = job(-20.0, 20.0, true);
    let err = GenerateOperation::new(&world, "T1").map(|_| ()).expect_err("refused").to_string();
    assert!(
        err.contains("Trochoidal slot") && err.contains("does not fit its selection") && err.contains("8.000") && err.contains("5.000") && err.contains("3.000"),
        "refused by name with the numbers: {err}"
    );
    let e = model::find_by_name(&world, "T1").expect("op");
    assert!(model::toolpath_of(&world, &e).is_none(), "no path is cached for a refused slot");
    let out = nc("long");
    assert!(posting::post_setup(&world, &out).is_err(), "nothing posts");
    assert!(!out.exists());
}

#[test]
fn a_slot_that_fits_its_selection_generates_and_posts_clean() {
    // 24 mm of centre line + 16 mm wide: x +-20, y +-8 inside +-25 x +-12.5.
    let (world, mut h) = job(-12.0, 12.0, true);
    h.execute(&world, GenerateOperation::new(&world, "T1").expect("a fitting slot generates"));
    let e = model::find_by_name(&world, "T1").expect("op");
    let c = model::toolpath_of(&world, &e).expect("cached path");
    let bad: Vec<_> = c.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(bad.is_empty(), "{bad:?}");
    // The stock sim judges the program against the model: no gouge.
    let rep = posting::verification_report(&world);
    assert!(rep.problems.is_empty(), "{:?}", rep.problems);
    let out = nc("fits");
    posting::post_setup(&world, &out).expect("a fitting slot posts");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn a_slot_a_hundredth_past_the_end_wall_is_refused_and_the_exact_fit_generates() {
    // 16 mm wide, B at x 17.01: the cutter reaches 25.01, 0.01 mm into
    // the end wall - refused through generate; at 17.0 it is tangent.
    let (world, _h) = job(-17.0, 17.01, true);
    let err = GenerateOperation::new(&world, "T1").map(|_| ()).expect_err("0.01 mm past is refused").to_string();
    assert!(err.contains("does not fit its selection") && err.contains("0.010"), "{err}");
    let (world, mut h) = job(-17.0, 17.0, true);
    h.execute(&world, GenerateOperation::new(&world, "T1").expect("the exact fit generates"));
}

#[test]
fn an_edge_chain_pick_confines_the_slot_like_the_floor_face() {
    let (world, _h) = job_on(-20.0, 20.0, Pick::Rim);
    let err = GenerateOperation::new(&world, "T1").map(|_| ()).expect_err("refused").to_string();
    assert!(err.contains("does not fit its selection") && err.contains("3.000"), "{err}");
    let (world, mut h) = job_on(-12.0, 12.0, Pick::Rim);
    h.execute(&world, GenerateOperation::new(&world, "T1").expect("a fitting slot on the rim generates"));
}

#[test]
fn the_post_refuses_an_unconfined_trochoid_that_enters_the_model() {
    // No selection, nothing confines it: the slot generates, and the
    // post's stock-sim gouge clause stops it at the pocket's end walls.
    let (world, mut h) = job(-20.0, 20.0, false);
    h.execute(&world, GenerateOperation::new(&world, "T1").expect("an unconfined slot generates"));
    let rep = posting::verification_report(&world);
    assert!(rep.problems.iter().any(|p| p.starts_with("gouge: program cuts below the model")), "{:?}", rep.problems);
    let out = nc("unconfined");
    let why = posting::post_setup(&world, &out).map(|_| ()).expect_err("the post refuses a gouge");
    assert!(why.contains("gouge"), "{why}");
    assert!(!out.exists());
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    s.handle(&msg).expect("a response")["result"].clone()
}

#[test]
fn mcp_refuses_the_same_slot_by_name() {
    let dir = std::env::temp_dir().join(format!("camcraft-1503-mcp-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let stl = dir.join("part.stl");
    write_stl(&part(), &stl);
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    assert!(ok(&call(&mut s, 1, "import_model", json!({ "path": stl.display().to_string() }))));
    assert!(ok(&call(&mut s, 2, "set_stock", json!({ "box": { "dx": 90.0, "dy": 70.0, "dz": 20.0 } }))));
    assert!(ok(&call(&mut s, 3, "add_tool", json!({ "catalog": TOOL }))));
    let floor = call(&mut s, 4, "detect_pockets", json!({ "model": "part" }))["structuredContent"]["features"][0]["floor"].clone();
    assert!(floor.as_str().is_some_and(|f| f.starts_with("face:0x")), "{floor}");
    // The kind's seeded defaults are the screenshot's A, B and width.
    let add = call(&mut s, 5, "add_operation", json!({ "kind": "Trochoidal slot", "tool": TOOL, "name": "T1", "source": "part",
        "params": { "bottom_z": { "reference": "fixed", "offset": 10.0 } } }));
    assert!(ok(&add), "{add}");
    let set = call(&mut s, 6, "set_operation_geometry", json!({ "operation": "T1", "picks": [{ "model": "part", "id": floor }] }));
    assert!(ok(&set), "{set}");
    let generated = call(&mut s, 7, "generate", json!({ "name": "T1" }));
    let text = generated.to_string();
    assert!(text.contains("does not fit its selection") && text.contains("3.000"), "{text}");
    let post = call(&mut s, 8, "post", json!({ "post": "Fanuc", "output": dir.join("t.nc").display().to_string() }));
    assert_eq!(post["isError"], json!(true), "nothing posts: {post}");
    let _ = std::fs::remove_dir_all(&dir);
}
