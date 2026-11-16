//! #1110 (part of #290): the readers of `Operation::tool` on a document
//! whose setup mounts a wire EDM and holds a generated Wire Contour -
//! a cut that mounts no library tool (`KindInfo::mounts_tool`, #532).
//! The program stock sim answers by name ("verified by the wire laws"),
//! never "tool  is not in the library"; the setup sheet shows the cut's
//! condition, wire diameter and gap and no missing-tool line; the tool
//! list and its export hold no row for the empty tool; the cutting-data
//! table says why it cannot speak. Beside each, the milling twin: an op
//! whose library tool is gone is still refused or listed exactly as
//! before - the skip keys on the kind, never on a missing tool.
//!
//! Gate map (#1110): covers app::stock_sim (stock_sim_wire, plan_program,
//! simulate_document), app::setup_sheet (operations, tools),
//! app::tool_list, app::tool_list_export, app::cutting, app::posting_sim.
//! Provenance: our own tests over our own fixtures; the EDM conditions are
//! SYNTHETIC numbers of a plausible size (#290 D4, D10).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, DeleteTool, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::{Height, OpParams, SrcRef, WireContourOp, WireSide};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;
use camcraft_app::{cutting, posting, setup_sheet, stock_sim, tool_list, tool_list_export};

const WIRE: &str = include_str!("edm/wire-edm.mch");
const TABLE: &str = r#"[ { "id": "C110", "name": "rough", "gap": 0.04, "wire_diameter": 0.25, "role": "rough" } ]"#;
const MISSING: &str = "is not in the library";

/// A 40 x 20 x 12 slug with 2 mm corners (3-degree facets), counter-clockwise.
fn slug() -> Mesh {
    let (hx, hy, r) = (18.0, 8.0, 2.0);
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(hx, hy, 0.0_f64), (-hx, hy, 90.0), (-hx, -hy, 180.0), (hx, -hy, 270.0)] {
        for s in 0..=30 {
            let t = (a0 + 3.0 * f64::from(s)).to_radians();
            pts.push((cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, 12.0), p(a, 12.0), p(b, 12.0)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(b, 0.0), p(a, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 12.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 12.0), p(a, 12.0)));
    }
    Mesh::from_facets(f)
}

/// The issue's document: a wire EDM with condition C110 mounted, the slug
/// on a stock, and the generated die cut "Die1".
fn wire_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let mut v: serde_json::Value = serde_json::from_str(WIRE).expect("template json");
    v["edm"]["conditions"] = serde_json::from_str(TABLE).expect("conditions json");
    let m = parse_mch(&v.to_string()).expect("parses");
    h.execute(&world, LoadMachine::new(&world, "wire.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("slug", slug()));
    let stock = SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 30.0, dz: 12.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    let op = WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Die,
        start: Some([0.0, 0.0]),
        tabs: None,
        capture: true,
        taper_deg: 0.0,
        burnout: None,
        clearance: camcraft_app::operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
        four_axis: None,
    };
    let create = CreateOperation::new(&world, "Die1", "", SrcRef::Entity("slug".into()), OpParams::WireContour(op));
    h.execute(&world, create.expect("die"));
    h.execute(&world, GenerateOperation::new(&world, "Die1").expect("generates"));
    (world, h)
}

/// The milling twin: a generated face op whose library tool is then
/// deleted - the missing tool every reader still refuses or lists.
fn milling_doc_with_a_lost_tool() -> World {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    let (name, tool) = camcraft_app::tools::catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").expect("catalog");
    h.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    let face = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    h.execute(&world, CreateOperation::new(&world, "Face1", name, SrcRef::Stock, face).expect("face"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generates"));
    h.execute(&world, DeleteTool::new(&world, name).expect("delete"));
    world
}

#[test]
fn the_program_sim_answers_a_wire_cut_by_name() {
    let (world, _h) = wire_doc();
    let err = stock_sim::simulate_document(&world).expect_err("a wire program is not judged by the column verdict");
    assert!(err.contains("Die1") && err.contains(stock_sim::wire::WIRE_VERDICT_REFUSAL), "{err}");
    assert!(!err.contains(MISSING), "{err}");
    // The post gate shows it as the not-judged line - by name, not as a
    // blocking problem (the cut carries its generator-time verdict).
    let report = posting::verification_report(&world);
    let line = report.not_judged.expect("said, not silenced");
    assert!(line.contains(stock_sim::wire::WIRE_VERDICT_REFUSAL) && !line.contains(MISSING), "{line}");
    assert!(report.problems.iter().all(|p| !p.contains(MISSING)), "{:?}", report.problems);
    // The setup sheet's opt-in verdict line says the same.
    let verdict = setup_sheet::simulate_verdict(&world);
    assert!(verdict.contains("not judged") && verdict.contains("wire laws"), "{verdict}");
}

#[test]
fn a_milling_op_with_a_missing_tool_is_still_refused_by_the_program_sim() {
    let world = milling_doc_with_a_lost_tool();
    let err = stock_sim::simulate_document(&world).expect_err("a lost tool is never simulated past");
    assert!(err.contains("EM-0400-2FL"), "the refusal names the tool: {err}");
    assert!(!err.contains(stock_sim::wire::WIRE_VERDICT_REFUSAL), "never the wire's skip: {err}");
}

#[test]
fn the_setup_sheet_shows_the_wire_condition_and_no_missing_tool() {
    let (world, _h) = wire_doc();
    let sheet = setup_sheet::render(&world, None).expect("sheet");
    let row = sheet.lines().find(|l| l.contains("Die1")).expect("the op row");
    assert!(row.contains("cond C110"), "the TOOL cell names the condition: {row}");
    assert!(sheet.contains("CONDITION C110  WIRE D 0.250  GAP 0.040"), "{sheet}");
    assert!(!sheet.contains("no longer in the document"), "{sheet}");
    assert!(!sheet.contains("tool ''"), "{sheet}");
}

#[test]
fn the_setup_sheet_still_says_a_milling_tool_is_gone() {
    let world = milling_doc_with_a_lost_tool();
    let sheet = setup_sheet::render(&world, None).expect("sheet");
    assert!(sheet.contains("(tool 'EM-0400-2FL' is no longer in the document)"), "{sheet}");
    assert!(!sheet.contains("CONDITION"), "{sheet}");
}

#[test]
fn the_tool_list_and_its_export_hold_no_row_for_a_wire_cut() {
    let (world, _h) = wire_doc();
    assert!(tool_list::for_setup(&world, None).is_empty(), "a wire EDM has no tool table");
    assert!(tool_list::program(&world).is_empty());
    assert!(tool_list_export::rows(&world, None).is_empty());
    let csv = tool_list_export::csv(&world, None);
    assert_eq!(csv.lines().count(), 1, "the header alone: {csv}");
}

#[test]
fn the_tool_list_keeps_a_row_for_a_lost_milling_tool() {
    let world = milling_doc_with_a_lost_tool();
    let rows = tool_list::for_setup(&world, None);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].tool.as_str(), rows[0].ops.as_slice()), ("EM-0400-2FL", ["Face1".to_owned()].as_slice()));
    let export = tool_list_export::rows(&world, None);
    assert_eq!((export[0].number.as_str(), export[0].size.as_str()), ("?", "missing"), "the list shows what the program would load");
}

#[test]
fn the_cutting_data_table_says_why_it_cannot_speak_for_a_wire_cut() {
    let (world, _h) = wire_doc();
    let err = cutting::suggest(&world, "Die1").expect_err("no tool to tabulate");
    assert!(err.contains("mounts no library tool") && err.contains("EDM condition"), "{err}");
    assert!(!err.contains(MISSING), "{err}");
    let world = milling_doc_with_a_lost_tool();
    let err = cutting::suggest(&world, "Face1").expect_err("a lost tool");
    assert_eq!(err, "tool EM-0400-2FL is not in the library", "the milling refusal is unchanged");
}
