//! #672 step 2: the plasma laws at the post gate. A generated Plasma
//! Profile is judged from its own CL records and the report SAYS the 3D
//! stock sim did not run for it (verdict = the 2D laws); a program the
//! laws refuse - an outline cut before its hole, a record whose PARENT
//! the rings disagree with - blocks the post by name; a plasma op with
//! no path is a named problem, never a silence. #801: an inch document
//! is judged in mm and says where in inches; a G41/G42 program is judged
//! on the torch centre it rides, the same rings as the computer offset.
//! Provenance: our own fixtures (the #670 suite's holed plate); the laws
//! cite UM10720 and the LinuxCNC Plasma Primer in `sim::verify::plasma`.
//!
//! Gate map (#672): covers app::posting, app::posting_plasma,
//! app::machine, post::machine::plasma, app::operation (plasma_op), sim::verify::plasma.

use flecs_ecs::prelude::World;

use camcraft_app::cl::{ContourRole, MoveKind, PathBuilder, PlasmaCut as Record, Toolpath, TorchSpan};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetUnits};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, PlasmaComp, PlasmaOp, SrcRef};
use camcraft_app::posting;
use camcraft_app::tools::torch_tool;
use camcraft_app::units::{self, Unit};
use camcraft_app::verify::plasma::{judge, PlasmaFinding, PlasmaProgram};
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

fn plasma_op() -> OpParams {
    plasma_op_with(PlasmaComp::Computer)
}

fn plasma_op_with(comp: PlasmaComp) -> OpParams {
    OpParams::PlasmaProfile(PlasmaOp {
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        kerf: None,
        comp,
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
    })
}

/// The 1300PL, the torch, the holed plate, and Plasma1 - generated or not.
fn plasma_doc(generate: bool) -> World {
    plasma_doc_with(generate, plasma_op())
}

fn plasma_doc_with(generate: bool, op: OpParams) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, "Tormach 1300PL").expect("mounts"));
    h.execute(&world, AddTool::new(&world, TORCH, torch_tool(1.5)).expect("torch"));
    h.execute(&world, ImportModel::from_mesh("plate", holed_plate()));
    h.execute(&world, CreateOperation::new(&world, "Plasma1", TORCH, SrcRef::Entity("plate".into()), op).expect("op"));
    if generate {
        h.execute(&world, GenerateOperation::new(&world, "Plasma1").expect("generates"));
    }
    world
}

fn cache_of(world: &World) -> camcraft_app::schema::Toolpath {
    let e = model::find_by_name(world, "Plasma1").expect("op");
    model::toolpath_of(world, &e).expect("generated")
}

/// Swap Plasma1's cached path for `path` (the stamp digests the inputs,
/// not the cache, so the op stays fresh - the gate reads the cache).
fn recache(world: &World, path: Toolpath) {
    let e = model::find_by_name(world, "Plasma1").expect("op");
    e.entity_view(world).set(camcraft_app::schema::Toolpath::new(path, Vec::new()));
    assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok, "fixture: still fresh");
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-plasma-gate-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

/// The post's answer, as the text the user reads - written or refused.
fn post(world: &World, tag: &str) -> Result<posting::Posted, String> {
    let path = tmp_nc(tag);
    let r = posting::post_setup(world, &path);
    let _ = std::fs::remove_file(&path);
    r
}

#[test]
fn a_generated_plasma_program_is_judged_by_the_2d_laws_and_says_so() {
    let world = plasma_doc(true);
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    let line = report.not_judged.clone().expect("the skipped 3D sim is said, never silent");
    assert!(line.starts_with(posting::NOT_JUDGED), "{line}");
    assert!(line.contains(posting::PLASMA_VERDICT) && line.contains("Plasma1") && line.contains("2D plasma laws"), "{line}");
    assert!(line.ends_with("clean"), "{line}");
    assert_eq!(posting::verification_gate(&world), report.problems, "one gate, two readings");
    let clause = posting::plasma_clause(&world);
    assert!(clause.problems.is_empty() && clause.line.is_some_and(|l| l.contains("Plasma1")));
    // The post (when a plasma post serves the table) carries the line;
    // whatever else refuses it, the plasma laws do not.
    match post(&world, "clean") {
        Ok(p) => assert_eq!(p.not_judged.as_deref(), Some(line.as_str())),
        Err(e) => assert!(!e.contains("plasma cut order") && !e.contains("plasma laws"), "{e}"),
    }
}

/// Plasma1's path with its two cuts laid down in the other order - the
/// outline first, then the hole - and the record saying so (sane: the
/// spans sit on the moves, the parent exists).
fn outline_first(world: &World) -> Toolpath {
    let cache = cache_of(world);
    let rec = cache.path.plasma.as_ref().expect("record");
    let (hole, outline) = (rec.torches[0], rec.torches[1]);
    assert!((hole.role, outline.role) == (ContourRole::Inner, ContourRole::Outer), "fixture: {:?}", rec.torches);
    let moves = &cache.path.moves;
    let mut b = PathBuilder::new(Point3::new(moves[0].from.x, moves[0].from.y, 25.0));
    let mut torches = Vec::new();
    for sp in [outline, hole] {
        let pierce = moves[sp.on].from;
        b.push(MoveKind::Rapid, Point3::new(b.at().x, b.at().y, 25.0), None);
        b.push(MoveKind::Rapid, Point3::new(pierce.x, pierce.y, 25.0), None);
        b.push(MoveKind::Rapid, pierce, None);
        let on = b.moves_len();
        for m in &moves[sp.on..=sp.off] {
            match m.arc {
                Some(a) => b.push_arc(MoveKind::Feed, m.to, m.feed, a.center, a.ccw),
                None => b.push(MoveKind::Feed, m.to, m.feed),
            }
        }
        torches.push(TorchSpan { on, off: b.moves_len() - 1, ..sp });
    }
    b.push(MoveKind::Rapid, Point3::new(b.at().x, b.at().y, 25.0), None);
    let mut t = b.finish();
    let swapped = Record { torches, ..rec.clone() };
    assert!(swapped.is_sane(&t.moves), "fixture: a sane record");
    t.plasma = Some(swapped);
    t
}

#[test]
fn an_outline_cut_before_its_hole_blocks_the_post_by_name() {
    let world = plasma_doc(true);
    recache(&world, outline_first(&world));
    let problems = posting::verification_gate(&world);
    let order = problems.iter().find(|p| p.contains("plasma cut order")).unwrap_or_else(|| panic!("{problems:?}"));
    assert!(order.starts_with("Plasma1: ") && order.contains("contour C0 is cut after contour C1"), "{order}");
    let e = post(&world, "order").expect_err("the post is blocked");
    assert!(e.contains("post blocked") && e.contains("plasma cut order") && e.contains("C0 is cut after contour C1"), "{e}");
}

#[test]
fn a_parent_the_rings_disagree_with_blocks_the_post_by_name() {
    let world = plasma_doc(true);
    let mut cache = cache_of(&world);
    // The outline (top level) made to name the hole as its parent: a
    // sane record (the id exists), a lie the rings catch.
    let rec = cache.path.plasma.as_mut().expect("record");
    rec.torches[1].parent = Some(rec.torches[0].contour);
    assert!(rec.is_sane(&cache.path.moves));
    recache(&world, cache.path);
    let problems = posting::verification_gate(&world);
    let lie = problems.iter().find(|p| p.contains("malformed program")).unwrap_or_else(|| panic!("{problems:?}"));
    assert!(lie.contains("no cut encloses it") && lie.contains("disagree"), "{lie}");
    let e = post(&world, "parent").expect_err("the post is blocked");
    assert!(e.contains("post blocked") && e.contains("disagree"), "{e}");
}

#[test]
fn a_plasma_op_with_no_path_is_a_named_problem_not_a_silence() {
    let world = plasma_doc(false);
    let problems = posting::verification_gate(&world);
    assert!(problems.iter().any(|p| p.starts_with("Plasma1: the plasma laws have no toolpath")), "{problems:?}");
    let report = posting::verification_report(&world);
    assert!(report.not_judged.is_some_and(|l| l.contains("Plasma1")), "the line still names the op");
    // A path that lost its record is refused the same way, by name.
    let world = plasma_doc(true);
    let mut cache = cache_of(&world);
    cache.path.plasma = None;
    recache(&world, cache.path);
    let problems = posting::verification_gate(&world);
    assert!(problems.iter().any(|p| p.contains("Plasma1: the plasma laws cannot judge the path") && p.contains("no plasma record")), "{problems:?}");
}

/// Two parts, the second laid across the first (0..20, then 10..30 x
/// 5..25): the later ring crosses the earlier one - a refusal with a
/// place. Sane record, straight lit moves.
fn crossing_parts() -> Toolpath {
    let parts: [(u32, &[(f64, f64)]); 2] = [
        (0, &[(-3.0, 10.0), (0.0, 10.0), (0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0), (0.0, 10.0), (-3.0, 13.0)]),
        (1, &[(33.0, 15.0), (30.0, 15.0), (30.0, 25.0), (10.0, 25.0), (10.0, 5.0), (30.0, 5.0), (30.0, 15.0), (33.0, 12.0)]),
    ];
    let mut b = PathBuilder::new(Point3::new(-3.0, 10.0, 25.0));
    let mut torches = Vec::new();
    for (contour, pts) in parts {
        let (px, py) = pts[0];
        b.push(MoveKind::Rapid, Point3::new(b.at().x, b.at().y, 25.0), None);
        b.push(MoveKind::Rapid, Point3::new(px, py, 25.0), None);
        b.push(MoveKind::Rapid, Point3::new(px, py, 0.0), None);
        let on = b.moves_len();
        for &(x, y) in &pts[1..] {
            b.push(MoveKind::Feed, Point3::new(x, y, 0.0), Some(2500.0));
        }
        torches.push(TorchSpan { contour, role: ContourRole::Outer, parent: None, on, off: b.moves_len() - 1, small: None });
    }
    b.push(MoveKind::Rapid, Point3::new(b.at().x, b.at().y, 25.0), None);
    let mut t = b.finish();
    let rec = Record { kerf: 1.5, torches, ..Record::default() };
    assert!(rec.is_sane(&t.moves), "fixture: a sane record");
    t.plasma = Some(rec);
    t
}

/// The crossing's place, as the laws find it (mm).
fn crossing_at(t: &Toolpath) -> camcraft_app::geo2d::Point2 {
    let f = judge(&PlasmaProgram::from_toolpath(t).expect("read"));
    f.iter()
        .find_map(|x| match x {
            PlasmaFinding::CutCrossing { at, .. } => Some(*at),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fixture: a cut crossing - {f:?}"))
}

/// #801: the laws run in mm on the torch-centre moves; an inch document
/// changes only how a finding says where. A generated program reads
/// clean in inches as in mm, and a refusal blocks the post by name with
/// its place in inches (the same refusal in a mm document reads mm).
#[test]
fn an_inch_document_is_judged_the_same_and_placed_in_inches() {
    let world = plasma_doc(true);
    let mut h = History::default();
    h.execute(&world, SetUnits::new(&world, Unit::Inches).expect("to inches"));
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    let line = report.not_judged.expect("the not-judged line");
    assert!(line.contains(posting::PLASMA_VERDICT) && line.contains("Plasma1") && line.ends_with("clean"), "{line}");
    let parts = crossing_parts();
    let at = crossing_at(&parts);
    recache(&world, parts.clone());
    let problems = posting::verification_gate(&world);
    let cross = problems.iter().find(|p| p.contains("plasma cut crossing")).unwrap_or_else(|| panic!("{problems:?}"));
    let inches = format!("near ({}, {}) in", units::len(at.x, Unit::Inches, 2), units::len(at.y, Unit::Inches, 2));
    assert!(cross.starts_with("Plasma1: ") && cross.contains("contour C1's cut crosses contour C0's") && cross.contains(&inches), "{cross} / {inches}");
    let e = post(&world, "inch").expect_err("the post is blocked");
    assert!(e.contains("post blocked") && e.contains(&inches), "{e}");
    // The same program in a mm document: the same refusal, placed in mm.
    let mm = plasma_doc(true);
    recache(&mm, parts);
    let problems = posting::verification_gate(&mm);
    let millimetres = format!("near ({}, {}) mm", units::len(at.x, Unit::Millimeters, 2), units::len(at.y, Unit::Millimeters, 2));
    assert!(problems.iter().any(|p| p.contains("plasma cut crossing") && p.contains(&millimetres)), "{problems:?}");
}

/// #801: a G41/G42 program carries the programmed part contour in its
/// compensation spans, but its moves are the torch centre, and that is
/// what the laws read - the same rings as the computer-offset program.
/// It is judged clean, and the same cuts laid outline-first are refused.
#[test]
fn a_g41_g42_program_is_judged_on_the_torch_centre() {
    let world = plasma_doc_with(true, plasma_op_with(PlasmaComp::Control));
    let cache = cache_of(&world);
    assert!(cache.path.plasma.as_ref().is_some_and(|r| r.control_comp), "fixture: compensated in the control");
    assert!(!cache.path.comps.is_empty(), "fixture: a G41/G42 span");
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    let line = report.not_judged.expect("the not-judged line");
    assert!(line.contains("Plasma1") && line.ends_with("clean"), "{line}");
    let control = PlasmaProgram::from_toolpath(&cache.path).expect("read");
    let computer = PlasmaProgram::from_toolpath(&cache_of(&plasma_doc(true)).path).expect("read");
    assert_eq!(control.cuts.len(), computer.cuts.len());
    for (a, b) in control.cuts.iter().zip(&computer.cuts) {
        let (ra, rb) = (a.ring().expect("closed"), b.ring().expect("closed"));
        assert_eq!(ra.len(), rb.len(), "C{}: the same ring", a.contour);
        assert!(ra.iter().zip(rb).all(|(p, q)| (p.x - q.x).hypot(p.y - q.y) < 1e-6), "C{}: the torch centre, not the part", a.contour);
    }
    recache(&world, outline_first(&world));
    let problems = posting::verification_gate(&world);
    assert!(problems.iter().any(|p| p.contains("plasma cut order") && p.contains("C0 is cut after contour C1")), "{problems:?}");
}
