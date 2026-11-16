//! #670: the Plasma Profile operation through the commands - a torch
//! tool (kerf = diameter, rpm refused by name), the op on a plasma
//! table only (the family door both ways), generation over a holed
//! plate (hole first, the small-hole rule, the torch records), the
//! named refusals (a hole under the kerf, a milling tool), undo / redo,
//! the machine check's verdict, and the project round-trip of the op,
//! the torch and the cached plasma record.
//! Provenance: our own tests over our own fixtures; the plasma rules
//! they pin are cited in `crates/motion/src/plasma/mod.rs`.
//!
//! Gate map (#670): covers app::operation (plasma_op, kinds registry,
//! family gate), app::commands (tool, operation create/edit/generate),
//! app::machine, post::machine::plasma (cuts_plasma), app::project (plasma_dto, tool
//! kind word), motion::plasma, cl torch records.

use flecs_ecs::prelude::World;

use camcraft_app::cl::ContourRole;
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{self, check_document, Kind};
use camcraft_app::model;
use camcraft_app::operation::{
    self, Family, FeatureSel, GeoSel, Height, KindInfo, OpParams, OpType, PlasmaComp, PlasmaOp, Section, SrcRef,
};
use camcraft_app::posting;
use camcraft_app::tools::{catalog, torch_tool, ToolKind};
use camcraft_app::world::new_world;

const TORCH: &str = "PT-KERF-150";

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

fn plasma_op() -> PlasmaOp {
    PlasmaOp {
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        kerf: None,
        comp: PlasmaComp::Computer,
        reversed: false,
        lead_in: 4.0,
        lead_out: 3.0,
        small_hole_dia: 32.0,
        small_hole_feed: 0.65,
        overburn: 1.0,
        feed: Some(2500.0),
        pierce_height: None,
        cut_height: None,
        pierce_delay: None,
    }
}

/// A plasma document: the 1300PL mounted, the torch, the holed plate.
fn plasma_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, "Tormach 1300PL").expect("mounts"));
    h.execute(&world, AddTool::new(&world, TORCH, torch_tool(1.5)).expect("torch"));
    h.execute(&world, ImportModel::from_mesh("plate", holed_plate()));
    (world, h)
}

fn cut(world: &World, h: &mut History, params: OpParams) {
    h.execute(world, CreateOperation::new(world, "Plasma1", TORCH, SrcRef::Entity("plate".into()), params).expect("op"));
    h.execute(world, GenerateOperation::new(world, "Plasma1").expect("generates"));
}

fn path(world: &World) -> camcraft_app::cl::Toolpath {
    let e = model::find_by_name(world, "Plasma1").expect("op");
    model::toolpath_of(world, &e).expect("generated").path
}

#[test]
fn the_torch_is_a_tool_whose_diameter_is_the_kerf_and_turns_no_spindle() {
    let t = torch_tool(1.5);
    assert_eq!(t.kind, ToolKind::Torch);
    assert!(t.is_sane() && t.torch_refusal().is_none());
    assert_eq!(t.kind.to_string(), "plasma torch");
    let spun = camcraft_app::tools::Tool { rpm: 3000.0, ..t.clone() };
    assert!(!spun.is_sane());
    let world = new_world();
    let err = AddTool::new(&world, "T", spun).err().expect("an rpm on a torch refuses");
    assert!(format!("{err:?}").contains("plasma torch turns no spindle"), "{err:?}");
    // A spinning kind still needs its rpm.
    let em = catalog().remove(0).1;
    assert!(!camcraft_app::tools::Tool { rpm: 0.0, ..em }.is_sane());
    assert!(catalog().iter().any(|(n, t)| *n == TORCH && t.kind == ToolKind::Torch));
}

#[test]
fn the_plasma_family_lives_on_a_plasma_table_only() {
    assert_eq!(OpType::PlasmaProfile.info().family(), Family::Plasma);
    assert_eq!(OpType::PlasmaProfile.info().tool_kinds, &[ToolKind::Torch]);
    assert_eq!(operation::shown_for(Kind::Plasma), vec![Section::Plasma]);
    assert_eq!(Section::Plasma.kinds_for(Kind::Plasma), vec![OpType::PlasmaProfile.ordinal()]);
    for m in [Kind::Mill, Kind::Lathe, Kind::MillTurn] {
        assert!(!operation::shown_for(m).contains(&Section::Plasma), "{m:?}");
        assert!(!KindInfo::ALL[OpType::PlasmaProfile.ordinal()].admits(m), "{m:?}");
    }
    // At the door: a plasma op on a mill is refused by name.
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, AddTool::new(&world, TORCH, torch_tool(1.5)).expect("torch"));
    h.execute(&world, ImportModel::from_mesh("plate", holed_plate()));
    let err = CreateOperation::new(&world, "P", TORCH, SrcRef::Entity("plate".into()), OpParams::PlasmaProfile(plasma_op()))
        .err()
        .expect("refused on a mill");
    assert!(format!("{err:?}").contains("plasma cutting work") && format!("{err:?}").contains("mount a plasma table"), "{err:?}");
    // #1291: this world mounts no machine, so the words say so.
    assert!(format!("{err:?}").contains("the setup has no machine mounted - mount a plasma table first"), "{err:?}");
    // And on the plasma table, a milling tool is the tool law's refusal.
    let (world, mut h) = plasma_doc();
    let em = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").expect("em");
    h.execute(&world, AddTool::new(&world, em.0, em.1).expect("em"));
    assert!(CreateOperation::new(&world, "P", em.0, SrcRef::Entity("plate".into()), OpParams::PlasmaProfile(plasma_op())).is_err());
}

#[test]
fn a_holed_plate_cuts_its_hole_first_by_the_small_hole_rule() {
    let (world, mut h) = plasma_doc();
    cut(&world, &mut h, OpParams::PlasmaProfile(plasma_op()));
    let p = path(&world);
    let rec = p.plasma.as_ref().expect("the plasma record");
    assert_eq!(rec.kerf, 1.5);
    let roles: Vec<_> = rec.torches.iter().map(|t| (t.contour, t.role, t.parent)).collect();
    assert_eq!(roles, vec![(0, ContourRole::Inner, Some(1)), (1, ContourRole::Outer, None)]);
    // A 10x10 hole (Ø ≈ 11.3 equivalent) is under 32: centre lead-in,
    // no lead-out, 65 % feed.
    let hole = rec.torches[0];
    assert_eq!(hole.small, Some(0.65));
    let first = &p.moves[hole.on];
    assert!(first.from.x.abs() < 1e-6 && first.from.y.abs() < 1e-6, "pierced at the centre: {:?}", first.from);
    assert!(p.moves[hole.on..=hole.off].iter().all(|m| m.feed == Some(2500.0 * 0.65)));
    // The cut rides the plate top; rapids at the clearance plane.
    assert!(p.moves[hole.on..=hole.off].iter().all(|m| (m.to.z - 5.0).abs() < 1e-9));
    assert!(p.notes.iter().any(|n| n.contains("outer contours clockwise")), "{:?}", p.notes);
    // The machine check judges it on the 1300PL: plasma work, in reach.
    let (_, m) = machine::of_setup(&world, None);
    let c = check_document(&world, &m.expect("the 1300PL")).expect("checks");
    assert!(c.problems.is_empty(), "{:?}", c.problems);
    // The PathPilot plasma post serves the table (#671, plasma_post.rs).
    assert_eq!(posting::PLASMA_POSTS, &["Tormach PathPilot Plasma"]);
}

#[test]
fn a_hole_smaller_than_the_kerf_is_refused_by_name() {
    let (world, mut h) = plasma_doc();
    let op = PlasmaOp { kerf: Some(12.0), ..plasma_op() };
    h.execute(&world, CreateOperation::new(&world, "Plasma1", TORCH, SrcRef::Entity("plate".into()), OpParams::PlasmaProfile(op)).expect("op"));
    let err = GenerateOperation::new(&world, "Plasma1").err().expect("refused");
    let e = format!("{err:?}");
    assert!(e.contains("contour C0") && e.contains("smaller than the kerf"), "{e}");
    // Insane numbers never reach the document.
    let bad = PlasmaOp { small_hole_feed: 0.0, ..plasma_op() };
    assert!(CreateOperation::new(&world, "P2", TORCH, SrcRef::Entity("plate".into()), OpParams::PlasmaProfile(bad)).is_err());
    let bad = PlasmaOp { kerf: Some(-1.0), ..plasma_op() };
    assert!(CreateOperation::new(&world, "P3", TORCH, SrcRef::Entity("plate".into()), OpParams::PlasmaProfile(bad)).is_err());
}

#[test]
fn create_and_edit_undo_and_redo() {
    let (world, mut h) = plasma_doc();
    let p0 = OpParams::PlasmaProfile(plasma_op());
    h.execute(&world, CreateOperation::new(&world, "Plasma1", TORCH, SrcRef::Entity("plate".into()), p0.clone()).expect("op"));
    let p1 = OpParams::PlasmaProfile(PlasmaOp { reversed: true, comp: PlasmaComp::Control, ..plasma_op() });
    h.execute(&world, EditOperationParams::new(&world, "Plasma1", p1.clone()).expect("edit"));
    let params = |w: &World| model::find_by_name(w, "Plasma1").and_then(|e| model::operation_of(w, &e)).map(|o| o.params);
    assert_eq!(params(&world), Some(p1.clone()));
    assert!(h.undo(&world));
    assert_eq!(params(&world), Some(p0.clone()));
    assert!(h.undo(&world));
    assert_eq!(params(&world), None);
    assert!(h.redo(&world));
    assert_eq!(params(&world), Some(p0));
    assert!(h.redo(&world));
    assert_eq!(params(&world), Some(p1));
    // The reversed, control-compensated cut generates with its spans.
    h.execute(&world, GenerateOperation::new(&world, "Plasma1").expect("generates"));
    let p = path(&world);
    assert!(p.plasma.as_ref().is_some_and(|r| r.reversed && r.control_comp));
    assert!(p.notes.iter().any(|n| n.contains("REVERSED")), "{:?}", p.notes);
}

#[test]
fn the_op_the_torch_and_the_plasma_record_round_trip() {
    let (world, mut h) = plasma_doc();
    let op = PlasmaOp { pierce_height: Some(3.8), pierce_delay: Some(0.4), feed: None, ..plasma_op() };
    cut(&world, &mut h, OpParams::PlasmaProfile(op.clone()));
    let before = path(&world);
    let file = std::env::temp_dir().join(format!("camcraft_plasma_op_rt_{}.ccam", std::process::id()));
    camcraft_app::project::save(&world, &file).expect("save");
    let first = std::fs::read_to_string(&file).expect("read");
    let again = new_world();
    camcraft_app::project::load(&again, &file).expect("load");
    let e = model::find_by_name(&again, "Plasma1").expect("op");
    assert_eq!(model::operation_of(&again, &e).map(|o| o.params), Some(OpParams::PlasmaProfile(op)));
    let t = model::find_by_name(&again, TORCH).and_then(|t| model::tool_of(&again, &t)).expect("torch");
    assert_eq!(t, torch_tool(1.5));
    assert_eq!(path(&again).plasma, before.plasma, "the torch spans ride the path cache");
    camcraft_app::project::save(&again, &file).expect("resave");
    let second = std::fs::read_to_string(&file).expect("reread");
    let _ = std::fs::remove_file(&file);
    assert!(first.contains("\"plasma_profile\"") && first.contains("\"torch\""), "the words are written");
    assert_eq!(first, second, "byte-identical");
}

#[test]
fn a_picked_top_face_cuts_its_hole_and_outline_the_same_way() {
    let (world, mut h) = plasma_doc();
    let e = model::find_by_name(&world, "plate").expect("plate");
    let f = model::features_of(&world, &e).expect("features");
    let top = f.faces.iter().find(|fa| fa.normal[2] > 0.9 && (fa.offset - 5.0).abs() < 1e-3).map(|fa| fa.id).expect("top face");
    let src = SrcRef::Selection(vec![GeoSel { entity: "plate".into(), feature: FeatureSel::Face(top) }]);
    h.execute(&world, CreateOperation::new(&world, "Plasma1", TORCH, src, OpParams::PlasmaProfile(plasma_op())).expect("op"));
    h.execute(&world, GenerateOperation::new(&world, "Plasma1").expect("generates"));
    let picked = path(&world);
    let rec = picked.plasma.as_ref().expect("record");
    let roles: Vec<_> = rec.torches.iter().map(|t| (t.role, t.parent)).collect();
    assert_eq!(roles, vec![(ContourRole::Inner, Some(1)), (ContourRole::Outer, None)]);
    // The same plate cut whole agrees move for move.
    let (whole_world, mut h2) = plasma_doc();
    cut(&whole_world, &mut h2, OpParams::PlasmaProfile(plasma_op()));
    assert_eq!(path(&whole_world).plasma, picked.plasma);
}
