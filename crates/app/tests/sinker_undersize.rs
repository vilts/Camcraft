//! #543 (part of #290): Sinker EDM rough and finish with undersize - two
//! Sinker Burns of one cavity, each with its own electrode and its own
//! machine condition. The condition's undersize sizes the electrode and
//! places its tip; the program verdict (the column engine's box atom)
//! sees the pair reach the finished size; a finish that would leave the
//! cavity under size is refused by name, and a finish no larger than the
//! rough it follows is warned by name. The pair's CL is golden.
//!
//! Gate map (#543): covers app::operation (sinker_op, sinker_pass,
//! stamp, params), app::project (sinker DTO), app::stock_sim
//! (simulate_document over the box atom), app::mcp (add_operation,
//! edit_operation, generate, undo over the sinker), motion::sinker,
//! motion::sinker_undersize, sim::verify (the SinkerFinishOrder warning).
//! Provenance: our own SYNTHETIC fixtures - the sinker template of #530
//! with invented conditions (no machine's manual) and a hand-built block
//! with a rectangular cavity. Not exercised on a real EDM control.

mod common;

#[path = "sinker_undersize/orbit_cases.rs"]
mod orbit_cases;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::{Height, OpParams, SinkerBurnOp, SinkerDatum, SinkerOrbit, SinkerOrbitKind, SinkerPass, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::Tool;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use camcraft_app::{model, project};

const SINKER: &str = include_str!("edm/sinker-edm.mch");
/// The finished cavity, mm: 20 x 12, 6 deep from the 20 mm work top.
const CAV: (f64, f64) = (20.0, 12.0);
/// The rough electrode: condition R1 undersizes 0.3 per side.
const ROUGH: &str = "E-R";
/// The finish electrode: condition F1 undersizes 0.02 per side.
const FINISH: &str = "E-F";

/// The sinker template with SYNTHETIC conditions (no manual's values).
fn sinker_machine() -> String {
    let mut v: serde_json::Value = serde_json::from_str(SINKER).expect("template json");
    v["edm"]["conditions"] = serde_json::json!([
        { "id": "R1", "name": "synthetic rough", "gap": 0.1, "undersize": 0.3, "role": "rough" },
        { "id": "F1", "name": "synthetic finish", "gap": 0.02, "undersize": 0.02, "role": "finish" },
        { "id": "FB", "name": "synthetic finish, undersized past its gap", "gap": 0.02, "undersize": 0.05, "role": "finish" },
        { "id": "FI", "name": "synthetic finish, as undersized as the rough", "gap": 0.3, "undersize": 0.3, "role": "finish" },
        { "id": "NU", "name": "synthetic, no undersize", "gap": 0.1 }
    ]);
    v.to_string()
}

/// The electrode a condition of undersize `u` calls for under the cavity.
fn electrode_for(u: f64) -> Tool {
    camcraft_tools::electrode::electrode_tool(CAV.0 - 2.0 * u, CAV.1 - 2.0 * u, 30.0, 15.0)
}

fn quad(out: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3) {
    out.push(Triangle::new(a, b, c));
    out.push(Triangle::new(a, c, d));
}

/// A 60 x 60 x 20 block (z 0..20) with the finished cavity, 6 deep,
/// centred on the origin.
fn block_with_cavity() -> Mesh {
    let p = Point3::new;
    let (hx, hy) = (CAV.0 * 0.5, CAV.1 * 0.5);
    let (s, top, fl) = (30.0, 20.0, 14.0);
    let mut f = Vec::new();
    quad(&mut f, p(-s, -s, top), p(-hx, -s, top), p(-hx, s, top), p(-s, s, top));
    quad(&mut f, p(hx, -s, top), p(s, -s, top), p(s, s, top), p(hx, s, top));
    quad(&mut f, p(-hx, -s, top), p(hx, -s, top), p(hx, -hy, top), p(-hx, -hy, top));
    quad(&mut f, p(-hx, hy, top), p(hx, hy, top), p(hx, s, top), p(-hx, s, top));
    quad(&mut f, p(-hx, -hy, fl), p(hx, -hy, fl), p(hx, hy, fl), p(-hx, hy, fl));
    quad(&mut f, p(-hx, -hy, fl), p(-hx, -hy, top), p(hx, -hy, top), p(hx, -hy, fl));
    quad(&mut f, p(hx, hy, fl), p(hx, hy, top), p(-hx, hy, top), p(-hx, hy, fl));
    quad(&mut f, p(-hx, hy, fl), p(-hx, hy, top), p(-hx, -hy, top), p(-hx, -hy, fl));
    quad(&mut f, p(hx, -hy, fl), p(hx, -hy, top), p(hx, hy, top), p(hx, hy, fl));
    quad(&mut f, p(-s, -s, 0.0), p(s, -s, 0.0), p(s, -s, top), p(-s, -s, top));
    quad(&mut f, p(s, s, 0.0), p(-s, s, 0.0), p(-s, s, top), p(s, s, top));
    quad(&mut f, p(-s, s, 0.0), p(-s, -s, 0.0), p(-s, -s, top), p(-s, s, top));
    quad(&mut f, p(s, -s, 0.0), p(s, s, 0.0), p(s, s, top), p(s, -s, top));
    quad(&mut f, p(-s, -s, 0.0), p(-s, s, 0.0), p(s, s, 0.0), p(s, -s, 0.0));
    Mesh::from_facets(f)
}

fn burn(pass: SinkerPass, condition: &str) -> OpParams {
    OpParams::SinkerBurn(SinkerBurnOp {
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(20.0),
        floor_z: Height::fixed(14.0),
        datum: SinkerDatum::WorkTop,
        depth: 6.0,
        x: 0.0,
        y: 0.0,
        feed: 40.0,
        condition: condition.into(),
        pass,
        cavity_x: CAV.0,
        cavity_y: CAV.1,
        cavity_outline: Vec::new(),
        layer: None,
        wear_lost: None,
        wear_over: None,
        orbit: None,
        picks: Default::default(),
    })
}

/// The part, its block of stock, the sinker, both electrodes.
fn sinker_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block_with_cavity()));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let m = parse_mch(&sinker_machine()).expect("sinker parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, AddTool::new(&world, ROUGH, electrode_for(0.3)).expect("rough electrode"));
    h.execute(&world, AddTool::new(&world, FINISH, electrode_for(0.02)).expect("finish electrode"));
    (world, h)
}

fn add(world: &World, h: &mut History, name: &str, tool: &str, p: OpParams) {
    h.execute(world, CreateOperation::new(world, name, tool, SrcRef::Stock, p).expect("a sinker burn on a sinker"));
}

fn generate(world: &World, h: &mut History, name: &str) {
    h.execute(world, GenerateOperation::new(world, name).expect("generates"));
}

fn path_of(world: &World, name: &str) -> camcraft_app::schema::Toolpath {
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("generated")
}

fn feed_tip(world: &World, name: &str) -> f64 {
    path_of(world, name)
        .path
        .moves
        .iter()
        .filter(|m| m.kind == camcraft_app::cl::MoveKind::Feed)
        .map(|m| m.to.z)
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn a_rough_finish_pair_reaches_the_finished_size_in_the_sim() {
    let (world, mut h) = sinker_doc();
    add(&world, &mut h, "Rough", ROUGH, burn(SinkerPass::Rough, "R1"));
    generate(&world, &mut h, "Rough");
    // The rough electrode stands its 0.3 undersize above the finished
    // floor: it burns 0.1 below its tip, leaving 0.2 on floor and walls.
    assert!((feed_tip(&world, "Rough") - 14.3).abs() < 1e-9, "rough tip {}", feed_tip(&world, "Rough"));
    let sim = simulate_document(&world).expect("judged");
    assert_eq!(sim.dev.gouge_cells, 0, "the rough gouges nothing: {:?}", sim.dev.max_gouge_at);
    assert!(!sim.dev.is_clean() && sim.dev.max_rest > 0.15, "the rough leaves stock: max rest {}", sim.dev.max_rest);

    add(&world, &mut h, "Finish", FINISH, burn(SinkerPass::Finish, "F1"));
    generate(&world, &mut h, "Finish");
    assert!((feed_tip(&world, "Finish") - 14.02).abs() < 1e-9);
    assert!(path_of(&world, "Finish").warnings.is_empty(), "a finish smaller in undersize than its rough is not warned");
    let sim = simulate_document(&world).expect("judged");
    assert!(
        sim.dev.is_clean(),
        "the pair reaches the finished size: max rest {} at {:?}, max gouge {} at {:?}",
        sim.dev.max_rest,
        sim.dev.max_rest_at,
        sim.dev.max_gouge,
        sim.dev.max_gouge_at
    );

    // The pair's CL is golden, one stream per file (#1374): a single file
    // holding two CL headers is not one CL text, so the CL reader's
    // round trip over every golden (camcraft-cl headless_bin) could not read it back.
    common::write_golden("sinker_rough_finish_pair_rough.cl", &common::gen_to_cltext(&path_of(&world, "Rough").path));
    common::write_golden("sinker_rough_finish_pair_finish.cl", &common::gen_to_cltext(&path_of(&world, "Finish").path));
}

#[test]
fn a_finish_that_leaves_the_cavity_under_size_is_refused_by_name() {
    let (world, mut h) = sinker_doc();
    // FB undersizes 0.05 but gaps only 0.02: its electrode (19.9 x 11.9)
    // burns 19.94 - 0.03 per side short of the finished wall.
    h.execute(&world, AddTool::new(&world, "E-FB", electrode_for(0.05)).expect("tool"));
    add(&world, &mut h, "Finish", "E-FB", burn(SinkerPass::Finish, "FB"));
    let Err(err) = GenerateOperation::new(&world, "Finish") else { panic!("an under-size finish generated") };
    assert!(err.to_string().contains("leaves the cavity under size"), "{err}");

    // The wrong electrode for its condition: F1 undersizes 0.02, so its
    // electrode is 19.96 - this one is 19.9.
    h.execute(&world, EditOperationParams::new(&world, "Finish", burn(SinkerPass::Rough, "F1")).expect("edit"));
    let Err(err) = GenerateOperation::new(&world, "Finish") else { panic!("a mis-sized electrode generated") };
    assert!(err.to_string().contains("undersizes its electrode"), "{err}");

    // A rough whose gap burns past the finished wall: the cavity is
    // stated smaller than the electrode plus two gaps.
    let mut p = burn(SinkerPass::Rough, "R1");
    if let OpParams::SinkerBurn(op) = &mut p {
        op.cavity_x = 19.5;
    }
    add(&world, &mut h, "Rough", ROUGH, p);
    let Err(err) = GenerateOperation::new(&world, "Rough") else { panic!("an overburn generated") };
    assert!(err.to_string().contains("past size"), "{err}");

    // No undersize in the condition: by name at generate.
    h.execute(&world, EditOperationParams::new(&world, "Rough", burn(SinkerPass::Rough, "NU")).expect("edit"));
    let Err(err) = GenerateOperation::new(&world, "Rough") else { panic!("no undersize generated") };
    assert!(err.to_string().contains("states no undersize"), "{err}");
}

/// Missing cavity sizes still refuse at command creation; #1355 lets
/// an orbited pass reach the undersize law at generation.
#[test]
fn a_pass_requires_its_cavity_but_accepts_an_orbit_at_command_creation() {
    let (world, mut h) = sinker_doc();
    let no_cavity = |pass, x: f64, y: f64| {
        let mut p = burn(pass, "R1");
        if let OpParams::SinkerBurn(op) = &mut p {
            op.cavity_x = x;
            op.cavity_y = y;
        }
        p
    };
    for (pass, x, y) in [(SinkerPass::Rough, 20.0, 0.0), (SinkerPass::Finish, 0.0, 12.0), (SinkerPass::Rough, f64::NAN, 12.0)] {
        let Err(err) = CreateOperation::new(&world, "Rough", ROUGH, SrcRef::Stock, no_cavity(pass, x, y)) else {
            panic!("a {pass:?} with cavity {x} x {y} was created")
        };
        assert!(err.to_string().contains("state its X and Y size (both above 0)"), "{err}");
    }
    // A single burn never reads the cavity: 0 x 0 is its draft.
    add(&world, &mut h, "Single", ROUGH, no_cavity(SinkerPass::Single, 0.0, 0.0));
    add(&world, &mut h, "Rough", ROUGH, burn(SinkerPass::Rough, "R1"));
    let Err(err) = EditOperationParams::new(&world, "Rough", no_cavity(SinkerPass::Rough, 20.0, 0.0)) else {
        panic!("an edit dropped the cavity")
    };
    assert!(err.to_string().contains("state its X and Y size"), "{err}");

    let mut orbited = burn(SinkerPass::Rough, "R1");
    if let OpParams::SinkerBurn(op) = &mut orbited {
        op.orbit = Some(SinkerOrbit { kind: SinkerOrbitKind::Circular, radius: 0.05, vectors: 4, turns: 1 });
    }
    h.execute(&world, EditOperationParams::new(&world, "Rough", orbited).expect("an orbited rough is accepted"));
}

#[test]
fn a_finish_no_smaller_in_undersize_than_its_rough_is_warned_by_name() {
    let (world, mut h) = sinker_doc();
    add(&world, &mut h, "Rough", ROUGH, burn(SinkerPass::Rough, "R1"));
    generate(&world, &mut h, "Rough");
    // FI undersizes 0.3 like the rough (with a 0.3 gap it still burns the
    // cavity to size), so its electrode is the rough's own size.
    add(&world, &mut h, "Finish", ROUGH, burn(SinkerPass::Finish, "FI"));
    generate(&world, &mut h, "Finish");
    let warns = path_of(&world, "Finish").warnings;
    let [w @ Warning::SinkerFinishOrder { finish_mm, rough_mm, .. }] = warns.as_slice() else {
        panic!("one order warning, got {warns:?}");
    };
    assert_eq!((*finish_mm, *rough_mm), (0.3, 0.3));
    assert!(!w.blocks_post(), "advisory: the finish's own size law holds the cavity");
    assert!(w.to_string().contains("not smaller than the rough's"), "{w}");

    // The rough electrode under the true finish condition burns 19.44:
    // the cavity is left under size - refused by name, not warned.
    h.execute(&world, EditOperationParams::new(&world, "Finish", burn(SinkerPass::Finish, "F1")).expect("edit"));
    let Err(err) = GenerateOperation::new(&world, "Finish") else { panic!("the rough electrode under F1 generated") };
    assert!(err.to_string().contains("leaves the cavity under size"), "{err}");
}

#[test]
fn a_rough_finish_pair_round_trips_the_file() {
    let (world, mut h) = sinker_doc();
    add(&world, &mut h, "Rough", ROUGH, burn(SinkerPass::Rough, "R1"));
    add(&world, &mut h, "Finish", FINISH, burn(SinkerPass::Finish, "F1"));
    let path = std::env::temp_dir().join(format!("camcraft-543-{}.ccam", std::process::id()));
    project::save(&world, &path).expect("save");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    for (name, p) in [("Rough", burn(SinkerPass::Rough, "R1")), ("Finish", burn(SinkerPass::Finish, "F1"))] {
        let e = model::find_by_name(&back, name).expect("op survives");
        assert_eq!(model::operation_of(&back, &e).expect("op").params, p);
    }
}

#[test]
fn the_rough_finish_pair_is_driven_over_mcp_with_the_gui_commands() {
    use camcraft_app::mcp::Server;
    use serde_json::{json, Value};
    fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
        *id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = s.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
        assert_ne!(result["isError"], json!(true), "{name} refused: {}", result["content"][0]["text"]);
        result["structuredContent"].clone()
    }
    let mch = std::env::temp_dir().join(format!("camcraft-543-{}.mch", std::process::id()));
    std::fs::write(&mch, sinker_machine()).expect("write the machine");
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 60, "dy": 60, "dz": 20 } }));
    call(&mut s, &mut id, "load_machine", json!({ "path": mch.display().to_string() }));
    let _ = std::fs::remove_file(&mch);
    for (name, u) in [(ROUGH, 0.3), (FINISH, 0.02)] {
        let def = project::tool_to_json(&electrode_for(u));
        call(&mut s, &mut id, "add_tool", json!({ "name": name, "definition": def, "holder": "none" }));
    }
    let heights = |cond: &str, pass: &str| {
        json!({ "depth": 6, "condition": cond, "pass": pass, "cavity_x": CAV.0, "cavity_y": CAV.1,
            "top_z": { "reference": "fixed", "offset": 20 }, "floor_z": { "reference": "fixed", "offset": 14 } })
    };
    call(&mut s, &mut id, "add_operation", json!({ "kind": "Sinker Burn", "tool": ROUGH, "source": "stock", "name": "Rough", "params": heights("R1", "rough") }));
    call(&mut s, &mut id, "add_operation", json!({ "kind": "Sinker Burn", "tool": FINISH, "source": "stock", "name": "Finish", "params": heights("F1", "finish") }));
    call(&mut s, &mut id, "generate", json!({ "name": "Rough" }));
    call(&mut s, &mut id, "generate", json!({ "name": "Finish" }));
    // Under the under-size condition: the edit lands (one undo step), the
    // generation refuses by name.
    call(&mut s, &mut id, "edit_operation", json!({ "name": "Finish", "params": { "condition": "FB" } }));
    id += 1;
    let reply = s
        .handle(&json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": "generate", "arguments": { "name": "Finish" } } }))
        .expect("a response");
    let text = reply["result"]["content"][0]["text"].as_str().unwrap_or_default().to_owned();
    assert_eq!(reply["result"]["isError"], json!(true), "{reply}");
    assert!(text.contains("undersizes its electrode") || text.contains("under size"), "{text}");
    call(&mut s, &mut id, "undo", json!({}));
    call(&mut s, &mut id, "generate", json!({ "name": "Finish" }));
}

/// The refresh over main's `stamp_tool` / `current_stamp` split: a
/// rough or finish's stamp digests its condition's undersize and, for a
/// finish, the rough it follows. A re-stated undersize marks the pass
/// Stale (the post refuses until regenerated), a re-stated rough
/// undersize marks the finish Stale too; undo reads Ok again.
#[test]
fn a_restated_undersize_marks_the_pass_and_the_finish_after_it_stale() {
    use camcraft_app::operation::{status_of, OpStatus};
    let (world, mut h) = sinker_doc();
    add(&world, &mut h, "Rough", ROUGH, burn(SinkerPass::Rough, "R1"));
    add(&world, &mut h, "Finish", FINISH, burn(SinkerPass::Finish, "F1"));
    generate(&world, &mut h, "Rough");
    generate(&world, &mut h, "Finish");
    let status = |w: &World, n: &str| status_of(w, &model::find_by_name(w, n).expect("op"));
    assert_eq!((status(&world, "Rough"), status(&world, "Finish")), (OpStatus::Ok, OpStatus::Ok));

    // The finish's own condition, its undersize re-stated (gap kept).
    let restate = |i: usize, u: f64| {
        let mut v: serde_json::Value = serde_json::from_str(&sinker_machine()).expect("json");
        v["edm"]["conditions"][i]["undersize"] = serde_json::json!(u);
        parse_mch(&v.to_string()).expect("sinker parses")
    };
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(restate(1, 0.03))).expect("remount"));
    assert_eq!(status(&world, "Finish"), OpStatus::Stale, "a re-stated finish undersize is a different judgement");
    assert_eq!(status(&world, "Rough"), OpStatus::Ok, "the rough's condition did not change");
    assert!(h.undo(&world));
    assert_eq!(status(&world, "Finish"), OpStatus::Ok, "undo restores the undersize the finish was judged with");

    // The rough's undersize re-stated: the rough, and the finish that
    // follows it (its order warning reads the rough's undersize).
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(restate(0, 0.25))).expect("remount"));
    assert_eq!(status(&world, "Rough"), OpStatus::Stale, "a re-stated rough undersize is a different burn");
    assert_eq!(status(&world, "Finish"), OpStatus::Stale, "the finish follows a different rough");
    assert!(h.undo(&world));
    assert_eq!((status(&world, "Rough"), status(&world, "Finish")), (OpStatus::Ok, OpStatus::Ok));
}
