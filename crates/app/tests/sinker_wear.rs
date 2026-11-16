//! #546 (part of #290): sinker electrode wear compensation in Z. A
//! stated axial wear rate - a condition's `wear` in the `.mch` (#531's
//! table), or a wear measured on the op between burns - deepens each
//! burn layer by the rate times the depth burned by then; with neither
//! stated the burn is not compensated and says so. A rate that is not a
//! finite ratio in [0, 1) is refused by name at the machine file and at
//! the op's commands. The stock sim judges the worn electrode's burn, so
//! a compensated burn to the cavity's own floor gouges nothing. The CL
//! is golden.
//!
//! Gate map (#546): covers motion::sinker_wear, app::operation
//! (sinker_wear_op, sinker_op, types::sinker, sane, params, stamp),
//! app::commands::operation (create/edit named refusal), app::stock_sim
//! (plan_program as-burned moves), app::project (sinker DTO keys),
//! post::machine::edm (condition wear).
//! Provenance: our own SYNTHETIC fixtures - the sinker template of #530
//! with invented conditions and invented wear rates (no machine's
//! manual, no vendor wear table). Not exercised on a real EDM control.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::sinker_wear_op::wear_status;
use camcraft_app::operation::{Height, OpParams, SinkerBurnOp, SinkerDatum, SinkerPass, SrcRef};
use camcraft_app::sinker::SinkerParams;
use camcraft_app::sinker_wear::{burn_compensated, SinkerWear, WearComp, NOT_COMPENSATED};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::world::new_world;
use camcraft_app::{model, project};

const SINKER: &str = include_str!("edm/sinker-edm.mch");
const ELECTRODE: &str = "E-10x6";
/// Synthetic spark gap per side, mm.
const GAP: f64 = 0.05;
/// Synthetic axial wear rate, mm lost per mm burned.
const RATE: f64 = 0.02;

/// The sinker template with one synthetic condition, its wear `wear`.
fn sinker_machine(wear: Option<serde_json::Value>) -> String {
    let mut v: serde_json::Value = serde_json::from_str(SINKER).expect("template json");
    let mut c = serde_json::json!({ "id": "S1", "name": "synthetic rough", "gap": GAP, "role": "rough" });
    if let Some(w) = wear {
        c["wear"] = w;
    }
    v["edm"]["conditions"] = serde_json::json!([c]);
    v.to_string()
}

fn quad(out: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3) {
    out.push(Triangle::new(a, b, c));
    out.push(Triangle::new(a, c, d));
}

/// A 60 x 60 x 20 block (z 0..20) with a `2hx` x `2hy` blind cavity
/// 6 mm deep, centred on the origin.
fn block_with_cavity(hx: f64, hy: f64) -> Mesh {
    let p = Point3::new;
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

/// A 6 mm burn from the work top to the cavity's floor at Z 14, in
/// `layer` steps, with a measured wear when given.
fn burn(layer: Option<f64>, measured: Option<(f64, f64)>) -> OpParams {
    OpParams::SinkerBurn(SinkerBurnOp {
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(20.0),
        floor_z: Height::fixed(14.0),
        datum: SinkerDatum::WorkTop,
        depth: 6.0,
        x: 0.0,
        y: 0.0,
        feed: 40.0,
        condition: "S1".into(),
        pass: SinkerPass::Single,
        cavity_x: 0.0,
        cavity_y: 0.0,
        orbit: None,
        layer,
        wear_lost: measured.map(|m| m.0),
        wear_over: measured.map(|m| m.1),
    })
}

fn sinker_doc(wear: Option<serde_json::Value>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block_with_cavity(5.0 + GAP, 3.0 + GAP)));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let m = parse_mch(&sinker_machine(wear)).expect("sinker parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("mount"));
    let e = camcraft_tools::electrode::electrode_tool(10.0, 6.0, 30.0, 15.0);
    h.execute(&world, AddTool::new(&world, ELECTRODE, e).expect("an electrode is a tool"));
    (world, h)
}

fn path_of(world: &World, name: &str) -> camcraft_app::cl::Toolpath {
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("generated").path
}

fn feeds(path: &camcraft_app::cl::Toolpath) -> Vec<f64> {
    path.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to.z).collect()
}

/// Each layer `d` below the top: tip at `20 - d + gap - rate * d`.
fn assert_layers(got: &[f64], rate: f64, depths: &[f64]) {
    assert_eq!(got.len(), depths.len(), "{got:?}");
    for (z, d) in got.iter().zip(depths) {
        let want = 20.0 - d + GAP - rate * d;
        assert!((z - want).abs() < 1e-9, "layer {d}: {z} vs {want} in {got:?}");
    }
}

#[test]
fn sinker_wear_layers_match_golden() {
    // 6 mm from the top of a 20 mm block in 2 mm layers at a 0.02 rate:
    // tips at 18.05 - 0.04, 16.05 - 0.08, 14.05 - 0.12.
    let p = SinkerParams {
        x: 12.5,
        y: -7.5,
        approach_z: 25.0,
        top_z: 20.0,
        floor_z: 10.0,
        model_floor_z: Some(0.0),
        datum: SinkerDatum::WorkTop,
        depth: 6.0,
        electrode_len: 30.0,
        feed: 40.0,
        gap: GAP,
        stand: None,
        orbit: None,
    };
    let w = SinkerWear { comp: Some(WearComp { rate: RATE, from: "synthetic".into() }), layer: Some(2.0) };
    let path = burn_compensated(&p, &w).expect("burns");
    assert_layers(&feeds(&path), RATE, &[2.0, 4.0, 6.0]);
    let text = common::gen_to_cltext(&path);
    assert_eq!(camcraft_app::cl::text::to_text(&path).expect("text"), text);
    common::write_golden("sinker_wear_layers.cl", &text);
}

#[test]
fn a_condition_wear_rate_deepens_each_layer_and_the_sim_judges_it_clean() {
    let (world, mut h) = sinker_doc(Some(serde_json::json!(RATE)));
    let op = CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(Some(2.5), None));
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("generates"));
    let path = path_of(&world, "Burn1");
    // A 2.5 step over 6 mm: layers at 2.5, 5, and the last at 6.
    assert_layers(&feeds(&path), RATE, &[2.5, 5.0, 6.0]);
    assert!(path.notes.iter().any(|n| n.contains("compensated in Z at 0.0200") && n.contains("condition S1")), "{:?}", path.notes);
    let e = model::find_by_name(&world, "Burn1").expect("op");
    let OpParams::SinkerBurn(p) = model::operation_of(&world, &e).expect("op").params else {
        panic!("not a sinker burn");
    };
    assert!(wear_status(&world, None, &p).contains("condition S1"), "{}", wear_status(&world, None, &p));
    // The worn electrode reaches the cavity's own floor: no gouge, though
    // the unworn tip is programmed 0.12 below it.
    let sim = simulate_document(&world).expect("judged");
    assert_eq!(sim.dev.gouge_cells, 0, "{:?}", sim.dev.max_gouge_at);
}

#[test]
fn a_missing_wear_rate_shows_not_compensated() {
    let (world, mut h) = sinker_doc(None);
    let op = CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(Some(2.0), None));
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("generates"));
    let path = path_of(&world, "Burn1");
    assert_layers(&feeds(&path), 0.0, &[2.0, 4.0, 6.0]);
    assert!(path.notes.iter().any(|n| n == NOT_COMPENSATED), "{:?}", path.notes);
    let OpParams::SinkerBurn(p) = burn(None, None) else { unreachable!() };
    assert_eq!(wear_status(&world, None, &p), NOT_COMPENSATED);
    assert!(NOT_COMPENSATED.contains("not compensated"));
}

#[test]
fn a_bad_wear_rate_is_refused_by_name() {
    // At the machine file: not a ratio in [0, 1).
    for w in [serde_json::json!(1.0), serde_json::json!(-0.1), serde_json::json!(2)] {
        let e = parse_mch(&sinker_machine(Some(w.clone()))).expect_err("a bad wear rate parsed");
        assert!(e.to_string().contains("not an axial wear rate"), "{w}: {e}");
    }
    let e = parse_mch(&sinker_machine(Some(serde_json::json!("fast")))).expect_err("a word parsed");
    assert!(e.to_string().contains("wear must be a number"), "{e}");
    // At the op's commands: a measured wear of 1 and over, half stated,
    // or a layer step that is no depth.
    let (world, mut h) = sinker_doc(None);
    let Err(e) = CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(None, Some((6.0, 5.0)))) else {
        panic!("a measured rate of 1.2 was admitted");
    };
    assert!(e.to_string().contains("not an axial wear rate"), "{e}");
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(None, None)).expect("create"));
    let mut half = burn(None, Some((0.1, 5.0)));
    if let OpParams::SinkerBurn(p) = &mut half {
        p.wear_over = None;
    }
    let Err(e) = EditOperationParams::new(&world, "Burn1", half) else { panic!("half a measurement was admitted") };
    assert!(e.to_string().contains("needs both"), "{e}");
    let Err(e) = EditOperationParams::new(&world, "Burn1", burn(None, Some((f64::NAN, 5.0)))) else { panic!("NaN") };
    assert!(e.to_string().contains("not a length lost"), "{e}");
    let Err(e) = EditOperationParams::new(&world, "Burn1", burn(Some(0.0), None)) else { panic!("a zero layer step") };
    assert!(e.to_string().contains("layer step"), "{e}");
}

#[test]
fn a_measured_wear_wins_over_the_condition_and_round_trips_the_file() {
    // The condition states 0.05; the electrode was measured at 0.1 lost
    // over 5 burned (0.02): the measurement is this electrode's.
    let (world, mut h) = sinker_doc(Some(serde_json::json!(0.05)));
    let params = burn(Some(3.0), Some((0.1, 5.0)));
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, params.clone()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("generates"));
    let path = path_of(&world, "Burn1");
    assert_layers(&feeds(&path), 0.02, &[3.0, 6.0]);
    assert!(path.notes.iter().any(|n| n.contains("measured: 0.1 lost over 5 burned")), "{:?}", path.notes);

    let file = std::env::temp_dir().join(format!("camcraft-546-{}.ccam", std::process::id()));
    project::save(&world, &file).expect("save");
    let back = new_world();
    project::load(&back, &file).expect("load");
    let _ = std::fs::remove_file(&file);
    let e = model::find_by_name(&back, "Burn1").expect("op survives");
    assert_eq!(model::operation_of(&back, &e).expect("op").params, params);

    // An older burn states no layer and no measured wear: it reads as one
    // layer, not compensated by the op, and writes no new keys.
    let mut old = project::params_to_json(&burn(Some(2.0), Some((0.1, 5.0))));
    let fields = old.as_object_mut().and_then(|m| m.values_mut().next()).and_then(|v| v.as_object_mut()).expect("the burn's keys");
    for k in ["layer", "wear_lost", "wear_over"] {
        assert!(fields.remove(k).is_some(), "{k} is written when stated");
    }
    let read = project::params_from_json(&old).expect("an older burn reads");
    assert_eq!(read, burn(None, None));
    let written = project::params_to_json(&read).to_string();
    assert!(!written.contains("layer") && !written.contains("wear_"), "{written}");
}

#[test]
fn the_wear_fields_ride_the_mcp_params_door() {
    use camcraft_app::mcp::Server;
    use serde_json::{json, Value};
    fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
        *id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        s.handle(&msg).expect("a request gets a response")["result"].clone()
    }
    let mch = std::env::temp_dir().join(format!("camcraft-546-{}.mch", std::process::id()));
    std::fs::write(&mch, sinker_machine(None)).expect("write the machine");
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 60, "dy": 60, "dz": 20 } }));
    call(&mut s, &mut id, "load_machine", json!({ "path": mch.display().to_string() }));
    let _ = std::fs::remove_file(&mch);
    let def = project::tool_to_json(&camcraft_tools::electrode::electrode_tool(10.0, 6.0, 30.0, 15.0));
    call(&mut s, &mut id, "add_tool", json!({ "name": ELECTRODE, "definition": def, "holder": "none" }));
    let params = json!({ "depth": 6, "condition": "S1", "top_z": { "reference": "fixed", "offset": 20 }, "floor_z": { "reference": "fixed", "offset": 14 } });
    let r = call(&mut s, &mut id, "add_operation", json!({ "kind": "Sinker Burn", "tool": ELECTRODE, "source": "stock", "name": "Burn1", "params": params }));
    assert_ne!(r["isError"], json!(true), "{r}");
    let r = call(&mut s, &mut id, "edit_operation", json!({ "name": "Burn1", "params": { "layer": 2, "wear_lost": 0.1, "wear_over": 5 } }));
    assert_ne!(r["isError"], json!(true), "{r}");
    let r = call(&mut s, &mut id, "generate", json!({ "name": "Burn1" }));
    assert_ne!(r["isError"], json!(true), "{r}");
    let r = call(&mut s, &mut id, "edit_operation", json!({ "name": "Burn1", "params": { "wear_lost": 6 } }));
    assert_eq!(r["isError"], json!(true), "{r}");
    let text = r["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("not an axial wear rate"), "{text}");
    let r = call(&mut s, &mut id, "undo", json!({}));
    assert_ne!(r["isError"], json!(true), "{r}");
}
