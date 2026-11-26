//! #3333: the Sinker Side Burn's start and exit picks, and the MCP door
//! of both sinker kinds. The side move runs between the burn position
//! and the wall end (the burn position moved by the amount toward the
//! wall). The start pick chooses the end where the move begins; the
//! exit pick chooses the end where it ends. The wall stands where the
//! amount states for every choice, and the stock sim finds no gouge.
//!
//! Provenance: our own SYNTHETIC fixtures (`edm/sinker_fixture.rs`).

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::mcp::Server;
use camcraft_app::operation::{OpParams, SinkerDatum, SinkerSideBurnOp, SinkerSideDirection};
use camcraft_app::project;
use camcraft_app::stock_sim::{simulate_box, simulate_document, Dexel};
use serde_json::{json, Value};

use super::sinker_fixture::{self, electrode, ELECTRODE, GAP};
use super::{doc, generated, moves, notes, picked, refused};

/// The electrode undersize per side and the side amount, mm.
const UNDER: f64 = 0.2;
const AMOUNT: f64 = 0.15;
const TIP: f64 = 14.0 + GAP;

/// A 6 mm side burn at the origin toward `direction`.
fn side(direction: SinkerSideDirection) -> OpParams {
    let OpParams::SinkerBurn(burn) = sinker_fixture::burn(SinkerDatum::WorkTop, 6.0, 14.0) else {
        panic!("the fixture is a Sinker Burn");
    };
    OpParams::SinkerSideBurn(SinkerSideBurnOp { burn, direction, amount: AMOUNT, undersize: UNDER })
}

/// The XY of every move's end, in order, with consecutive repeats removed.
fn stations(path: &[Move]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    for m in path {
        if out.last() != Some(&(m.to.x, m.to.y)) {
            out.push((m.to.x, m.to.y));
        }
    }
    out
}

/// The top of the stock column at a point, after the path burned it.
fn sim(path: &[Move]) -> impl Fn(f64, f64) -> f64 {
    let mut dex = Dexel::block(-30.0, -30.0, 30.0, 30.0, 20.0, 0.05).expect("grid");
    simulate_box(&mut dex, path, 5.0, 3.0, GAP).expect("burns");
    move |x: f64, y: f64| {
        let k = dex.col(camcraft_app::geo2d::Point2::new(x, y)).expect("in the grid");
        dex.top_at(k)
    }
}

#[test]
fn a_side_burn_starts_and_ends_at_the_picked_ends_of_its_side_move() {
    let (world, mut h) = doc(5.0 + UNDER, 3.0 + UNDER);
    let (b, w) = ((0.0, 0.0), (AMOUNT, 0.0));
    let auto = generated(&world, &mut h, side(SinkerSideDirection::PlusX));
    assert_eq!(stations(&auto), [b, w, b], "no pick: plunge at the burn position, out, back");
    // A pick on the wall side of the middle of the move chooses the wall end.
    let (at_b, at_w) = (Some([-1.0, 0.5]), Some([4.0, -0.5]));
    for (start, exit, want) in [
        (at_b, at_b, vec![b, w, b]),
        (None, at_w, vec![b, w]),
        (at_w, None, vec![w, b]),
        (at_w, at_b, vec![w, b]),
        (at_w, at_w, vec![w, b, w]),
    ] {
        let path = generated(&world, &mut h, picked(side(SinkerSideDirection::PlusX), start, exit));
        assert_eq!(stations(&path), want, "start {start:?}, exit {exit:?}");
        let (first, last) = (path.first().expect("an approach"), path.last().expect("a retract"));
        assert_eq!((first.kind, first.to.z), (MoveKind::Rapid, 25.0), "the approach is a rapid on the approach plane");
        assert_eq!((last.kind, last.to.z), (MoveKind::Rapid, 25.0), "the retract goes straight up to the approach plane");
        assert_eq!((last.from.x, last.from.y), (last.to.x, last.to.y), "the retract moves in Z only");
        let feeds: Vec<_> = path.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        assert!(feeds.iter().all(|m| (m.to.z - TIP).abs() < 1e-12), "every feed ends at the burn floor's tip");
        assert!(path.iter().all(|m| (0.0..=AMOUNT).contains(&m.to.x) && m.to.y == 0.0), "no move leaves the side move");
        if want != [b, w, b] {
            assert!(notes(&world, "Op1").iter().any(|n| n.contains("start point") || n.contains("exit point")), "{:?}", notes(&world, "Op1"));
        }
        // The faced wall moves by the amount; the far wall stays at the plunge's gap.
        let top = sim(&path);
        let wall = 5.0 + GAP + AMOUNT;
        assert_eq!(top(wall - 0.025, 0.0), 14.0, "burned to the floor out to the moved wall");
        assert_eq!(top(wall + 0.025, 0.0), 20.0, "the next column out is untouched");
        assert_eq!(top(-(5.0 + GAP + 0.025), 0.0), 20.0, "the far wall is not burned past the gap");
        assert_eq!(top(0.0, 3.0 + GAP + 0.025), 20.0, "the side walls are not burned");
        assert_eq!(simulate_document(&world).expect("judged").dev.gouge_cells, 0, "start {start:?}, exit {exit:?}");
    }
    // The same law along -Y: the wall end is the burn position moved toward -Y.
    let path = generated(&world, &mut h, picked(side(SinkerSideDirection::MinusY), Some([0.0, -3.0]), Some([0.0, -3.0])));
    assert_eq!(stations(&path), [(0.0, -AMOUNT), (0.0, 0.0), (0.0, -AMOUNT)]);
    // Undo and redo restore the previous path.
    let picked_path = moves(&world, "Op1");
    let before = generated(&world, &mut h, side(SinkerSideDirection::MinusY));
    h.undo(&world);
    h.undo(&world);
    assert_eq!(moves(&world, "Op1"), picked_path, "undo restores the picked path");
    h.redo(&world);
    h.redo(&world);
    assert_eq!(moves(&world, "Op1"), before, "redo restores the path without a pick");
}

#[test]
fn a_side_burn_pick_at_the_middle_of_the_move_is_refused_by_name() {
    let (world, _h) = doc(5.0 + UNDER, 3.0 + UNDER);
    for (start, exit, what) in [(Some([AMOUNT / 2.0, 1.0]), None, "start point"), (None, Some([AMOUNT / 2.0, 0.0]), "exit point")] {
        let why = refused(&world, picked(side(SinkerSideDirection::PlusX), start, exit));
        assert!(why.contains("Sinker Side Burn") && why.contains(what) && why.contains("middle of the side move"), "{why}");
    }
}

/// The generator's own door: a pick that skipped the command door (a
/// project file, a caller of `operation::generate`) is refused by name
/// there too, and no path is made.
#[test]
fn the_generator_refuses_an_impossible_sinker_pick_by_name() {
    use camcraft_app::operation::{self, SinkerOrbit, SinkerOrbitKind, SrcRef};
    use camcraft_app::schema::Operation;
    let (world, _h) = doc(5.0 + UNDER, 3.0 + UNDER);
    let op = |params: OpParams| Operation {
        tool: ELECTRODE.into(),
        src: SrcRef::Stock,
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let refused = |params: OpParams| match operation::generate(&world, &op(params), "") {
        Ok(_) => panic!("the generator made a path for an impossible pick"),
        Err(e) => e.to_string(),
    };
    let why = refused(picked(side(SinkerSideDirection::PlusX), Some([AMOUNT / 2.0, 0.0]), None));
    assert!(why.contains("Sinker Side Burn") && why.contains("middle of the side move"), "{why}");
    let burn = |orbit: Option<SinkerOrbit>| {
        let mut p = sinker_fixture::burn(SinkerDatum::WorkTop, 6.0, 14.0);
        if let OpParams::SinkerBurn(b) = &mut p {
            b.orbit = orbit;
        }
        p
    };
    let why = refused(picked(burn(None), None, Some([1.0, 0.0])));
    assert!(why.contains("Sinker Burn") && why.contains("without an orbit"), "{why}");
    let circle = SinkerOrbit { kind: SinkerOrbitKind::Circular, radius: 0.1, vectors: 4, turns: 1 };
    let why = refused(picked(burn(Some(circle)), Some([0.0, 0.0]), None));
    assert!(why.contains("Sinker Burn") && why.contains("on the burn position"), "{why}");
    let why = refused(picked(burn(Some(SinkerOrbit { kind: SinkerOrbitKind::Vector, ..circle })), Some([1.0, 1.0]), None));
    assert!(why.contains("Sinker Burn") && why.contains("on no vector"), "{why}");
}

fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Result<Value, String> {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    if result["isError"] == json!(true) {
        Err(result["content"][0]["text"].as_str().unwrap_or_default().to_owned())
    } else {
        Ok(result["structuredContent"].clone())
    }
}

#[test]
fn mcp_edit_operation_sets_the_sinker_picks_and_refuses_an_impossible_one_by_name() {
    let mch = std::env::temp_dir().join(format!("camcraft-3333-{}.mch", std::process::id()));
    std::fs::write(&mch, super::machine()).expect("write the machine");
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    let mut ok = |s: &mut Server, name: &str, args: Value| call(s, &mut id, name, args).unwrap_or_else(|e| panic!("{name} refused: {e}"));
    ok(&mut s, "set_stock", json!({ "box": { "dx": 60, "dy": 60, "dz": 20 } }));
    ok(&mut s, "load_machine", json!({ "path": mch.display().to_string() }));
    let _ = std::fs::remove_file(&mch);
    ok(&mut s, "add_tool", json!({ "name": ELECTRODE, "definition": project::tool_to_json(&electrode()), "holder": "none" }));
    let heights = json!({ "depth": 6, "condition": "S1", "top_z": { "reference": "fixed", "offset": 20 }, "floor_z": { "reference": "fixed", "offset": 14 } });
    let mut burn = heights.clone();
    burn["orbit"] = json!({ "kind": "circular", "radius": 0.3 });
    ok(&mut s, "add_operation", json!({ "kind": "Sinker Burn", "tool": ELECTRODE, "source": "stock", "name": "Burn1", "params": burn }));
    let mut side = heights;
    side["direction"] = json!("plus_x");
    side["amount"] = json!(AMOUNT);
    side["undersize"] = json!(UNDER);
    ok(&mut s, "add_operation", json!({ "kind": "Sinker Side Burn", "tool": ELECTRODE, "source": "stock", "name": "Side1", "params": side }));

    ok(&mut s, "edit_operation", json!({ "name": "Burn1", "params": { "picks": { "start": [[0.0, 5.0]], "exit": [[-4.0, 0.0]] } } }));
    ok(&mut s, "edit_operation", json!({ "name": "Side1", "params": { "picks": { "start": [[4.0, 0.0]] } } }));
    ok(&mut s, "generate", json!({ "name": "Burn1" }));
    ok(&mut s, "generate", json!({ "name": "Side1" }));
    let job = ok(&mut s, "get_job", json!({})).to_string();
    assert!(job.contains("\"picks\""), "get_job shows the picks: {job:.600}");
    let kinds = ok(&mut s, "list_operation_types", json!({}));
    for label in ["Sinker Burn", "Sinker Side Burn"] {
        let row = kinds["kinds"].as_array().and_then(|a| a.iter().find(|k| k["label"] == json!(label))).cloned().unwrap_or_default();
        let taken = &row["start_exit"];
        assert_eq!((&taken["start"], &taken["exit"], &taken["field"]), (&json!(true), &json!(true), &json!("picks")), "{label}: {taken}");
    }

    // Impossible picks: refused by name, nothing changes.
    let why = call(&mut s, &mut id, "edit_operation", json!({ "name": "Burn1", "params": { "picks": { "start": [[0.0, 0.0]] } } })).expect_err("on the burn position");
    assert!(why.contains("Sinker Burn") && why.contains("on the burn position"), "{why}");
    let why = call(&mut s, &mut id, "edit_operation", json!({ "name": "Burn1", "params": { "orbit": null } })).expect_err("a pick without an orbit");
    assert!(why.contains("Sinker Burn") && why.contains("without an orbit"), "{why}");
    let why = call(&mut s, &mut id, "edit_operation", json!({ "name": "Side1", "params": { "picks": { "exit": [[AMOUNT / 2.0, 0.0]] } } })).expect_err("the middle");
    assert!(why.contains("Sinker Side Burn") && why.contains("middle of the side move"), "{why}");
    // A refused edit changed nothing: both operations still generate.
    call(&mut s, &mut id, "generate", json!({ "name": "Burn1" })).expect("the burn generates after the refusals");
    call(&mut s, &mut id, "generate", json!({ "name": "Side1" })).expect("the side burn generates after the refusal");
}
