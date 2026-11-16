//! #541 (part of #290): the Sinker Burn - one rectangular electrode
//! burned straight down Z to a depth read from a stated datum, on a
//! sinker EDM only. The electrode is a tool-library tool (validated at
//! AddTool), the condition's spark gap places the tip (#531), a depth
//! past the cavity floor datum is refused by name, and the 3-axis stock
//! sim verifies the plunge with the electrode's box atom (#290 D7): a
//! burn deeper than the part's cavity is convicted. The CL is golden.
//!
//! Gate map (#541): covers app::operation (sinker_op, kinds, params,
//! stamp), app::commands::operation (family gate), app::stock_sim
//! (plan_program, carry walk), app::project (sinker DTO), motion::sinker,
//! sim::stock_sim (dexel_box), tools::electrode, and (#542) app::posting
//! (SINKER_POSTS, post_serves, the sinker stream PARAMs) with the
//! generic sinker post over the synthetic fixture dialect.
//! Provenance: our own SYNTHETIC fixtures - the sinker template of #530
//! with an invented condition (no machine's manual), and a hand-built
//! block with a rectangular cavity. Not exercised on a real EDM control.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SetStock};
use camcraft_app::operation::{OpParams, SinkerDatum, SrcRef};
use camcraft_app::sinker::{burn_to_depth, SinkerParams};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{simulate_box, simulate_document, Dexel};
use camcraft_app::tools::Tool;
use camcraft_app::world::new_world;
use camcraft_app::{model, project};

#[path = "edm/sinker_fixture.rs"]
mod sinker_fixture;

use sinker_fixture::*;

fn feed_tip(world: &World, name: &str) -> f64 {
    let e = model::find_by_name(world, name).expect("op");
    let path = model::toolpath_of(world, &e).expect("generated").path;
    path.moves.iter().filter(|m| m.kind == camcraft_app::cl::MoveKind::Feed).map(|m| m.to.z).fold(f64::INFINITY, f64::min)
}

#[test]
fn sinker_burn_cavity_matches_golden() {
    // A 6 mm burn from the work top of a 20 mm block: approach at 25,
    // the tip fed to one 0.05 gap above the floor at Z 14, retract.
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
    let path = burn_to_depth(&p).expect("burns");
    let text = common::gen_to_cltext(&path);
    assert_eq!(camcraft_app::cl::text::to_text(&path).expect("text"), text);
    common::write_golden("sinker_burn_cavity.cl", &text);
}

/// The review of #1131: the plunge must go DOWN into the work. A spark
/// gap that lifts the tip to the approach plane would post an upward
/// "burn" feed; a gap not smaller than the burn below the work top
/// leaves the electrode outside the work. Both refused by name.
#[test]
fn a_spark_gap_that_lifts_the_tip_out_of_the_work_is_refused_by_name() {
    let mut p = SinkerParams {
        x: 0.0,
        y: 0.0,
        approach_z: 25.0,
        top_z: 20.0,
        floor_z: 10.0,
        model_floor_z: Some(0.0),
        datum: SinkerDatum::WorkTop,
        depth: 6.0,
        electrode_len: 30.0,
        feed: 40.0,
        gap: 30.0,
        stand: None,
        orbit: None,
    };
    let e = burn_to_depth(&p).expect_err("a 30 mm gap feeds the tip up to Z 44");
    assert!(e.to_string().contains("the spark gap 30.000 places the tip at or above the approach plane"), "{e}");
    p.gap = 11.0; // tip Z 25: on the approach plane exactly
    let e = burn_to_depth(&p).expect_err("a tip on the approach plane");
    assert!(e.to_string().contains("at or above the approach plane"), "{e}");
    p.gap = 6.0; // tip Z 20: on the work top, the burn is 6
    let e = burn_to_depth(&p).expect_err("a gap as large as the burn");
    assert!(e.to_string().contains("the spark gap 6.000 is not smaller than the burn's 6.000 below the work top"), "{e}");
    p.gap = 5.5; // tip Z 19.5: inside the work
    let path = burn_to_depth(&p).expect("a gap smaller than the burn burns");
    assert!(path.moves.iter().all(|m| m.to.z <= 25.0), "nothing climbs past the approach plane");
}

/// #543 over the review of #1131: a rough or finish places its tip one
/// undersize stand-off above the floor (never less than the gap), and
/// the two refusals above read that stand - an undersize cannot lift
/// the tip past the approach plane or out of the work where a gap of
/// the same size would be refused.
#[test]
fn an_undersize_stand_off_that_lifts_the_tip_out_of_the_work_is_refused_by_name() {
    let mut p = SinkerParams {
        x: 0.0,
        y: 0.0,
        approach_z: 25.0,
        top_z: 20.0,
        floor_z: 10.0,
        model_floor_z: Some(0.0),
        datum: SinkerDatum::WorkTop,
        depth: 6.0,
        electrode_len: 30.0,
        feed: 40.0,
        gap: 0.5,
        stand: Some(30.0),
        orbit: None,
    };
    let e = burn_to_depth(&p).expect_err("a 30 mm stand-off feeds the tip up to Z 44");
    assert!(e.to_string().contains("the undersize stand-off 30.000 places the tip at or above the approach plane"), "{e}");
    p.stand = Some(6.0); // tip Z 20: on the work top, the burn is 6
    let e = burn_to_depth(&p).expect_err("a stand-off as large as the burn");
    assert!(e.to_string().contains("the undersize stand-off 6.000 is not smaller than the burn's 6.000 below the work top"), "{e}");
    p.stand = Some(0.75); // tip Z 14.75: inside the work, above the gap
    let path = burn_to_depth(&p).expect("a stand-off smaller than the burn burns");
    assert!(path.moves.iter().any(|m| (m.to.z - 14.75).abs() < 1e-9), "the tip stops one stand-off above the floor");
}

#[test]
fn a_rectangular_electrode_burns_its_cavity_to_the_stated_depth() {
    let (world, mut h) = sinker_doc(6.0);
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(SinkerDatum::WorkTop, 6.0, 14.0)).expect("a sinker burn on a sinker"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("generates"));
    assert!((feed_tip(&world, "Burn1") - (14.0 + GAP)).abs() < 1e-9, "the tip stops one gap above the floor");

    // The column engine, read directly: inside the face grown by the gap
    // the block stands at the floor, outside it untouched.
    let e = model::find_by_name(&world, "Burn1").expect("op");
    let moves = model::toolpath_of(&world, &e).expect("path").path.moves;
    let mut dex = Dexel::block(-30.0, -30.0, 30.0, 30.0, 20.0, 0.25).expect("grid");
    simulate_box(&mut dex, &moves, 5.0, 3.0, GAP).expect("burns");
    let top = |x: f64, y: f64| {
        let k = dex.col(camcraft_app::geo2d::Point2::new(x, y)).expect("in the grid");
        dex.top_at(k)
    };
    assert!((top(0.0, 0.0) - 14.0).abs() < 1e-9 && (top(4.9, 2.9) - 14.0).abs() < 1e-9, "the cavity floor");
    assert_eq!(top(5.3, 0.0), 20.0, "beside the face, past the gap");
    assert_eq!(top(0.0, 3.3), 20.0);

    // The program verdict judges it clean against the part.
    let sim = simulate_document(&world).expect("judged");
    assert_eq!(sim.dev.gouge_cells, 0, "a burn to the cavity's own floor gouges nothing: {:?}", sim.dev.max_gouge_at);
}

#[test]
fn a_burn_deeper_than_the_cavity_is_convicted_by_the_stock_sim() {
    // The floor datum admits 7 mm, but the part's cavity is 6 deep: the
    // door passes it, the box atom convicts it.
    let (world, mut h) = sinker_doc(6.0);
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(SinkerDatum::WorkTop, 7.0, 13.0)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("generates"));
    let sim = simulate_document(&world).expect("judged");
    assert!(sim.dev.gouge_cells > 0, "a 1 mm overburn is a gouge");
    assert!((sim.dev.max_gouge - 1.0).abs() < 0.05, "max gouge {}", sim.dev.max_gouge);
}

#[test]
fn a_depth_past_the_floor_datum_is_refused_by_name() {
    let (world, mut h) = sinker_doc(6.0);
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(SinkerDatum::WorkTop, 6.5, 14.0)).expect("create"));
    let Err(err) = GenerateOperation::new(&world, "Burn1") else { panic!("a burn past the floor datum generated") };
    assert!(err.to_string().contains("past the cavity floor datum"), "{err}");
    // From the cavity floor: the stand-off is what is left above it.
    let next = burn(SinkerDatum::CavityFloor, 0.0, 14.0);
    h.execute(&world, EditOperationParams::new(&world, "Burn1", next).expect("edit"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("burns to the floor"));
    assert!((feed_tip(&world, "Burn1") - (14.0 + GAP)).abs() < 1e-9);
    // From the electrode tip at the approach plane (Z 25): 11 reaches Z 14.
    h.execute(&world, EditOperationParams::new(&world, "Burn1", burn(SinkerDatum::ElectrodeTip, 11.0, 14.0)).expect("edit"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("burns"));
    assert!((feed_tip(&world, "Burn1") - (14.0 + GAP)).abs() < 1e-9);
    h.execute(&world, EditOperationParams::new(&world, "Burn1", burn(SinkerDatum::ElectrodeTip, 11.5, 14.0)).expect("edit"));
    let Err(err) = GenerateOperation::new(&world, "Burn1") else { panic!("past the floor from the tip") };
    assert!(err.to_string().contains("past the cavity floor datum"), "{err}");
}

#[test]
fn a_sinker_burn_needs_a_sinker_and_a_stated_condition() {
    // On a mill: refused at the door, by name.
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    h.execute(&world, AddTool::new(&world, ELECTRODE, electrode()).expect("tool"));
    let Err(err) = CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(SinkerDatum::WorkTop, 6.0, 14.0)) else {
        panic!("a sinker burn entered a mill");
    };
    assert!(err.to_string().contains("sinker EDM work") && err.to_string().contains("mount a sinker EDM"), "{err}");

    // On the sinker: a milling tool is refused by the tool law, a milling
    // op stays refused (#530), and an unknown condition is refused by name.
    let (world, mut h) = sinker_doc(6.0);
    let mill = camcraft_app::tools::catalog().into_iter().find(|(n, _)| *n == MILL_TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, MILL_TOOL, mill).expect("tool"));
    assert!(CreateOperation::new(&world, "Burn0", MILL_TOOL, SrcRef::Stock, burn(SinkerDatum::WorkTop, 6.0, 14.0)).is_err(), "an end mill is no electrode");
    let mut p = burn(SinkerDatum::WorkTop, 6.0, 14.0);
    if let OpParams::SinkerBurn(op) = &mut p {
        op.condition = "S9".into();
    }
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, p).expect("create"));
    let Err(err) = GenerateOperation::new(&world, "Burn1") else { panic!("an unknown condition generated") };
    assert!(err.to_string().contains("no EDM condition \"S9\""), "{err}");
}

#[test]
fn an_electrode_is_refused_at_add_tool_when_garbled_and_round_trips_the_file() {
    let world = new_world();
    let spun = Tool { rpm: 500.0, ..electrode() };
    let Err(err) = AddTool::new(&world, "E-bad", spun) else { panic!("a spinning electrode was admitted") };
    assert!(err.to_string().contains("turns no spindle"), "{err}");
    let flat = Tool { width: 0.0, ..electrode() };
    assert!(AddTool::new(&world, "E-flat", flat).is_err(), "no Y size");

    let (world, mut h) = sinker_doc(6.0);
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(SinkerDatum::CavityFloor, 0.25, 14.0)).expect("create"));
    let path = std::env::temp_dir().join(format!("camcraft-541-{}.ccam", std::process::id()));
    project::save(&world, &path).expect("save");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    let e = model::find_by_name(&back, "Burn1").expect("op survives");
    assert_eq!(model::operation_of(&back, &e).expect("op").params, burn(SinkerDatum::CavityFloor, 0.25, 14.0));
    let t = model::find_by_name(&back, ELECTRODE).and_then(|t| model::tool_of(&back, &t)).expect("tool survives");
    assert_eq!(t, electrode());
}

#[test]
fn the_sinker_burn_is_driven_over_mcp_with_the_gui_commands() {
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
    let mch = std::env::temp_dir().join(format!("camcraft-541-{}.mch", std::process::id()));
    std::fs::write(&mch, sinker_machine()).expect("write the machine");
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 60, "dy": 60, "dz": 20 } }));
    call(&mut s, &mut id, "load_machine", json!({ "path": mch.display().to_string() }));
    let _ = std::fs::remove_file(&mch);
    let def = project::tool_to_json(&electrode());
    call(&mut s, &mut id, "add_tool", json!({ "name": ELECTRODE, "definition": def, "holder": "none" }));
    let params = json!({ "depth": 6, "condition": "S1", "top_z": { "reference": "fixed", "offset": 20 }, "floor_z": { "reference": "fixed", "offset": 14 } });
    call(&mut s, &mut id, "add_operation", json!({ "kind": "Sinker Burn", "tool": ELECTRODE, "source": "stock", "name": "Burn1", "params": params }));
    call(&mut s, &mut id, "generate", json!({ "name": "Burn1" }));
    // Deeper than the floor datum: the edit lands (one undo step), the
    // generation refuses by name.
    call(&mut s, &mut id, "edit_operation", json!({ "name": "Burn1", "params": { "depth": 6.5 } }));
    id += 1;
    let reply = s
        .handle(&json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": "generate", "arguments": { "name": "Burn1" } } }))
        .expect("a response");
    let text = reply["result"]["content"][0]["text"].as_str().unwrap_or_default().to_owned();
    assert_eq!(reply["result"]["isError"], json!(true), "{reply}");
    assert!(text.contains("past the cavity floor datum"), "{text}");
    call(&mut s, &mut id, "undo", json!({}));
    call(&mut s, &mut id, "generate", json!({ "name": "Burn1" }));
    // The review of #1131: the document's built-in milling post does not
    // serve a sinker (#1232's door, since #542 ships the sinker post), so
    // the post tool refuses the burn by name and writes no file.
    let nc = std::env::temp_dir().join(format!("camcraft-1131-mcp-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&nc);
    id += 1;
    let args = json!({ "output": nc.display().to_string() });
    let reply = s
        .handle(&json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": "post", "arguments": args } }))
        .expect("a response");
    let text = reply["result"]["content"][0]["text"].as_str().unwrap_or_default().to_owned();
    assert_eq!(reply["result"]["isError"], json!(true), "{reply}");
    assert!(text.contains("does not serve") && text.contains("a sinker EDM machine") && text.contains("Burn1"), "{text}");
    assert!(!nc.exists(), "the refused post wrote {}", nc.display());
}

/// The #1131 refresh against main's wire EDM (`stamp_tool` split): the
/// burn's stamp still digests the sinker condition's spark gap and the
/// electrode's face width. A re-stated gap or a re-cut electrode reads
/// Stale (the post refuses until regenerated); undo reads Ok again.
#[test]
fn a_changed_spark_gap_or_electrode_face_marks_the_burn_stale() {
    use camcraft_app::commands::{ChangeTool, LoadMachine};
    use camcraft_app::machine::parse_mch;
    use camcraft_app::operation::{status_of, OpStatus};
    let (world, mut h) = sinker_doc(6.0);
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(SinkerDatum::WorkTop, 6.0, 14.0)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("generates"));
    let status = |w: &World| status_of(w, &model::find_by_name(w, "Burn1").expect("op"));
    assert_eq!(status(&world), OpStatus::Ok, "fresh after generating");

    // The same sinker, its one condition's gap re-stated.
    let mut v: serde_json::Value = serde_json::from_str(&sinker_machine()).expect("json");
    v["edm"]["conditions"][0]["gap"] = serde_json::json!(GAP * 2.0);
    let m = parse_mch(&v.to_string()).expect("sinker parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("remount"));
    assert_eq!(status(&world), OpStatus::Stale, "a re-stated spark gap is a different burn");
    assert!(h.undo(&world));
    assert_eq!(status(&world), OpStatus::Ok, "undo restores the gap the burn was cut with");

    // A wider electrode face (same length and diameter) is a different burn.
    let wider = camcraft_tools::electrode::electrode_tool(10.0, 7.0, 30.0, 15.0);
    h.execute(&world, ChangeTool::new(&world, ELECTRODE, wider).expect("re-cut the electrode"));
    assert_eq!(status(&world), OpStatus::Stale, "a re-cut electrode face is a different burn");
    assert!(h.undo(&world));
    assert_eq!(status(&world), OpStatus::Ok);
}

// ---------------------------------------------------------------- #542
// The generic ISO-style sinker post through the real op: every EDM code
// from the synthetic fixture dialect's `edm.codes` (#290 D5). Not
// exercised on a real EDM control.

const DIALECT: &str = include_str!("edm/sinker-iso-dialect.mch");
const SINKER_POST: &str = "Sinker EDM (ISO)";

/// The burn of `a_rectangular_electrode_burns_its_cavity_to_the_stated_depth`
/// on the fixture dialect, posted with `post`: the program, or the refusal.
fn post_burn(mch: &str, post: &str) -> Result<String, String> {
    use camcraft_app::commands::SelectPost;
    use camcraft_app::schema::PostRef;
    let (world, mut h) = sinker_doc_on(mch, 6.0);
    h.execute(&world, CreateOperation::new(&world, "Burn1", ELECTRODE, SrcRef::Stock, burn(SinkerDatum::WorkTop, 6.0, 14.0)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Burn1").expect("generates"));
    h.execute(&world, SelectPost::new(&world, PostRef::Named(post.into())).expect("a registry post"));
    let out = std::env::temp_dir().join(format!("camcraft-542-{}-{post}.nc", std::process::id()));
    let posted = camcraft_app::posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    posted.map(|p| p.program)
}

#[test]
fn the_sinker_burn_posts_through_the_generic_sinker_post_to_its_golden() {
    let program = post_burn(DIALECT, SINKER_POST).expect("the sinker post writes the burn");
    let rel = "tests/golden/nc/sinker-iso/sinker_burn_program.nc";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&path, &program).expect("write golden");
    }
    assert_eq!(program, std::fs::read_to_string(&path).expect("golden"), "the sinker program moved a byte");
    // The condition the op names and the electrode's 15 mm pickup offset
    // ride the stream; the tip stops one gap above the 14 mm floor.
    for line in ["ES1", "M100 H15.", "M101 Z14.05"] {
        assert!(program.lines().any(|l| l == line), "{line} in\n{program}");
    }
}

#[test]
fn a_missing_sinker_code_refuses_the_post_by_name() {
    let mut v: serde_json::Value = serde_json::from_str(DIALECT).expect("fixture");
    v["edm"]["codes"].as_object_mut().expect("codes").remove("condition");
    let err = post_burn(&v.to_string(), SINKER_POST).expect_err("no condition code, no program");
    assert!(err.contains("states no edm.codes.condition"), "{err}");
}

#[test]
fn a_mill_post_on_a_sinker_machine_is_still_refused() {
    use camcraft_app::machine::Kind;
    use camcraft_app::posting;
    for post in ["Fanuc", "Haas", "Tormach PathPilot Plasma"] {
        let err = post_burn(DIALECT, post).expect_err("a non-EDM post on a sinker");
        assert!(err.contains("a sinker EDM machine") && err.contains("does not serve"), "{post}: {err}");
        assert!(!posting::post_serves(post, Kind::SinkerEdm), "{post}");
    }
    assert_eq!(posting::SINKER_POSTS, &[SINKER_POST]);
    assert!(posting::post_serves(SINKER_POST, Kind::SinkerEdm));
    for k in [Kind::Mill, Kind::Lathe, Kind::MillTurn, Kind::Plasma, Kind::WireEdm] {
        assert!(!posting::post_serves(SINKER_POST, k), "{k:?}");
    }
    assert_eq!(posting::default_post_for(Kind::SinkerEdm), camcraft_app::schema::PostRef::Named(SINKER_POST.into()));
}

/// Rework onto main's EDM designs: an orbit burn (#544) and a Sinker
/// Side Burn (#545) carry `SinkerCondition` and the pickup into their
/// stream, and the orbit its geometry and radius - so the sinker post
/// refuses each by name (`script_sinker_iso.rs`) instead of reading
/// their explicit moves as a stray feed.
#[test]
fn an_orbit_burn_and_a_side_burn_say_their_sinker_params_in_the_stream() {
    use camcraft_app::operation::{SinkerBurnOp, SinkerOrbit, SinkerOrbitKind, SinkerSideBurnOp, SinkerSideDirection};
    use camcraft_app::post::cnc::Record;
    use serde_json::json;
    let mut v: serde_json::Value = serde_json::from_str(DIALECT).expect("fixture");
    v["edm"]["max_orbit_radius"] = json!(1.0);
    let (world, mut h) = sinker_doc_on(&v.to_string(), 6.0);
    let OpParams::SinkerBurn(plain) = burn(SinkerDatum::WorkTop, 6.0, 14.0) else { panic!("a burn") };
    let orbit = SinkerBurnOp { orbit: Some(SinkerOrbit { kind: SinkerOrbitKind::Circular, radius: 0.04, vectors: 8, turns: 2 }), ..plain.clone() };
    let side = SinkerSideBurnOp { burn: plain, direction: SinkerSideDirection::PlusX, amount: 0.04, undersize: 0.1 };
    h.execute(&world, CreateOperation::new(&world, "Orbit1", ELECTRODE, SrcRef::Stock, OpParams::SinkerBurn(orbit)).expect("an orbit burn"));
    h.execute(&world, CreateOperation::new(&world, "Side1", ELECTRODE, SrcRef::Stock, OpParams::SinkerSideBurn(side)).expect("a side burn"));
    for n in ["Orbit1", "Side1"] {
        h.execute(&world, GenerateOperation::new(&world, n).unwrap_or_else(|e| panic!("{n} generates: {e}")));
    }
    let streams = camcraft_app::posting::op_streams(&world).expect("streams");
    let param = |op: &str, name: &str| {
        let recs = &streams.iter().find(|(n, _)| n == op).unwrap_or_else(|| panic!("{op} streams")).1;
        recs.iter().find_map(|r| match r {
            Record::Param { name: n, value } if n == name => Some(value.clone()),
            _ => None,
        })
    };
    for op in ["Orbit1", "Side1"] {
        assert_eq!(param(op, "SinkerCondition"), Some(json!("S1")), "{op}");
        assert_eq!(param(op, "EdmCondition"), None, "{op}: the wire section's word is not the sinker's");
        assert!(param(op, "ElectrodePickup").is_some(), "{op}");
    }
    assert_eq!(param("Orbit1", "EdmOrbit"), Some(json!("circular")));
    assert_eq!(param("Orbit1", "EdmOrbitRadius"), Some(json!(0.04)));
    assert_eq!(param("Side1", "EdmOrbit"), None);
}
