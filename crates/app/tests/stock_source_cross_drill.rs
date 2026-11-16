//! #1635: a stock-source cross drill answers for the parts held in its
//! stock by the radial part law (`operation::cross_part`), the law a
//! part-source cross drill answers to - never for the stock it was read
//! off. Before the fix the law read the source's mesh: off the stock that
//! is the stock itself, so EVERY stock-source cross drill was refused at
//! generation ("rapids into the finished part"), the legal ones too - the
//! drill into the part's own modelled cross hole, and one through
//! stock-only material. Nothing posted unjudged (no P1); the legal
//! programs were false refusals.
//!
//! Now a stock-source cross drill is the chain a part-source cross drill
//! drills in the same raw stock: the setup's parts are the solid it
//! measures and keeps (each declared cross hole's round wall excused,
//! about its own axis), the source stock is the raw material its mouths
//! stand off. A hole into a part where the part models none is refused
//! by name, on an aligned and a turned stock alike; a hole into the
//! part's modelled hole, or one in stock-only material, posts.
//!
//! Provenance: our own punched-bar fixture (`punched_bar`, #117) over the
//! cross-part law; no vendor material consulted.
//!
//! Gate map: covers app::operation::keep_law (is_cross_drill,
//! is_stock_kept_kind, parts_in_stock), app::operation::cross_spindle
//! (Solids), app::operation::generate (the cross arm's solids, the
//! lathe reading), app::posting, app::mcp (add_operation, generate,
//! verify).
//! Lane shape: all tests here run in seconds.

mod pocket_support;
mod punched_bar;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice};
use camcraft_app::geometry::{quat_from_yaw, Mesh, Point3, IDENTITY_Q};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::mcp::Server;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project};
use flecs_ecs::prelude::World;
use serde_json::{json, Value};

const DRILL: &str = "DR-0400";
/// A 6 mm drill for the r3 hole: the bar's 32-gon mouth stands 0.014 mm
/// inside that circle, past the 0.01 gouge tolerance, so the drill at the
/// hole's size passes only by the cross law's wall excuse about the
/// hole's own axis.
const WIDE: &str = "DR-0600";

/// One r2 hole plunging -X from the r10 bar's face at z 0, 6 mm deep.
fn hole() -> CrossHole {
    CrossHole { origin: Point3::new(10.0, 0.0, 0.0), dir: Point3::new(-1.0, 0.0, 0.0), radius: 2.0, entry_d: 0.0, floor_d: -6.0, through: false }
}

/// [`hole`] at r3, for [`WIDE`].
fn wide_hole() -> CrossHole {
    CrossHole { radius: 3.0, ..hole() }
}

/// The r10 finished bar, z [-80, 20], with no cross hole modelled.
fn solid_bar() -> Mesh {
    punched_bar::punched_bar(10.0, punched_bar::FOOT, punched_bar::TOP, &[])
}

/// The r10 bar with [`hole`] modelled in it.
fn holed_bar() -> Mesh {
    punched_bar::punched_bar(10.0, punched_bar::FOOT, punched_bar::TOP, &[hole()])
}

/// An r3 core the 6 mm hole (floor at r4) never reaches: the hole is
/// in stock-only material.
fn thin_core() -> Mesh {
    punched_bar::punched_bar(3.0, punched_bar::FOOT, punched_bar::TOP, &[])
}

/// The raw stocks, each over the bar's Z span: an r20 bar and a 40 x 40
/// box.
fn stocks() -> [(&'static str, StockShape); 2] {
    [("bar", StockShape::Cylinder { diameter: 40.0, height: 100.0 }), ("box", StockShape::Box { dx: 40.0, dy: 40.0, dz: 100.0 })]
}

fn drill() -> OpParams {
    drill_of(hole())
}

fn drill_of(h: CrossHole) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![],
        cross: vec![h],
    }
}

/// A world on `word`'s live-tooling machine holding `part` as "bar" (or
/// no part) in `stock`, aligned or turned a quarter about Z (a rotation
/// the program sim refuses to model).
fn bench(word: &str, part: Option<Mesh>, stock: &StockShape, turned: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let rot = if turned { quat_from_yaw(std::f64::consts::FRAC_PI_2) } else { IDENTITY_Q };
    let at = Location { pos: Point3::new(0.0, 0.0, punched_bar::FOOT), rot };
    h.execute(&world, SetStock::new(&world, stock.clone(), at).expect("stock"));
    let r = |name: &str, axis: [f64; 3]| AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() };
    let axes = vec![r("A", [1.0, 0.0, 0.0]), r("B", [0.0, 1.0, 0.0]), r("C", [0.0, 0.0, 1.0])];
    let m = Machine { model: format!("TestBench {word}"), kind: Some(word.into()), axes, ..Default::default() };
    h.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    for name in [DRILL, WIDE] {
        let mut tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry");
        tool.flute_len = 30.0; // long series: the mouths on the raw stock stand 16 mm above the floors
        h.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    }
    if let Some(part) = part {
        h.execute(&world, ImportModel::from_mesh("bar", part));
    }
    (world, h)
}

/// Create and generate "S1" off `src`: its cached path, or the refusal.
fn generated(world: &World, h: &mut History, src: SrcRef) -> Result<Toolpath, String> {
    generated_with(world, h, src, DRILL, drill())
}

fn generated_with(world: &World, h: &mut History, src: SrcRef, tool: &str, params: OpParams) -> Result<Toolpath, String> {
    h.execute(world, CreateOperation::new(world, "S1", tool, src, params).expect("create"));
    let cmd = GenerateOperation::new(world, "S1").map_err(|e| e.to_string())?;
    h.execute(world, cmd);
    let e = model::find_by_name(world, "S1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    let blocking: Vec<_> = tp.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "a generated chain carries no blocking finding: {blocking:?}");
    Ok(tp.path.clone())
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-1635-{tag}-{}.nc", std::process::id()))
}

#[test]
fn a_stock_source_cross_drill_into_the_part_is_refused_by_name_on_both_stocks() {
    // The bar models no cross hole: the drill meets the part's steel at
    // the mouth. Before #1635 this was refused too - but for "rapids
    // into the finished part", the stock read as the solid to keep.
    for (sn, stock) in stocks() {
        for turned in [false, true] {
            let tag = format!("{sn} turned {turned}");
            let (world, mut h) = bench("mill-turn", Some(solid_bar()), &stock, turned);
            let e = generated(&world, &mut h, SrcRef::Stock).err().unwrap_or_else(|| panic!("{tag}: the cross hole into the part generated"));
            assert!(
                e.contains("cuts into the finished part") && e.contains("the model has no hole there"),
                "{tag}: refused by the part law, by name: {e}"
            );
            let out = nc(&format!("into-{sn}-{turned}"));
            let posted = posting::post_setup(&world, &out);
            assert!(posted.is_err() && !out.exists(), "{tag}: nothing is written");
        }
    }
}

#[test]
fn a_stock_source_cross_drill_into_the_parts_own_hole_or_stock_only_material_posts() {
    for (case, part) in [("in-hole", holed_bar()), ("stock-only", thin_core())] {
        for (sn, stock) in stocks() {
            for turned in [false, true] {
                let tag = format!("{case} {sn} turned {turned}");
                let (world, mut h) = bench("mill-turn", Some(part.clone()), &stock, turned);
                let t = generated(&world, &mut h, SrcRef::Stock).unwrap_or_else(|e| panic!("{tag}: a legal stock-source cross drill generates: {e}"));
                // The chain a part-source cross drill drills in the same
                // raw stock - the same law, the same standoffs.
                let (world2, mut h2) = bench("mill-turn", Some(part.clone()), &stock, turned);
                let p = generated(&world2, &mut h2, SrcRef::Entity("bar".into())).unwrap_or_else(|e| panic!("{tag}: part source: {e}"));
                assert_eq!(t.moves.len(), p.moves.len(), "{tag}: the part source's chain");
                for (i, (a, b)) in t.moves.iter().zip(&p.moves).enumerate() {
                    assert!(a.from == b.from && a.to == b.to && a.kind == b.kind, "{tag}: move {i} {a:?} vs {b:?}");
                }
                let out = nc(&format!("{case}-{sn}-{turned}"));
                let _ = std::fs::remove_file(&out);
                posting::post_setup(&world, &out).unwrap_or_else(|e| panic!("{tag}: a legal stock-source cross drill posts: {e}"));
                assert!(out.exists(), "{tag}: the program was written");
                let _ = std::fs::remove_file(&out);
            }
        }
    }
}

#[test]
fn a_stock_source_cross_drill_at_the_modelled_holes_size_is_excused_its_faceted_wall() {
    // The plain solid law (nothing excused) would convict this drill on
    // the r3 32-gon's flats; the cross law measures the declared hole's
    // wall about its own axis, as on a part source.
    let bar = punched_bar::punched_bar(10.0, punched_bar::FOOT, punched_bar::TOP, &[wide_hole()]);
    for (sn, stock) in stocks() {
        for turned in [false, true] {
            let tag = format!("{sn} turned {turned}");
            let (world, mut h) = bench("mill-turn", Some(bar.clone()), &stock, turned);
            generated_with(&world, &mut h, SrcRef::Stock, WIDE, drill_of(wide_hole()))
                .unwrap_or_else(|e| panic!("{tag}: the drill at the hole's size generates: {e}"));
            let out = nc(&format!("wide-{sn}-{turned}"));
            let _ = std::fs::remove_file(&out);
            posting::post_setup(&world, &out).unwrap_or_else(|e| panic!("{tag}: posts: {e}"));
            let _ = std::fs::remove_file(&out);
        }
    }
}

#[test]
fn a_stock_source_cross_drill_with_no_part_in_the_setup_keeps_nothing() {
    for (sn, stock) in stocks() {
        let (world, mut h) = bench("mill-turn", None, &stock, false);
        generated(&world, &mut h, SrcRef::Stock).unwrap_or_else(|e| panic!("{sn}: no part, nothing kept: {e}"));
    }
}

#[test]
fn a_lathe_reads_the_turned_part_in_a_box_stock_not_the_box() {
    // A plain lathe's cross drill stands on the turned reading of its
    // work (#67). Off a box stock that work is the bar the stock holds;
    // the box itself reads as no turned part and was refused for it.
    let (_, boxed) = &stocks()[1];
    let (world, mut h) = bench("lathe", Some(holed_bar()), boxed, false);
    generated(&world, &mut h, SrcRef::Stock).unwrap_or_else(|e| panic!("the turned bar in a box stock drills: {e}"));
    let (world, mut h) = bench("lathe", Some(solid_bar()), boxed, false);
    let e = generated(&world, &mut h, SrcRef::Stock).err().unwrap_or_default();
    assert!(e.contains("the model has no hole there"), "into the part, refused by name: {e}");
}

// ---- MCP ------------------------------------------------------------

fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Result<Value, String> {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    if result["isError"] == json!(true) {
        return Err(result["content"][0]["text"].as_str().unwrap_or_default().to_string());
    }
    Ok(result["structuredContent"].clone())
}

/// Over MCP (the built-in Mill-turn, a stock-source `add_operation` -
/// MCP's default source - with the cross list, an r12 raw bar at MCP's
/// stock origin, z [0, 100]): `generate`'s reply or its refusal.
fn mcp_generate(tag: &str, part: &Mesh) -> Result<Value, String> {
    let dir = std::env::temp_dir().join(format!("camcraft-1635-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("bar.stl");
    let up = |p: Point3| Point3::new(p.x, p.y, p.z + 50.0);
    let lifted = part.facets.iter().map(|t| camcraft_app::geometry::Triangle::new(up(t.a), up(t.b), up(t.c))).collect();
    pocket_support::write_stl(&Mesh::from_facets(lifted), &stl);
    let mut params = drill();
    if let OpParams::Drill { cross, clearance_z, top_z, .. } = &mut params {
        // The jobber drill's 12 mm flute: the r12 raw bar puts the mouth
        // 8 mm above the floor.
        cross[0].origin.z = 50.0;
        *clearance_z = Height::fixed(110.0);
        *top_z = Height::fixed(100.0);
    }
    let fields = project::params_to_json(&params).as_object().and_then(|o| o.values().next().cloned()).expect("the Drill's fields");
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    let ok = |r: Result<Value, String>, what: &str| r.unwrap_or_else(|e| panic!("{tag}: {what} refused: {e}"));
    ok(call(&mut s, &mut id, "import_model", json!({ "name": "bar", "path": stl.display().to_string() })), "import");
    ok(call(&mut s, &mut id, "set_stock", json!({ "cylinder": { "diameter": 24.0, "height": 100.0 } })), "stock");
    ok(call(&mut s, &mut id, "load_machine", json!({ "builtin": "Mill-turn" })), "machine");
    ok(call(&mut s, &mut id, "add_tool", json!({ "catalog": DRILL })), "tool");
    ok(call(&mut s, &mut id, "add_operation", json!({ "kind": "Drill", "tool": DRILL, "name": "S1", "params": fields })), "add");
    let g = call(&mut s, &mut id, "generate", json!({}));
    let _ = std::fs::remove_dir_all(&dir);
    g
}

#[test]
fn over_mcp_a_stock_source_cross_drill_answers_for_the_part_the_same() {
    let e = mcp_generate("into", &solid_bar()).err().unwrap_or_default();
    assert!(e.contains("cuts into the finished part") && e.contains("the model has no hole there"), "into the part, refused by name: {e}");
    let g = mcp_generate("in-hole", &holed_bar()).unwrap_or_else(|e| panic!("into the part's own hole generates: {e}"));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
}
