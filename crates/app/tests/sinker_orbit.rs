//! #544 (part of #290): sinker EDM orbit patterns - circular, vector and
//! planetary - expanded into explicit CL moves at the burn floor, so the
//! 3-axis stock sim verifies every move the electrode makes (#290 D3).
//! A circular orbit widens the cavity by exactly its radius; no orbit
//! move leaves its radius; the pattern must be one the machine lists
//! under `edm.orbits` and the radius within its `edm.max_orbit_radius`
//! (#290 D6), each refused by name otherwise. The CL is golden. With
//! electrode wear compensation (#546) the orbit runs at each layer's
//! worn tip and never below the deepest compensated tip.
//!
//! Gate map (#544): covers motion::sinker_orbit, motion::sinker (the
//! orbit leg), app::operation (sinker_op orbit law, params digest,
//! sane), app::project (sinker DTO orbit), app::mcp (params door),
//! sim::stock_sim (dexel_box over the orbit's chords).
//! Provenance: our own SYNTHETIC fixtures - the sinker template of #530
//! with an invented condition, invented orbit words and an invented
//! maximum radius (no machine's manual), and a hand-built block with a
//! rectangular cavity. Not exercised on a real EDM control.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::{Height, OpParams, SinkerBurnOp, SinkerDatum, SinkerOrbit, SinkerOrbitKind, SinkerPass, SrcRef};
use camcraft_app::sinker::{burn_to_depth, SinkerParams};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{simulate_box, simulate_document, Dexel};
use camcraft_app::world::new_world;
use camcraft_app::{model, project};

const SINKER: &str = include_str!("edm/sinker-edm.mch");
const ELECTRODE: &str = "E-10x6";
/// Synthetic spark gap per side and maximum orbit radius, mm.
const GAP: f64 = 0.05;
const MAX_R: f64 = 0.5;

/// The sinker template with one synthetic condition, the given orbit
/// patterns and (when stated) a maximum orbit radius.
fn machine(kinds: &[&str], max: Option<f64>) -> String {
    let mut v: serde_json::Value = serde_json::from_str(SINKER).expect("template json");
    v["edm"]["conditions"] = serde_json::json!([{ "id": "S1", "gap": GAP, "role": "rough" }]);
    let words: Vec<_> = kinds.iter().map(|k| serde_json::json!({ "kind": k, "word": format!("ORB-{k} R{{radius}}") })).collect();
    v["edm"]["orbits"] = serde_json::Value::Array(words);
    if let Some(m) = max {
        v["edm"]["max_orbit_radius"] = serde_json::json!(m);
    }
    v.to_string()
}

const ALL: [&str; 3] = ["circular", "vector", "planetary"];

fn quad(out: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3) {
    out.push(Triangle::new(a, b, c));
    out.push(Triangle::new(a, c, d));
}

/// A 60 x 60 x 20 block with a `2hx` x `2hy` blind cavity 6 deep.
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

fn orbit(kind: SinkerOrbitKind, radius: f64) -> SinkerOrbit {
    SinkerOrbit { kind, radius, vectors: 8, turns: 2 }
}

/// A 6 mm burn from the work top to the cavity floor at Z 14.
fn burn(orbit: Option<SinkerOrbit>) -> OpParams {
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
        orbit,
        layer: None,
        wear_lost: None,
        wear_over: None,
    })
}

/// A sinker document: the part (cavity half sizes `hx`, `hy`), a block
/// of stock, the sinker machine `mch`, the 10 x 6 electrode.
fn doc(hx: f64, hy: f64, mch: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block_with_cavity(hx, hy)));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let m = parse_mch(mch).expect("sinker parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("mount"));
    let e = camcraft_tools::electrode::electrode_tool(10.0, 6.0, 30.0, 15.0);
    h.execute(&world, AddTool::new(&world, ELECTRODE, e).expect("an electrode is a tool"));
    (world, h)
}

/// The cavity the electrode plus the gap plus the orbit radius leaves.
fn orbit_doc() -> (World, History) {
    doc(5.0 + GAP + MAX_R, 3.0 + GAP + MAX_R, &machine(&ALL, Some(MAX_R)))
}

fn moves(world: &World, name: &str) -> Vec<camcraft_app::cl::Move> {
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("generated").path.moves
}

fn generated(world: &World, h: &mut History, p: OpParams) -> Vec<camcraft_app::cl::Move> {
    h.execute(world, CreateOperation::new(world, "Burn1", ELECTRODE, SrcRef::Stock, p).expect("create"));
    h.execute(world, GenerateOperation::new(world, "Burn1").expect("generates"));
    moves(world, "Burn1")
}

#[test]
fn sinker_orbit_circular_matches_golden() {
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
        orbit: Some(orbit(SinkerOrbitKind::Circular, 0.2)),
    };
    let path = burn_to_depth(&p).expect("burns");
    let text = common::gen_to_cltext(&path);
    assert_eq!(camcraft_app::cl::text::to_text(&path).expect("text"), text);
    common::write_golden("sinker_orbit_circular.cl", &text);
}

#[test]
fn a_circular_orbit_widens_the_simulated_cavity_by_its_radius() {
    let (world, mut h) = orbit_doc();
    let path = generated(&world, &mut h, burn(Some(orbit(SinkerOrbitKind::Circular, MAX_R))));
    let feeds: Vec<_> = path.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(feeds.iter().skip(1).all(|m| (m.to.z - (14.0 + GAP)).abs() < 1e-12), "the orbit rides the burn floor");
    let reach = feeds.iter().map(|m| m.to.x.hypot(m.to.y)).fold(0.0, f64::max);
    assert_eq!(reach, MAX_R, "the orbit reaches its radius exactly, never past it");

    // The column engine, read directly at a 0.05 cell: with the orbit
    // the floor runs out to the electrode + gap + radius on each axis -
    // a column centre inside that edge is burned, the next one is not.
    let sim = |path: &[camcraft_app::cl::Move]| {
        let mut dex = Dexel::block(-30.0, -30.0, 30.0, 30.0, 20.0, 0.05).expect("grid");
        simulate_box(&mut dex, path, 5.0, 3.0, GAP).expect("burns");
        move |x: f64, y: f64| {
            let k = dex.col(camcraft_app::geo2d::Point2::new(x, y)).expect("in the grid");
            dex.top_at(k)
        }
    };
    let top = sim(&path);
    let edge_x = 5.0 + GAP + MAX_R; // 5.55: column centres at 5.525 and 5.575
    assert_eq!(top(edge_x - 0.025, 0.0), 14.0, "burned out to the radius in X");
    assert_eq!(top(edge_x + 0.025, 0.0), 20.0, "and not a column past it");
    assert_eq!(top(-(edge_x - 0.025), 0.0), 14.0);
    let edge_y = 3.0 + GAP + MAX_R;
    assert_eq!(top(0.0, edge_y - 0.025), 14.0, "burned out to the radius in Y");
    assert_eq!(top(0.0, edge_y + 0.025), 20.0);
    assert_eq!(top(0.0, -(edge_y - 0.025)), 14.0);
    // The same burn without the orbit stops one radius short.
    h.execute(&world, EditOperationParams::new(&world, "Burn1", burn(None)).expect("edit"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("plunges"));
    let plain = sim(&moves(&world, "Burn1"));
    assert_eq!(plain(edge_x - 0.025, 0.0), 20.0, "a plain plunge leaves the radius standing");
    assert_eq!(plain(5.0 + GAP - 0.025, 0.0), 14.0);
}

#[test]
fn every_orbit_pattern_stays_in_its_radius_and_verifies_clean() {
    for kind in [SinkerOrbitKind::Circular, SinkerOrbitKind::Vector, SinkerOrbitKind::Planetary] {
        let (world, mut h) = orbit_doc();
        let path = generated(&world, &mut h, burn(Some(orbit(kind, MAX_R))));
        assert!(path.iter().all(|m| m.to.x.hypot(m.to.y) <= MAX_R), "{kind:?}: a move past the radius");
        assert!(path.iter().all(|m| m.arc.is_none()), "{kind:?}: chords only, so the box atom sweeps them");
        let sim = simulate_document(&world).expect("judged");
        assert_eq!(sim.dev.gouge_cells, 0, "{kind:?} gouges its own cavity at {:?}", sim.dev.max_gouge_at);
    }
}

#[test]
fn an_orbit_wider_than_the_parts_cavity_is_convicted_by_the_stock_sim() {
    // The cavity fits the plain plunge only: the orbit's walls overburn.
    let (world, mut h) = doc(5.0 + GAP, 3.0 + GAP, &machine(&ALL, Some(MAX_R)));
    generated(&world, &mut h, burn(Some(orbit(SinkerOrbitKind::Circular, MAX_R))));
    let sim = simulate_document(&world).expect("judged");
    assert!(sim.dev.gouge_cells > 0, "a 0.5 mm orbit past the cavity walls is a gouge");
}

#[test]
fn an_unlisted_pattern_and_an_over_radius_orbit_are_refused_by_name() {
    let (world, mut h) = doc(5.0 + GAP + MAX_R, 3.0 + GAP + MAX_R, &machine(&["circular"], Some(MAX_R)));
    let vector = burn(Some(orbit(SinkerOrbitKind::Vector, 0.2)));
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, vector).expect("create"));
    let Err(err) = GenerateOperation::new(&world, "Burn1") else { panic!("an unlisted orbit generated") };
    assert!(err.to_string().contains("lists no vector orbit under edm.orbits"), "{err}");

    let wide = burn(Some(orbit(SinkerOrbitKind::Circular, MAX_R + 0.1)));
    h.execute(&world, EditOperationParams::new(&world, "Burn1", wide).expect("edit"));
    let Err(err) = GenerateOperation::new(&world, "Burn1") else { panic!("an over-radius orbit generated") };
    assert!(err.to_string().contains("orbit radius 0.600 exceeds machine") && err.to_string().contains("maximum orbit radius 0.500"), "{err}");

    // At the maximum exactly: admitted.
    h.execute(&world, EditOperationParams::new(&world, "Burn1", burn(Some(orbit(SinkerOrbitKind::Circular, MAX_R)))).expect("edit"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("at the maximum"));

    // A machine that states no maximum: absent is not unlimited.
    let (world, mut h) = doc(5.6, 3.6, &machine(&ALL, None));
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(Some(orbit(SinkerOrbitKind::Circular, 0.2)))).expect("create"));
    let Err(err) = GenerateOperation::new(&world, "Burn1") else { panic!("an unchecked orbit generated") };
    assert!(err.to_string().contains("states no maximum orbit radius"), "{err}");
    // No orbit at all needs neither.
    h.execute(&world, EditOperationParams::new(&world, "Burn1", burn(None)).expect("edit"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("a plain plunge"));

    // A garbled orbit is refused at the door, before any world law.
    assert!(EditOperationParams::new(&world, "Burn1", burn(Some(orbit(SinkerOrbitKind::Circular, 0.0)))).is_err(), "radius 0");
    let none = SinkerOrbit { vectors: 0, ..orbit(SinkerOrbitKind::Vector, 0.2) };
    assert!(EditOperationParams::new(&world, "Burn1", burn(Some(none))).is_err(), "no vectors");
}

#[test]
fn the_orbit_round_trips_the_file_and_rides_the_mcp_params_door() {
    let (world, mut h) = orbit_doc();
    let p = burn(Some(orbit(SinkerOrbitKind::Planetary, 0.3)));
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, p.clone()).expect("create"));
    let path = std::env::temp_dir().join(format!("camcraft-544-{}.ccam", std::process::id()));
    project::save(&world, &path).expect("save");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    let e = model::find_by_name(&back, "Burn1").expect("op survives");
    assert_eq!(model::operation_of(&back, &e).expect("op").params, p);

    use camcraft_app::mcp::Server;
    use serde_json::{json, Value};
    fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
        *id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        s.handle(&msg).expect("a request gets a response")["result"].clone()
    }
    let ok = |r: &Value, what: &str| assert_ne!(r["isError"], json!(true), "{what} refused: {}", r["content"][0]["text"]);
    let mch = std::env::temp_dir().join(format!("camcraft-544-{}.mch", std::process::id()));
    std::fs::write(&mch, machine(&["circular"], Some(MAX_R))).expect("write the machine");
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    ok(&call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 60, "dy": 60, "dz": 20 } })), "set_stock");
    ok(&call(&mut s, &mut id, "load_machine", json!({ "path": mch.display().to_string() })), "load_machine");
    let _ = std::fs::remove_file(&mch);
    let def = project::tool_to_json(&camcraft_tools::electrode::electrode_tool(10.0, 6.0, 30.0, 15.0));
    ok(&call(&mut s, &mut id, "add_tool", json!({ "name": ELECTRODE, "definition": def, "holder": "none" })), "add_tool");
    let params = json!({ "depth": 6, "condition": "S1", "top_z": { "reference": "fixed", "offset": 20 }, "floor_z": { "reference": "fixed", "offset": 14 },
        "orbit": { "kind": "circular", "radius": 0.4 } });
    ok(&call(&mut s, &mut id, "add_operation", json!({ "kind": "Sinker Burn", "tool": ELECTRODE, "source": "stock", "name": "Burn1", "params": params })), "add_operation");
    ok(&call(&mut s, &mut id, "generate", json!({ "name": "Burn1" })), "generate");
    // Past the maximum: the edit lands (one undo step), generation refuses by name.
    ok(&call(&mut s, &mut id, "edit_operation", json!({ "name": "Burn1", "params": { "orbit": { "radius": 0.7 } } })), "edit");
    let r = call(&mut s, &mut id, "generate", json!({ "name": "Burn1" }));
    assert_eq!(r["isError"], json!(true), "{r}");
    assert!(r["content"][0]["text"].as_str().unwrap_or_default().contains("maximum orbit radius"), "{r}");
    ok(&call(&mut s, &mut id, "undo", json!({})), "undo");
    ok(&call(&mut s, &mut id, "generate", json!({ "name": "Burn1" })), "generate after undo");
    // An unlisted pattern over the door, then the orbit dropped.
    ok(&call(&mut s, &mut id, "edit_operation", json!({ "name": "Burn1", "params": { "orbit": { "kind": "planetary" } } })), "edit");
    let r = call(&mut s, &mut id, "generate", json!({ "name": "Burn1" }));
    assert!(r["content"][0]["text"].as_str().unwrap_or_default().contains("lists no planetary orbit"), "{r}");
    ok(&call(&mut s, &mut id, "edit_operation", json!({ "name": "Burn1", "params": { "orbit": null } })), "drop the orbit");
    ok(&call(&mut s, &mut id, "generate", json!({ "name": "Burn1" })), "a plain plunge");
}

/// #546 meets #544: a worn burn with an orbit expands each layer's orbit
/// at that layer's compensated Z; no move goes below the deepest
/// compensated tip, and the stock sim (judging the worn electrode)
/// finds the cavity clean. With no rate the moves are main's orbit burn.
#[test]
fn a_worn_burn_orbits_at_each_compensated_layer_tip() {
    let (world, mut h) = orbit_doc();
    let mut p = burn(Some(orbit(SinkerOrbitKind::Circular, 0.2)));
    if let OpParams::SinkerBurn(b) = &mut p {
        // 0.12 lost over 6 burned: a rate of 0.02; layers 2 mm deep.
        (b.wear_lost, b.wear_over, b.layer) = (Some(0.12), Some(6.0), Some(2.0));
    }
    let path = generated(&world, &mut h, p);
    let feeds: Vec<_> = path.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    let layer_z = [18.0 + GAP - 0.04, 16.0 + GAP - 0.08, 14.0 + GAP - 0.12];
    let deepest = layer_z[2];
    assert!(path.iter().all(|m| m.to.z >= deepest - 1e-9), "no move below the deepest compensated tip Z {deepest}");
    let per = feeds.len() / 3;
    assert_eq!(feeds.len(), 3 * per, "the same orbit at each of the three layers");
    for (i, want) in layer_z.iter().enumerate() {
        let layer = &feeds[i * per..(i + 1) * per];
        assert!(layer.iter().all(|m| (m.to.z - want).abs() < 1e-9), "layer {i} rides Z {want}");
        let reach = layer.iter().map(|m| m.to.x.hypot(m.to.y)).fold(0.0, f64::max);
        assert!((reach - 0.2).abs() < 1e-12, "layer {i} orbits its radius: {reach}");
    }
    let sim = simulate_document(&world).expect("judged");
    assert_eq!(sim.dev.gouge_cells, 0, "the worn orbit gouges at {:?}", sim.dev.max_gouge_at);

    // No rate: the generated orbit is exactly main's orbit burn.
    let (world, mut h) = orbit_doc();
    let o = orbit(SinkerOrbitKind::Circular, 0.2);
    let got = generated(&world, &mut h, burn(Some(o)));
    let main = SinkerParams {
        x: 0.0,
        y: 0.0,
        approach_z: 25.0,
        top_z: 20.0,
        floor_z: 14.0,
        model_floor_z: Some(0.0),
        datum: SinkerDatum::WorkTop,
        depth: 6.0,
        electrode_len: 15.0,
        feed: 40.0,
        gap: GAP,
        stand: None,
        orbit: Some(o),
    };
    assert_eq!(got, burn_to_depth(&main).expect("burns").moves);
}
