//! #1641: a part-source cross drill answers for the setup's OTHER parts.
//! #1621's neighbour law (`keep_law::neighbour_parts`) judged every
//! part-source kind that carries a part law - read as `depth_law_of` not
//! `None` - and a Drill with cross holes reads `None` (its radial law,
//! `cross_part`, is not a `DepthLaw`). So a cross hole drilled on part
//! B whose drill entered part A beside it in the same stock carried no
//! finding, and - the program sim not judging a posed (radial) chain -
//! POSTED, on an aligned and a turned stock alike (reproduced before the
//! fix: every "into A" case below generated clean and wrote its program).
//!
//! Now the chain is judged against the setup's other parts by the same
//! radial sweep (`cross_part::neighbours_clear`), nothing excused: the
//! declared holes are the source's own, so a round hole A models on the
//! same line is still A's steel to this op. Fixtures and a stock-only
//! model stay out (`neighbour_parts`' set).
//!
//! Provenance: our own punched-bar fixture (`punched_bar`, #117) beside
//! plain boxes and a tube; no vendor material consulted.
//!
//! Gate map: covers app::operation::keep_law (neighbour_parts,
//! kept_models), app::operation::cross_part (neighbours_clear),
//! app::operation::cross_spindle (Solids), app::operation::generate (the
//! cross arm's solids), app::posting,
//! app::mcp (add_operation with a model source, generate).
//! Lane shape: all tests here run in seconds.

mod pocket_support;
mod punched_bar;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice};
use camcraft_app::geometry::{quat_from_yaw, Mesh, Point3, Triangle, IDENTITY_Q};
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
/// The 6 mm drill for the r3 holes: a 32-gon wall of r3 stands 0.014 mm
/// inside the drill, past the gouge tolerance - passed only by the wall
/// excuse.
const WIDE: &str = "DR-0600";

/// One r2 hole plunging -X from B's face at z 0, 6 mm deep.
fn hole() -> CrossHole {
    CrossHole { origin: Point3::new(10.0, 0.0, 0.0), dir: Point3::new(-1.0, 0.0, 0.0), radius: 2.0, entry_d: 0.0, floor_d: -6.0, through: false }
}

fn wide_hole() -> CrossHole {
    CrossHole { radius: 3.0, ..hole() }
}

/// Part B: the r10 bar, z [-80, 20], with `h` modelled - the source's
/// own hole is legal.
fn part_b(h: CrossHole) -> Mesh {
    punched_bar::punched_bar(10.0, punched_bar::FOOT, punched_bar::TOP, &[h])
}

fn boxed(lo: [f64; 3], hi: [f64; 3]) -> Mesh {
    Mesh::from_facets(camcraft_app::accessory::mesh::cuboid(Point3::new(lo[0], lo[1], lo[2]), Point3::new(hi[0], hi[1], hi[2])))
}

/// Part A across the hole's line, inside the raw stock: the drill runs
/// through it on its way to B (A models no hole).
fn a_on_the_line() -> Mesh {
    boxed([12.0, -5.0, -5.0], [18.0, 5.0, 5.0])
}

/// Part A outside the r20 raw bar, between it and the station at r25:
/// only the approach (and retract) passes through it.
fn a_in_the_approach() -> Mesh {
    boxed([21.0, -5.0, -5.0], [24.0, 5.0, 5.0])
}

/// Part A legal beside the chain: across the bar on the far side, and
/// 2 mm over the r2 drill on the hole's own side.
fn a_beside() -> [(&'static str, Mesh); 2] {
    [("far side", boxed([-18.0, -5.0, -5.0], [-12.0, 5.0, 5.0])), ("over the line", boxed([12.0, -5.0, 4.0], [18.0, 5.0, 10.0]))]
}

/// Part A: a tube along the hole's line, x [12, 18], bore r3 as a 32-gon
/// (the r3 hole's own size) and outside r5 - A models a round hole
/// exactly where the drill runs, which is still A's wall to this op.
fn a_tube() -> Mesh {
    const N: usize = 32;
    let (x0, x1, ri, ro) = (12.0, 18.0, 3.0, 5.0);
    let at = |x: f64, r: f64, i: usize| {
        let th = std::f64::consts::TAU * (i % N) as f64 / N as f64;
        Point3::new(x, r * th.cos(), r * th.sin())
    };
    let mut f = Vec::new();
    let mut quad = |a: Point3, b: Point3, c: Point3, d: Point3| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    for i in 0..N {
        quad(at(x0, ro, i), at(x0, ro, i + 1), at(x1, ro, i + 1), at(x1, ro, i)); // outside
        quad(at(x0, ri, i), at(x1, ri, i), at(x1, ri, i + 1), at(x0, ri, i + 1)); // bore
        quad(at(x0, ri, i), at(x0, ri, i + 1), at(x0, ro, i + 1), at(x0, ro, i)); // -X end
        quad(at(x1, ri, i), at(x1, ro, i), at(x1, ro, i + 1), at(x1, ri, i + 1)); // +X end
    }
    Mesh::from_facets(f)
}

fn stocks() -> [(&'static str, StockShape); 2] {
    [("bar", StockShape::Cylinder { diameter: 40.0, height: 100.0 }), ("box", StockShape::Box { dx: 40.0, dy: 40.0, dz: 100.0 })]
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

/// A mill-turn bench holding B (with `h` modelled) and `a` in `stock`,
/// aligned or turned a quarter about Z (a rotation the program sim
/// refuses to model).
fn bench(b: Mesh, a: Mesh, stock: &StockShape, turned: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let rot = if turned { quat_from_yaw(std::f64::consts::FRAC_PI_2) } else { IDENTITY_Q };
    let at = Location { pos: Point3::new(0.0, 0.0, punched_bar::FOOT), rot };
    h.execute(&world, SetStock::new(&world, stock.clone(), at).expect("stock"));
    let r = |name: &str, axis: [f64; 3]| AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() };
    let axes = vec![r("A", [1.0, 0.0, 0.0]), r("B", [0.0, 1.0, 0.0]), r("C", [0.0, 0.0, 1.0])];
    let m = Machine { model: "TestBench mill-turn".into(), kind: Some("mill-turn".into()), axes, ..Default::default() };
    h.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    for name in [DRILL, WIDE] {
        let mut tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry");
        tool.flute_len = 30.0; // long series: the mouths on the raw stock stand 16 mm above the floors
        h.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    }
    h.execute(&world, ImportModel::from_mesh("B", b));
    h.execute(&world, ImportModel::from_mesh("A", a));
    (world, h)
}

/// Create and generate "S1" on part B: its refusal, or `Ok` when it
/// generated with no blocking finding.
fn generated(world: &World, h: &mut History, tool: &str, params: OpParams) -> Result<(), String> {
    h.execute(world, CreateOperation::new(world, "S1", tool, SrcRef::Entity("B".into()), params).expect("create"));
    let cmd = GenerateOperation::new(world, "S1").map_err(|e| e.to_string())?;
    h.execute(world, cmd);
    let e = model::find_by_name(world, "S1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    let blocking: Vec<_> = tp.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "a generated chain carries no blocking finding: {blocking:?}");
    Ok(())
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-1641-{}-{}.nc", tag.replace(' ', "-"), std::process::id()))
}

fn assert_refused(world: &World, tag: &str, e: &str) {
    assert!(
        e.contains("cross hole at (10, 0, 0) along (-1, 0, 0)") && e.contains("a neighbouring part") && e.contains("another part the setup holds"),
        "{tag}: refused by the neighbour law, by name: {e}"
    );
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let posted = posting::post_setup(world, &out);
    assert!(posted.is_err() && !out.exists(), "{tag}: nothing is written");
}

#[test]
fn a_cross_drill_on_b_into_part_a_is_refused_by_name_on_both_stocks() {
    for (sn, stock) in stocks() {
        for turned in [false, true] {
            let tag = format!("line {sn} turned {turned}");
            let (world, mut h) = bench(part_b(hole()), a_on_the_line(), &stock, turned);
            let e = generated(&world, &mut h, DRILL, drill_of(hole())).err().unwrap_or_else(|| panic!("{tag}: the drill through A generated"));
            assert_refused(&world, &tag, &e);
        }
    }
}

#[test]
fn a_cross_drill_whose_approach_crosses_part_a_is_refused_on_both_stocks() {
    // The r20 raw bar: A stands between the stock's face and the r25
    // station, so only the approach and retract cross it.
    let (sn, stock) = stocks()[0].clone();
    for turned in [false, true] {
        let tag = format!("approach {sn} turned {turned}");
        let (world, mut h) = bench(part_b(hole()), a_in_the_approach(), &stock, turned);
        let e = generated(&world, &mut h, DRILL, drill_of(hole())).err().unwrap_or_else(|| panic!("{tag}: the approach through A generated"));
        assert_refused(&world, &tag, &e);
    }
}

#[test]
fn a_neighbours_round_hole_on_the_line_is_not_excused() {
    // B's own r3 hole at the drill's size is excused its faceted wall
    // (#1635's pin, on a part source here); A's identical bore on the
    // same line is A's steel: only the source's declared holes excuse.
    for (sn, stock) in stocks() {
        for turned in [false, true] {
            let tag = format!("tube {sn} turned {turned}");
            let (world, mut h) = bench(part_b(wide_hole()), a_tube(), &stock, turned);
            let e = generated(&world, &mut h, WIDE, drill_of(wide_hole())).err().unwrap_or_else(|| panic!("{tag}: the drill through A's bore generated"));
            assert_refused(&world, &tag, &e);
        }
    }
}

#[test]
fn a_cross_drill_on_b_clear_of_part_a_posts_on_both_stocks() {
    for (case, a) in a_beside() {
        for (sn, stock) in stocks() {
            for turned in [false, true] {
                let tag = format!("{case} {sn} turned {turned}");
                let (world, mut h) = bench(part_b(hole()), a.clone(), &stock, turned);
                generated(&world, &mut h, DRILL, drill_of(hole())).unwrap_or_else(|e| panic!("{tag}: the legal drill generates: {e}"));
                let out = nc(&tag);
                let _ = std::fs::remove_file(&out);
                posting::post_setup(&world, &out).unwrap_or_else(|e| panic!("{tag}: the legal drill posts: {e}"));
                assert!(out.exists(), "{tag}: the program was written");
                let _ = std::fs::remove_file(&out);
            }
        }
    }
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

/// Over MCP (the built-in Mill-turn, `add_operation` with `"source":
/// "B"`, an r12 raw bar at MCP's stock origin, z [0, 100]; both parts
/// lifted 50 mm onto it): `generate`'s reply or its refusal. `a` is in
/// B's frame before the lift.
fn mcp_generate(tag: &str, a: &Mesh) -> Result<Value, String> {
    let dir = std::env::temp_dir().join(format!("camcraft-1641-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let up = |m: &Mesh| {
        let p = |p: Point3| Point3::new(p.x, p.y, p.z + 50.0);
        Mesh::from_facets(m.facets.iter().map(|t| Triangle::new(p(t.a), p(t.b), p(t.c))).collect())
    };
    let (b_stl, a_stl) = (dir.join("b.stl"), dir.join("a.stl"));
    pocket_support::write_stl(&up(&part_b(hole())), &b_stl);
    pocket_support::write_stl(&up(a), &a_stl);
    let mut params = drill_of(hole());
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
    ok(call(&mut s, &mut id, "import_model", json!({ "name": "B", "path": b_stl.display().to_string() })), "import B");
    ok(call(&mut s, &mut id, "import_model", json!({ "name": "A", "path": a_stl.display().to_string() })), "import A");
    ok(call(&mut s, &mut id, "set_stock", json!({ "cylinder": { "diameter": 24.0, "height": 100.0 } })), "stock");
    ok(call(&mut s, &mut id, "load_machine", json!({ "builtin": "Mill-turn" })), "machine");
    ok(call(&mut s, &mut id, "add_tool", json!({ "catalog": DRILL })), "tool");
    ok(call(&mut s, &mut id, "add_operation", json!({ "kind": "Drill", "tool": DRILL, "name": "S1", "source": "B", "params": fields })), "add");
    let g = call(&mut s, &mut id, "generate", json!({}));
    let _ = std::fs::remove_dir_all(&dir);
    g
}

#[test]
fn over_mcp_a_part_source_cross_drill_answers_for_the_neighbour_the_same() {
    // A across the line between the r12 stock's face and the r22
    // station (the approach), and the same box on the far side.
    let e = mcp_generate("into", &boxed([14.0, -4.0, -4.0], [20.0, 4.0, 4.0])).err().unwrap_or_default();
    assert!(e.contains("a neighbouring part") && e.contains("another part the setup holds"), "into A, refused by name: {e}");
    let g = mcp_generate("beside", &boxed([-20.0, -4.0, -4.0], [-14.0, 4.0, 4.0])).unwrap_or_else(|e| panic!("clear of A generates: {e}"));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
}
