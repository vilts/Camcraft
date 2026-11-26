//! #1883: the Waterjet Profile operation through the commands - the op
//! on a waterjet only (the family door both ways), generation over a
//! holed plate (hole first, pierces in the scrap, tabs as gaps), the
//! named refusals (a milling tool), undo / redo, and the project
//! round-trip of the op.
//! Provenance: our own tests over our own fixtures; the waterjet rules
//! they pin are cited in `crates/motion/src/waterjet/mod.rs`.
//!
//! Gate map (#1883): covers app::operation (waterjet_op, kinds
//! registry, family gate), app::commands (operation create/edit/
//! generate), app::project (waterjet_dto), app::posting, app::stock_sim,
//! motion::waterjet.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{self, Kind};
use camcraft_app::model;
use camcraft_app::operation::{
    Family, Height, JetLead, JetSide, JetTabsOp, KindInfo, OpParams, OpType, Section, SrcRef, WaterjetOp,
};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::waterjet_tools::waterjet_tool;
use camcraft_app::world::new_world;

const NOZZLE: &str = "Nozzle";

fn ring(h: f64, z: f64) -> [Point3; 4] {
    [Point3::new(-h, -h, z), Point3::new(h, -h, z), Point3::new(h, h, z), Point3::new(-h, h, z)]
}

fn quad(a: Point3, b: Point3, c: Point3, d: Point3) -> [Triangle; 2] {
    [Triangle::new(a, b, c), Triangle::new(a, c, d)]
}

fn annulus(outer: [Point3; 4], inner: [Point3; 4], up: bool, out: &mut Vec<Triangle>) {
    for i in 0..4 {
        let (o0, o1, i0, i1) = (outer[i], outer[(i + 1) % 4], inner[i], inner[(i + 1) % 4]);
        out.extend(if up { quad(o0, o1, i1, i0) } else { quad(o0, i0, i1, o1) });
    }
}

fn walls(r: f64, z0: f64, z1: f64, outward: bool, out: &mut Vec<Triangle>) {
    let (lo, hi) = (ring(r, z0), ring(r, z1));
    for i in 0..4 {
        let (a, b, c, d) = (lo[i], lo[(i + 1) % 4], hi[(i + 1) % 4], hi[i]);
        out.extend(if outward { quad(a, b, c, d) } else { quad(a, d, c, b) });
    }
}

/// A 40x40x5 plate with a 10x10 square hole through it.
fn holed_plate() -> Mesh {
    let mut f = Vec::new();
    annulus(ring(20.0, 5.0), ring(5.0, 5.0), true, &mut f);
    annulus(ring(20.0, 0.0), ring(5.0, 0.0), false, &mut f);
    walls(20.0, 0.0, 5.0, true, &mut f);
    walls(5.0, 0.0, 5.0, false, &mut f);
    Mesh::from_facets(f)
}

const WATERJET: &str = r#"{"general":{"model":"Waterjet test","kind":"waterjet"},"kinematics":{"default":{"parts":[{"name":"X","type":"linear","travel":{"min":-100,"max":360}},{"name":"Y","type":"linear","travel":{"min":-150,"max":150}},{"name":"Z","type":"linear","travel":{"min":-20,"max":60}},{"name":"nozzle","type":"head"}]}},"controller":{"synced_configuration":{"parts":{"X":{"coordinate":0,"max_normal_speed":1000},"Y":{"coordinate":1,"max_normal_speed":700},"Z":{"coordinate":2,"max_normal_speed":500}}}}}"#;

fn waterjet_op() -> WaterjetOp {
    WaterjetOp {
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        kerf: None,
        control_comp: false,
        reversed: false,
        lead: JetLead::Arc,
        lead_in: 3.0,
        lead_out: 2.0,
        open_side: JetSide::Center,
        tabs: None,
        feed: 250.0,
        pierce_time: 1.5,
        abrasive_delay: 0.5,
        linearize: None,
        picks: Default::default(),
    }
}

/// A waterjet document: the waterjet mounted, the nozzle, the plate.
fn waterjet_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let m = machine::parse_mch(WATERJET).expect("valid waterjet");
    h.execute(&world, LoadMachine::new(&world, "wj.mch".into(), Some(m)).expect("mounts"));
    h.execute(&world, AddTool::new(&world, NOZZLE, waterjet_tool(0.8)).expect("nozzle"));
    h.execute(&world, ImportModel::from_mesh("plate", holed_plate()));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 5.0 }, Location::default()).expect("stock"));
    (world, h)
}

fn cut(world: &World, h: &mut History, params: OpParams) {
    h.execute(world, CreateOperation::new(world, "Jet1", NOZZLE, SrcRef::Entity("plate".into()), params).expect("op"));
    h.execute(world, GenerateOperation::new(world, "Jet1").expect("generates"));
}

fn path(world: &World) -> camcraft_app::cl::Toolpath {
    let e = model::find_by_name(world, "Jet1").expect("op");
    model::toolpath_of(world, &e).expect("generated").path
}

fn pierces(p: &camcraft_app::cl::Toolpath) -> Vec<(f64, f64)> {
    p.moves.iter().filter(|m| m.kind == MoveKind::Rapid && m.to.z == 5.0 && m.from.z > 5.0).map(|m| (m.to.x, m.to.y)).collect()
}

#[test]
fn the_waterjet_family_lives_on_a_waterjet_only() {
    let info = OpType::WaterjetProfile.info();
    assert_eq!((info.label, info.section, info.family()), ("Waterjet Profile", Section::Waterjet, Family::Waterjet));
    assert!(KindInfo::ALL.iter().any(|k| k.ty == OpType::WaterjetProfile));
    assert_eq!(Section::Waterjet.kinds_for(Kind::Waterjet).len(), 1);
    for kind in [Kind::Mill, Kind::MillTurn, Kind::Lathe, Kind::Plasma, Kind::WireEdm, Kind::SinkerEdm] {
        assert!(Section::Waterjet.kinds_for(kind).is_empty(), "{kind:?} admits no waterjet work");
    }
    // A mill document refuses the op by name.
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, AddTool::new(&world, NOZZLE, waterjet_tool(0.8)).expect("nozzle"));
    h.execute(&world, ImportModel::from_mesh("plate", holed_plate()));
    let err = CreateOperation::new(&world, "Jet1", NOZZLE, SrcRef::Entity("plate".into()), OpParams::WaterjetProfile(waterjet_op()))
        .err()
        .expect("a mill refuses waterjet work");
    assert!(format!("{err:?}").contains("waterjet work"), "{err:?}");
}

#[test]
fn a_holed_plate_cuts_its_hole_first_with_every_pierce_in_the_scrap() {
    let (world, mut h) = waterjet_doc();
    cut(&world, &mut h, OpParams::WaterjetProfile(waterjet_op()));
    let p = path(&world);
    let ps = pierces(&p);
    assert_eq!(ps.len(), 2, "{ps:?}");
    // The hole first: its pierce stands inside the 10x10 hole; the
    // outline's outside the 40x40 plate.
    assert!(ps[0].0.abs() < 5.0 && ps[0].1.abs() < 5.0, "hole first: {ps:?}");
    assert!(ps[1].0.abs() > 20.0 || ps[1].1.abs() > 20.0, "outline last, outside: {ps:?}");
    assert!(p.notes.iter().any(|n| n.contains("cut inner first (1 hole(s), 0 tab(s)), 2 pierce(s)")), "{:?}", p.notes);
}

#[test]
fn tabs_split_the_outline_into_led_in_stretches() {
    let (world, mut h) = waterjet_doc();
    let tabs = JetTabsOp { count: 2, width: 2.0, at: vec![[0.0, -20.0], [0.0, 20.0]] };
    cut(&world, &mut h, OpParams::WaterjetProfile(WaterjetOp { tabs: Some(tabs), ..waterjet_op() }));
    let p = path(&world);
    assert_eq!(pierces(&p).len(), 3, "the hole, then two stretches of the outline");
    assert!(p.notes.iter().any(|n| n.contains("2 tab(s)), 3 pierce(s)")), "{:?}", p.notes);
}

#[test]
fn a_milling_tool_is_refused_by_name() {
    let (world, mut h) = waterjet_doc();
    let em = camcraft_app::tools::catalog().remove(0);
    h.execute(&world, AddTool::new(&world, em.0, em.1).expect("end mill"));
    let r = CreateOperation::new(&world, "Jet1", em.0, SrcRef::Entity("plate".into()), OpParams::WaterjetProfile(waterjet_op()));
    assert!(r.is_err(), "a waterjet profile mounts a nozzle only");
}

#[test]
fn create_and_edit_undo_and_redo() {
    let (world, mut h) = waterjet_doc();
    let p0 = OpParams::WaterjetProfile(waterjet_op());
    h.execute(&world, CreateOperation::new(&world, "Jet1", NOZZLE, SrcRef::Entity("plate".into()), p0.clone()).expect("op"));
    let p1 = OpParams::WaterjetProfile(WaterjetOp { reversed: true, lead: JetLead::Straight, linearize: Some(0.01), ..waterjet_op() });
    h.execute(&world, EditOperationParams::new(&world, "Jet1", p1.clone()).expect("edit"));
    let params = |w: &World| model::find_by_name(w, "Jet1").and_then(|e| model::operation_of(w, &e)).map(|o| o.params);
    assert_eq!(params(&world), Some(p1.clone()));
    assert!(h.undo(&world));
    assert_eq!(params(&world), Some(p0.clone()));
    assert!(h.undo(&world));
    assert_eq!(params(&world), None);
    assert!(h.redo(&world));
    assert_eq!(params(&world), Some(p0));
    assert!(h.redo(&world));
    assert_eq!(params(&world), Some(p1));
    h.execute(&world, GenerateOperation::new(&world, "Jet1").expect("generates"));
    let p = path(&world);
    assert!(!p.has_arcs(), "linearized");
    assert!(p.notes.iter().any(|n| n.contains("REVERSED")), "{:?}", p.notes);
    // A bad number is refused at the command door.
    let bad = WaterjetOp { feed: 0.0, ..waterjet_op() };
    assert!(EditOperationParams::new(&world, "Jet1", OpParams::WaterjetProfile(bad)).is_err());
}

#[test]
fn the_op_round_trips_byte_identically() {
    let (world, mut h) = waterjet_doc();
    let tabs = JetTabsOp { count: 1, width: 1.5, at: vec![[20.0, 0.0]] };
    let op = WaterjetOp { kerf: Some(0.9), control_comp: true, open_side: JetSide::Left, tabs: Some(tabs), linearize: Some(0.02), ..waterjet_op() };
    cut(&world, &mut h, OpParams::WaterjetProfile(op.clone()));
    let file = std::env::temp_dir().join(format!("camcraft_waterjet_op_rt_{}.ccam", std::process::id()));
    camcraft_app::project::save(&world, &file).expect("save");
    let first = std::fs::read_to_string(&file).expect("read");
    let again = new_world();
    camcraft_app::project::load(&again, &file).expect("load");
    let e = model::find_by_name(&again, "Jet1").expect("op");
    assert_eq!(model::operation_of(&again, &e).map(|o| o.params), Some(OpParams::WaterjetProfile(op)));
    camcraft_app::project::save(&again, &file).expect("resave");
    let second = std::fs::read_to_string(&file).expect("reread");
    let _ = std::fs::remove_file(&file);
    assert!(first.contains("\"waterjet_profile\"") && first.contains("\"tabs\""), "the words are written");
    assert_eq!(first, second, "byte-identical");
}

#[test]
fn waterjet_post_gate_names_the_missing_safety_records() {
    let (world, mut h) = waterjet_doc();
    cut(&world, &mut h, OpParams::WaterjetProfile(waterjet_op()));
    let report = camcraft_app::posting::verification_report(&world);
    let refusal = report.problems.iter().find(|p| p.contains("Jet1") && p.contains("standoff"))
        .expect("the waterjet operation must be refused by name");
    for missing in ["jet on/off", "pierce dwell", "waterjet laws"] {
        assert!(refusal.contains(missing), "{refusal}");
    }
}

#[test]
fn waterjet_user_post_cannot_bypass_the_safety_refusal() {
    let (world, mut h) = waterjet_doc();
    cut(&world, &mut h, OpParams::WaterjetProfile(waterjet_op()));
    let file = std::env::temp_dir().join(format!("camcraft_waterjet_refused_{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&file);
    let script = r#"serves = ["waterjet"]; function onOpen() { writeln("(WATERJET)"); }"#;
    let refusal = camcraft_app::posting::post_script(&world, &file, script).expect_err("a user post must refuse unsafe waterjet work");
    assert!(!file.exists(), "a refused waterjet post must not write an NC file");
    assert!(refusal.contains("Jet1") && refusal.contains("standoff") && refusal.contains("jet on/off"), "{refusal}");
}

#[test]
fn waterjet_safety_refusal_omits_suppressed_operations_and_empty_documents() {
    let (world, mut h) = waterjet_doc();
    let empty = camcraft_app::posting::verification_report(&world);
    assert!(!empty.problems.iter().any(|p| p.contains("waterjet")), "{:?}", empty.problems);
    cut(&world, &mut h, OpParams::WaterjetProfile(waterjet_op()));
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Jet1", true).expect("suppress"));
    let report = camcraft_app::posting::verification_report(&world);
    assert!(!report.problems.iter().any(|p| p.contains("Jet1") && p.contains("standoff")), "{:?}", report.problems);
    assert!(!report.not_judged.as_deref().unwrap_or_default().contains("Jet1"), "{:?}", report.not_judged);
}

#[test]
fn waterjet_stock_verdict_is_refused_by_name() {
    let (world, mut h) = waterjet_doc();
    cut(&world, &mut h, OpParams::WaterjetProfile(waterjet_op()));
    let refusal = camcraft_app::stock_sim::simulate_document(&world).expect_err("waterjet stock removal is not judged");
    assert!(refusal.contains("Jet1") && refusal.contains("waterjet") && refusal.contains("not judged"), "{refusal}");
}

#[test]
fn waterjet_nozzle_is_absent_from_stock_playback() {
    let (world, mut h) = waterjet_doc();
    cut(&world, &mut h, OpParams::WaterjetProfile(waterjet_op()));
    let playback = camcraft_app::stock_play::PlaySeed::gather(&world, false, 1000.0).expect("the stock playback plan is available");
    assert_eq!(playback.index().total(), 0, "a waterjet nozzle must not remove simulated stock");
}

#[test]
fn waterjet_verification_reports_the_unjudged_operation() {
    let (world, mut h) = waterjet_doc();
    cut(&world, &mut h, OpParams::WaterjetProfile(waterjet_op()));
    let report = camcraft_app::posting::verification_report(&world);
    let line = report.not_judged.expect("the waterjet operation must have a visible not-judged note");
    assert!(line.contains("Jet1") && line.contains("waterjet"), "{line}");
}

#[test]
fn waterjet_zero_lead_in_reports_the_pierce_on_the_kerf_ring() {
    let (world, mut h) = waterjet_doc();
    cut(&world, &mut h, OpParams::WaterjetProfile(WaterjetOp { lead_in: 0.0, ..waterjet_op() }));
    let p = path(&world);
    assert!(p.notes.iter().any(|n| n.contains("zero lead-in") && n.contains("kerf ring")), "{:?}", p.notes);
}
