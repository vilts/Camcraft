//! #545 (part of #290): sinker EDM side burning - a separate op kind
//! (Sinker Side Burn) with its own condition and its own electrode
//! undersize: the burn's plunge, then one lateral move along an axis
//! that moves the faced wall out by the stated amount. The stock sim
//! proves the wall stands where stated and the next column out is
//! untouched; a lateral move past the allowance (undersize less the
//! condition's spark gap) is refused by name at creation and at
//! generation; the CL is golden. A measured wear and a layer step (#546)
//! compensate the plunge in Z (#1340); a half-stated one is refused by name.
//!
//! Gate map (#545): covers motion::sinker_side, motion::sinker (check),
//! app::operation (sinker_side_op, kinds row, params digest, sane),
//! app::commands (sinker_gate on create / edit), app::project (side
//! DTO), app::mcp (params door), sim::stock_sim (dexel_box along the
//! lateral move).
//! Provenance: our own SYNTHETIC fixtures - the sinker template of #530
//! with an invented condition (no machine's manual), and a hand-built
//! block with a rectangular cavity. Not exercised on a real EDM control.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, PasteOperation,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::{
    Height, OpParams, SinkerBurnOp, SinkerDatum, SinkerPass, SinkerSideBurnOp, SinkerSideDirection, SrcRef,
};
use camcraft_app::sinker::SinkerParams;
use camcraft_app::sinker_side::{side_burn, SideBurn};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{simulate_box, simulate_document, Dexel};
use camcraft_app::world::new_world;
use camcraft_app::{model, project};

const SINKER: &str = include_str!("edm/sinker-edm.mch");
const ELECTRODE: &str = "E-10x6";
/// Synthetic spark gap per side, undersize per side, side amount, mm.
const GAP: f64 = 0.05;
const UNDER: f64 = 0.2;
const AMOUNT: f64 = 0.15;

/// The sinker template with one synthetic condition of spark gap `gap`.
fn machine(gap: f64) -> String {
    let mut v: serde_json::Value = serde_json::from_str(SINKER).expect("template json");
    v["edm"]["conditions"] = serde_json::json!([{ "id": "S1", "gap": gap, "role": "finish" }]);
    v.to_string()
}

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

/// A 6 mm side burn from the work top to the cavity floor at Z 14.
fn side(direction: SinkerSideDirection, amount: f64, undersize: f64, condition: &str) -> OpParams {
    OpParams::SinkerSideBurn(SinkerSideBurnOp {
        burn: SinkerBurnOp {
            clearance_z: Height::fixed(25.0),
            top_z: Height::fixed(20.0),
            floor_z: Height::fixed(14.0),
            datum: SinkerDatum::WorkTop,
            depth: 6.0,
            x: 0.0,
            y: 0.0,
            feed: 40.0,
            condition: condition.into(),
            pass: SinkerPass::Single,
            cavity_x: 0.0,
            cavity_y: 0.0,
            orbit: None,
            layer: None,
            wear_lost: None,
            wear_over: None,
        },
        direction,
        amount,
        undersize,
    })
}

/// A sinker document: the part (cavity half sizes `hx`, `hy`), a block
/// of stock, the sinker machine with spark gap `gap`, the 10 x 6
/// electrode.
fn doc(hx: f64, hy: f64, gap: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block_with_cavity(hx, hy)));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let m = parse_mch(&machine(gap)).expect("sinker parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("mount"));
    let e = camcraft_tools::electrode::electrode_tool(10.0, 6.0, 30.0, 15.0);
    h.execute(&world, AddTool::new(&world, ELECTRODE, e).expect("an electrode is a tool"));
    (world, h)
}

/// The finished cavity: the electrode plus its undersize per side.
fn finished_doc() -> (World, History) {
    doc(5.0 + UNDER, 3.0 + UNDER, GAP)
}

fn moves(world: &World, name: &str) -> Vec<camcraft_app::cl::Move> {
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("generated").path.moves
}

fn generated(world: &World, h: &mut History, p: OpParams) -> Vec<camcraft_app::cl::Move> {
    h.execute(world, CreateOperation::new(world, "Side1", ELECTRODE, SrcRef::Stock, p).expect("create"));
    h.execute(world, GenerateOperation::new(world, "Side1").expect("generates"));
    moves(world, "Side1")
}

/// The column engine at a 0.05 cell, read at a point: the top of the
/// column whose centre the point is in.
fn sim(path: &[camcraft_app::cl::Move]) -> impl Fn(f64, f64) -> f64 {
    let mut dex = Dexel::block(-30.0, -30.0, 30.0, 30.0, 20.0, 0.05).expect("grid");
    simulate_box(&mut dex, path, 5.0, 3.0, GAP).expect("burns");
    move |x: f64, y: f64| {
        let k = dex.col(camcraft_app::geo2d::Point2::new(x, y)).expect("in the grid");
        dex.top_at(k)
    }
}

#[test]
fn sinker_side_burn_matches_golden() {
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
    let s = SideBurn { direction: SinkerSideDirection::MinusY, amount: AMOUNT, undersize: UNDER };
    let path = side_burn(&p, &s).expect("burns");
    let text = common::gen_to_cltext(&path);
    assert_eq!(camcraft_app::cl::text::to_text(&path).expect("text"), text);
    common::write_golden("sinker_side_burn.cl", &text);
}

#[test]
fn a_side_burn_moves_the_faced_wall_by_its_amount_in_the_sim() {
    for (dir, (ux, uy)) in [
        (SinkerSideDirection::PlusX, (1.0, 0.0)),
        (SinkerSideDirection::MinusX, (-1.0, 0.0)),
        (SinkerSideDirection::PlusY, (0.0, 1.0)),
        (SinkerSideDirection::MinusY, (0.0, -1.0)),
    ] {
        let (world, mut h) = finished_doc();
        let path = generated(&world, &mut h, side(dir, AMOUNT, UNDER, "S1"));
        let feeds: Vec<_> = path.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        assert!(feeds.iter().all(|m| (m.to.z - (14.0 + GAP)).abs() < 1e-12), "{dir:?}: the side move rides the burn floor");
        let reach = feeds.iter().map(|m| m.to.x.hypot(m.to.y)).fold(0.0, f64::max);
        assert_eq!(reach, AMOUNT, "{dir:?}: the electrode travels exactly the amount");

        let top = sim(&path);
        // The half size along the direction, and across it.
        let (along, across) = if ux != 0.0 { (5.0, 3.0) } else { (3.0, 5.0) };
        let at = |d: f64, c: f64| if ux != 0.0 { top(ux * d, c) } else { top(c, uy * d) };
        let wall = along + GAP + AMOUNT; // the faced wall, moved by the amount
        assert_eq!(at(wall - 0.025, 0.0), 14.0, "{dir:?}: burned to the floor out to the moved wall");
        assert_eq!(at(wall + 0.025, 0.0), 20.0, "{dir:?}: the next column out is untouched");
        // The opposite wall stays where the plunge left it.
        let back = along + GAP;
        assert_eq!(at(-(back - 0.025), 0.0), 14.0, "{dir:?}: the far wall is burned to the plunge");
        assert_eq!(at(-(back + 0.025), 0.0), 20.0, "{dir:?}: and not a column past it");
        // The walls across the move stay at the plunge's gap.
        assert_eq!(at(0.0, across + GAP - 0.025), 14.0, "{dir:?}");
        assert_eq!(at(0.0, across + GAP + 0.025), 20.0, "{dir:?}: the side walls are not burned");
        // The program verdict: the moved wall stops at the finished
        // cavity, no gouge.
        let verdict = simulate_document(&world).expect("judged");
        assert_eq!(verdict.dev.gouge_cells, 0, "{dir:?} gouges at {:?}", verdict.dev.max_gouge_at);
    }
}

#[test]
fn a_lateral_move_past_the_allowance_is_refused_by_name() {
    let (world, mut h) = finished_doc();
    let create = |p: OpParams| CreateOperation::new(&world, "Side1", ELECTRODE, SrcRef::Stock, p).map(|_| ());
    // The allowance is undersize 0.2 less gap 0.05: 0.15. A hair past it
    // is a move into the part.
    let err = create(side(SinkerSideDirection::PlusX, 0.16, UNDER, "S1")).expect_err("into the part");
    assert!(err.to_string().contains("lateral move of 0.160 toward +X burns into the part - the side's allowance is 0.150"), "{err}");
    // Past the undersize itself, even with no condition named yet.
    let err = create(side(SinkerSideDirection::MinusY, 0.3, UNDER, "")).expect_err("past the undersize");
    assert!(err.to_string().contains("exceeds the electrode undersize 0.200"), "{err}");
    // A gap the undersize cannot honour.
    let err = create(side(SinkerSideDirection::PlusX, 0.01, 0.04, "S1")).expect_err("gap past the undersize");
    assert!(err.to_string().contains("spark gap 0.050 exceeds the electrode undersize 0.040"), "{err}");
    // An unknown condition.
    let err = create(side(SinkerSideDirection::PlusX, AMOUNT, UNDER, "S9")).expect_err("unknown condition");
    assert!(err.to_string().contains("states no EDM condition \"S9\""), "{err}");
    // A non-finite, zero or negative amount; a garbled undersize.
    for amount in [f64::NAN, f64::INFINITY, -0.1, 0.0] {
        let err = create(side(SinkerSideDirection::PlusX, amount, UNDER, "S1")).expect_err("no amount");
        assert!(err.to_string().contains("moves no wall"), "{amount}: {err}");
    }
    for undersize in [f64::NAN, -0.2] {
        let err = create(side(SinkerSideDirection::PlusX, AMOUNT, undersize, "S1")).expect_err("no undersize");
        assert!(err.to_string().contains("is not a finite, non-negative number"), "{undersize}: {err}");
    }
    // #543's rough / finish is a Sinker Burn's pass: the side burn sizes
    // its electrode by its own undersize, so its burn is a single pass.
    let mut rough = side(SinkerSideDirection::PlusX, AMOUNT, UNDER, "S1");
    if let OpParams::SinkerSideBurn(op) = &mut rough {
        op.burn.pass = SinkerPass::Rough;
        op.burn.cavity_x = 20.0;
        op.burn.cavity_y = 12.0;
    }
    let err = create(rough).expect_err("a side burn as a rough");
    assert!(err.to_string().contains("its burn is a single pass"), "{err}");

    // At the allowance exactly: admitted. Edited past it: refused at the
    // edit's door, the op unchanged.
    let at = side(SinkerSideDirection::PlusX, AMOUNT, UNDER, "S1");
    h.execute(&world, CreateOperation::new(&world, "Side1", ELECTRODE, SrcRef::Stock, at.clone()).expect("at the allowance"));
    let err = EditOperationParams::new(&world, "Side1", side(SinkerSideDirection::PlusX, 0.2, UNDER, "S1")).map(|_| ()).expect_err("edit");
    assert!(err.to_string().contains("burns into the part"), "{err}");
    h.execute(&world, GenerateOperation::new(&world, "Side1").expect("generates"));

    // The machine re-states the condition with a wider gap: the stored
    // op's allowance shrinks under it, and generation refuses by name.
    let m = parse_mch(&machine(0.1)).expect("parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("remount"));
    let Err(err) = GenerateOperation::new(&world, "Side1") else { panic!("a move past the new allowance generated") };
    assert!(err.to_string().contains("the side's allowance is 0.100 (undersize 0.200 less spark gap 0.100)"), "{err}");
    let e = model::find_by_name(&world, "Side1").expect("op");
    assert_eq!(model::operation_of(&world, &e).expect("op").params, at, "the refusal left the op as it was");
}

#[test]
fn an_undersize_overstated_against_the_part_is_convicted_by_the_stock_sim() {
    // The part's cavity is the plain plunge's: the claimed undersize is
    // not there, so the side move burns into the wall.
    let (world, mut h) = doc(5.0 + GAP, 3.0 + GAP, GAP);
    generated(&world, &mut h, side(SinkerSideDirection::PlusX, AMOUNT, UNDER, "S1"));
    let verdict = simulate_document(&world).expect("judged");
    assert!(verdict.dev.gouge_cells > 0, "a 0.15 mm side move past the part's wall is a gouge");
}

#[test]
fn the_side_burn_round_trips_the_file_and_rides_the_mcp_params_door() {
    let (world, mut h) = finished_doc();
    let p = side(SinkerSideDirection::MinusX, 0.1, UNDER, "S1");
    h.execute(&world, CreateOperation::new(&world, "Side1", ELECTRODE, SrcRef::Stock, p.clone()).expect("create"));
    let path = std::env::temp_dir().join(format!("camcraft-545-{}.ccam", std::process::id()));
    project::save(&world, &path).expect("save");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    let e = model::find_by_name(&back, "Side1").expect("op survives");
    assert_eq!(model::operation_of(&back, &e).expect("op").params, p);

    use camcraft_app::mcp::Server;
    use serde_json::{json, Value};
    fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
        *id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        s.handle(&msg).expect("a request gets a response")["result"].clone()
    }
    let text = |r: &Value| r["content"][0]["text"].as_str().unwrap_or_default().to_owned();
    let ok = |r: &Value, what: &str| assert_ne!(r["isError"], json!(true), "{what} refused: {}", r["content"][0]["text"]);
    let mch = std::env::temp_dir().join(format!("camcraft-545-{}.mch", std::process::id()));
    std::fs::write(&mch, machine(GAP)).expect("write the machine");
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    let kinds = call(&mut s, &mut id, "list_operation_types", json!({}));
    let listed = text(&kinds);
    assert!(listed.contains("Sinker Side Burn") && listed.contains("sinker_side_burn"), "the kind is listed: {listed}");
    assert!(listed.contains("undersize") && listed.contains("amount") && listed.contains("plus_x"), "with its fields: {listed}");
    ok(&call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 60, "dy": 60, "dz": 20 } })), "set_stock");
    ok(&call(&mut s, &mut id, "load_machine", json!({ "path": mch.display().to_string() })), "load_machine");
    let _ = std::fs::remove_file(&mch);
    let def = project::tool_to_json(&camcraft_tools::electrode::electrode_tool(10.0, 6.0, 30.0, 15.0));
    ok(&call(&mut s, &mut id, "add_tool", json!({ "name": ELECTRODE, "definition": def, "holder": "none" })), "add_tool");
    let params = json!({ "depth": 6, "condition": "S1", "top_z": { "reference": "fixed", "offset": 20 }, "floor_z": { "reference": "fixed", "offset": 14 },
        "direction": "plus_y", "amount": 0.12, "undersize": 0.2 });
    ok(&call(&mut s, &mut id, "add_operation", json!({ "kind": "Sinker Side Burn", "tool": ELECTRODE, "source": "stock", "name": "Side1", "params": params })), "add_operation");
    ok(&call(&mut s, &mut id, "generate", json!({ "name": "Side1" })), "generate");
    // Past the allowance over the door: refused by name, nothing changes.
    let r = call(&mut s, &mut id, "edit_operation", json!({ "name": "Side1", "params": { "amount": 0.2 } }));
    assert_eq!(r["isError"], json!(true), "{r}");
    assert!(text(&r).contains("burns into the part"), "{r}");
    ok(&call(&mut s, &mut id, "edit_operation", json!({ "name": "Side1", "params": { "direction": "minus_x", "amount": 0.15 } })), "edit");
    ok(&call(&mut s, &mut id, "generate", json!({ "name": "Side1" })), "generate after the edit");
    ok(&call(&mut s, &mut id, "undo", json!({})), "undo");
    ok(&call(&mut s, &mut id, "generate", json!({ "name": "Side1" })), "generate after undo");
}

/// #1340: a measured wear and a layer step compensate the side burn's
/// plunge in Z - admitted at create, edit, paste and the file; the lateral
/// move rides the deepest compensated tip, no move goes below it, and the
/// stock sim (reading the worn electrode) finds no gouge. A half-stated
/// wear is refused by name; a condition's own wear does not apply.
#[test]
fn a_worn_side_burn_is_compensated_in_z_and_judged_clean() {
    let worn = |lost: Option<f64>, over: Option<f64>, layer: Option<f64>| {
        let mut p = side(SinkerSideDirection::PlusX, AMOUNT, UNDER, "S1");
        if let OpParams::SinkerSideBurn(s) = &mut p {
            (s.burn.wear_lost, s.burn.wear_over, s.burn.layer) = (lost, over, layer);
        }
        p
    };
    let (world, mut h) = finished_doc();
    let create = |p: OpParams| CreateOperation::new(&world, "Side9", ELECTRODE, SrcRef::Stock, p).map(|_| ());
    for (lost, over) in [(Some(0.1), None), (None, Some(5.0))] {
        let err = create(worn(lost, over, None)).expect_err("a half-stated wear");
        assert!(err.to_string().contains("needs both the length lost and the depth burned"), "{lost:?}/{over:?}: {err}");
    }
    let err = create(worn(None, None, Some(0.0))).expect_err("a nonsense layer step");
    assert!(err.to_string().contains("layer step 0 must be a positive depth"), "{err}");

    // 0.12 lost over 6 burned: a rate of 0.02, in 2 mm layers.
    let p = worn(Some(0.12), Some(6.0), Some(2.0));
    let path = generated(&world, &mut h, p.clone());
    let feeds: Vec<_> = path.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    let want = [18.05 - 0.04, 16.05 - 0.08, 14.05 - 0.12, 14.05 - 0.12, 14.05 - 0.12];
    assert_eq!(feeds.len(), want.len(), "three layers, out, back");
    for (m, z) in feeds.iter().zip(want) {
        assert!((m.to.z - z).abs() < 1e-9, "{:?} vs {z}", m.to);
    }
    assert_eq!(feeds[3].to.x, AMOUNT, "the lateral move is the stated amount");
    assert!(path.iter().all(|m| m.to.z >= 14.05 - 0.12 - 1e-9), "no move below the deepest compensated tip");
    let verdict = simulate_document(&world).expect("judged");
    assert_eq!(verdict.dev.gouge_cells, 0, "the worn burn gouges at {:?}", verdict.dev.max_gouge_at);

    // The edit, paste and file doors admit it too.
    let layered = worn(None, None, Some(3.0));
    h.execute(&world, EditOperationParams::new(&world, "Side1", layered.clone()).expect("edit admits a layer step"));
    let e = model::find_by_name(&world, "Side1").expect("op");
    let mut copy = model::operation_of(&world, &e).expect("op");
    copy.params = p.clone();
    h.execute(&world, PasteOperation::new(&world, "Side2", copy, Vec::new()).expect("paste admits a measured wear"));
    let file = std::env::temp_dir().join(format!("camcraft-1340-{}.ccam", std::process::id()));
    project::save(&world, &file).expect("save");
    let back = new_world();
    project::load(&back, &file).expect("the reader admits it");
    let _ = std::fs::remove_file(&file);
    for (name, want) in [("Side1", &layered), ("Side2", &p)] {
        let e = model::find_by_name(&back, name).expect("op survives");
        assert_eq!(&model::operation_of(&back, &e).expect("op").params, want, "{name}");
    }

    // A condition stating its own wear: a side burn with no measured
    // wear keeps its plain moves (the condition's wear is not applied).
    let mut v: serde_json::Value = serde_json::from_str(&machine(GAP)).expect("json");
    v["edm"]["conditions"][0]["wear"] = serde_json::json!(0.02);
    let (world, mut h) = finished_doc();
    let m = parse_mch(&v.to_string()).expect("parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("remount"));
    let path = generated(&world, &mut h, side(SinkerSideDirection::PlusX, AMOUNT, UNDER, "S1"));
    assert!(path.iter().filter(|m| m.kind == MoveKind::Feed).all(|m| m.to.z == 14.0 + GAP), "uncompensated");
}
