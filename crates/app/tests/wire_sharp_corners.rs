//! #3207: a Wire Contour states that its inside corners may keep the
//! wire's own radius. A sharp 10 mm square die opening is refused by
//! default, and cut when the option is stated; the leftover in the
//! corners is a note on the path and on the verify line, never a
//! refusal; the option is one undoable edit and rides the project file,
//! absent when off.
//!
//! Gate map (#3207): covers app::operation (types::wire_edm, wire_op,
//! wire_verify), app::project (wire_edm_dto), app::posting
//! (posting_wire), motion::wire_edm (laws).
//! Provenance: our own fixtures. The condition is SYNTHETIC - invented
//! numbers of a plausible size, from no machine's manual (#290 D4, D10).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef, WireContourOp, WireSide, DEFAULT_CLEARANCE};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const WIRE: &str = include_str!("edm/wire-edm.mch");
/// A wire machine whose guides lean (synthetic, #536): for the taper.
const TAPER_WIRE: &str = include_str!("edm/wire-iso-fixture.mch");
/// Synthetic: wire 0.25, gap 0.04 - a kerf offset of 0.165.
const TABLE: &str = r#"[{ "id": "C110", "name": "rough", "gap": 0.04, "wire_diameter": 0.25, "role": "rough" }]"#;
const K: f64 = 0.165;

/// A sharp `2h` square, extruded from z = 0 to z = 12.
fn square_prism(h: f64) -> Mesh {
    let pts = [(-h, -h), (h, -h), (h, h), (-h, h)];
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

fn doc() -> (World, History) {
    doc_on(WIRE)
}

fn doc_on(machine: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let mut v: serde_json::Value = serde_json::from_str(machine).expect("template json");
    v["edm"]["conditions"] = serde_json::from_str(TABLE).expect("conditions json");
    let m = parse_mch(&v.to_string()).expect("parses");
    h.execute(&world, LoadMachine::new(&world, "wire.mch".into(), Some(m)).expect("mount"));
    h.execute(
        &world,
        camcraft_app::commands::AddTool::new(&world, "EDM-W025", camcraft_app::wire_tools::wire_tool(0.25)).expect("wire"),
    );
    h.execute(&world, ImportModel::from_mesh("opening", square_prism(5.0)));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 12.0 }, Location::default()).expect("stock");
    h.execute(&world, stock);
    (world, h)
}

fn die(sharp_corners: bool) -> WireContourOp {
    WireContourOp {
        lead_arc_radius: None,
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Die,
        picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(0.0, 0.0))], exit: Vec::new() },
        tabs: None,
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: DEFAULT_CLEARANCE,
        skims: Vec::new(),
        sharp_corners,
    }
}

#[test]
fn a_sharp_square_die_is_cut_and_verified_with_its_corner_rest_as_a_note() {
    let (world, mut h) = doc();
    let c = CreateOperation::new(&world, "Die1", "EDM-W025", SrcRef::Entity("opening".into()), OpParams::WireContour(die(false)))
        .expect("op");
    h.execute(&world, c);
    // The default stays as it was: refused by name.
    let err = GenerateOperation::new(&world, "Die1").err().expect("refused").to_string();
    assert!(err.contains("inside corner") && err.contains("minimum inside radius is 0.1650 mm"), "{err}");
    // Stated: one undoable edit, then the die is cut.
    h.execute(&world, EditOperationParams::new(&world, "Die1", OpParams::WireContour(die(true))).expect("edit"));
    h.execute(&world, GenerateOperation::new(&world, "Die1").expect("a sharp die is cut"));
    let rest = format!("{:.4}", (2.0_f64.sqrt() - 1.0) * K);
    let e = model::find_by_name(&world, "Die1").expect("op");
    let notes = model::toolpath_of(&world, &e).expect("generated").path.notes;
    assert!(notes.iter().any(|n| n.contains("sharp inside corners stated") && n.contains(&rest)), "{notes:?}");
    // The verify passes, and says the rest on its line.
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let line = clause.line.expect("the judged line");
    assert!(line.contains("Die1") && line.contains("sharp inside corners stated") && line.contains(&rest), "{line}");
    // The project file: the key rides when on, and is absent when off.
    let OpParams::WireContour(on) = model::operation_of(&world, &e).expect("op").params else { panic!("wire") };
    let json = project::params_to_json(&OpParams::WireContour(on.clone()));
    assert!(json.to_string().contains("\"sharp_corners\":true"), "{json}");
    assert_eq!(project::params_from_json(&json).expect("reads back"), OpParams::WireContour(on));
    let off = project::params_to_json(&OpParams::WireContour(die(false)));
    assert!(!off.to_string().contains("sharp_corners"), "absent when off: {off}");
    // Undo (the generation, then the edit) restores the default.
    assert!(h.undo(&world) && h.undo(&world));
    let OpParams::WireContour(back) = model::operation_of(&world, &e).expect("op").params else { panic!("wire") };
    assert!(!back.sharp_corners, "undoing the edit restores the default");
}

/// A tapered sharp die (#3232 review): the corner rest rides the tapered
/// path and the verify line, as for the vertical cut.
#[test]
fn a_tapered_sharp_square_die_says_its_corner_rest_on_the_verify_line() {
    let (world, mut h) = doc_on(TAPER_WIRE);
    // The opening is a straight prism: a die that narrows downward stays
    // inside it, where one that widens downward burns the part below.
    let op = WireContourOp { taper_deg: -1.0, ..die(true) };
    let c = CreateOperation::new(&world, "Die1", "EDM-W025", SrcRef::Entity("opening".into()), OpParams::WireContour(op)).expect("op");
    h.execute(&world, c);
    h.execute(&world, GenerateOperation::new(&world, "Die1").expect("a tapered sharp die is cut"));
    let e = model::find_by_name(&world, "Die1").expect("op");
    let notes = model::toolpath_of(&world, &e).expect("generated").path.notes;
    assert!(notes.iter().any(|n| n.contains("sharp inside corners stated") && n.contains("worst uncut rest")), "{notes:?}");
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let line = clause.line.expect("the judged line");
    assert!(line.contains("Die1") && line.contains("sharp inside corners stated") && line.contains("worst uncut rest"), "{line}");
}
