//! #1170 (part of #290): the 4-axis wire cut's stated verify tolerance,
//! end to end. A punch lofted from a square toward a round (a sync
//! pair at every vertex, so the wire's rulings are the model's own) is
//! tessellated with each band quad split along its outer diagonal, so
//! its facets stand past the ruled surface the pairing lays - by the
//! chord tolerance this file reads off the facets. With no verify tolerance stated the post is
//! blocked by a wire gouge that says what the facets may be; a stated
//! tolerance below the reading is refused by its name; one at least the
//! chord tolerance posts, and the report line says the tolerance and
//! the deepest reading it let pass. The tolerance is an undoable edit
//! that leaves the stored path valid; one that is no tolerance is
//! refused at the door, and the project file carries both new keys.
//! Not exercised on a real EDM control (#290 D10).
//!
//! Gate map (#1170): covers app::operation::wire_verify (verify_tol),
//! app::posting_wire (the tolerance on the report line, the facets
//! hint), app::operation::types::wire_edm (WireFourAxis door),
//! app::operation::wire_ruled (the widening in the stamp and the
//! generator) and app::project::wire_edm_dto (widening,
//! verify_tolerance).
//! Provenance: our own tests over our own synthetic fixture.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{
    self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef, WireContourOp, WireFourAxis, WirePairingRule, WireSide,
    WireSyncPair, WireWidening, VERIFY_TOLERANCE_MAX,
};
use camcraft_app::posting::{self, BUILTINS};
use camcraft_app::project;
use camcraft_app::world::new_world;

const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
const WIRE: &str = "Generic ISO Wire EDM";
const H: f64 = 12.0;
const N: usize = 64;
const ROUND: f64 = 0.07;

type P2 = (f64, f64);

/// A square of half-side `a` as N points, counter-clockwise from the
/// middle of its left side (`wire_4axis`'s fixture).
fn square(a: f64) -> Vec<P2> {
    (0..N)
        .map(|j| {
            let s = 8.0 * a * j as f64 / N as f64;
            match s {
                s if s <= a => (-a, -s),
                s if s <= 3.0 * a => (s - 2.0 * a, -a),
                s if s <= 5.0 * a => (a, s - 4.0 * a),
                s if s <= 7.0 * a => (6.0 * a - s, a),
                s => (-a, 8.0 * a - s),
            }
        })
        .collect()
}

fn circle(r: f64) -> Vec<P2> {
    (0..N)
        .map(|j| {
            let t = std::f64::consts::PI + std::f64::consts::TAU * j as f64 / N as f64;
            (r * t.cos(), r * t.sin())
        })
        .collect()
}

/// The punch's two profiles: a square below, and above it the same
/// square drawn `ROUND` of the way toward a circle - a gentle twist.
fn profiles() -> (Vec<P2>, Vec<P2>) {
    let (sq, rd) = (square(8.0), circle(9.6));
    let up = sq.iter().zip(&rd).map(|(s, r)| (s.0 + ROUND * (r.0 - s.0), s.1 + ROUND * (r.1 - s.1))).collect();
    (sq, up)
}

/// The 4-axis block: a sync pair at every vertex, as the model lofts.
fn block() -> WireFourAxis {
    let (lo, up) = profiles();
    let sync = lo.iter().zip(&up).map(|(l, u)| WireSyncPair { upper: Some([u.0, u.1]), lower: Some([l.0, l.1]) }).collect();
    WireFourAxis { pairing: WirePairingRule::SyncPoints, sync, ..WireFourAxis::default() }
}

fn at((x, y): P2, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// `lower` at z = 0 lofted to `upper` at z = H, point j to point j,
/// each band quad split along its OUTER diagonal: the facets stand past
/// the ruled surface. Returns the mesh and its chord tolerance - the
/// farthest a facet stands from the quad's own ruled patch (the
/// diagonal's middle against the patch's middle).
fn bulging_loft(lower: &[P2], upper: &[P2]) -> (Mesh, f64) {
    let mut f = Vec::new();
    let mut chord = 0.0f64;
    for j in 0..N {
        let k = (j + 1) % N;
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), at(lower[k], 0.0), at(lower[j], 0.0)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, H), at(upper[j], H), at(upper[k], H)));
        let (a, b, c, d) = (at(lower[j], 0.0), at(lower[k], 0.0), at(upper[k], H), at(upper[j], H));
        let mid = |p: Point3, q: Point3| ((p.x + q.x) * 0.5).hypot((p.y + q.y) * 0.5);
        if mid(a, c) >= mid(b, d) {
            f.push(Triangle::new(a, b, c));
            f.push(Triangle::new(a, c, d));
        } else {
            f.push(Triangle::new(a, b, d));
            f.push(Triangle::new(b, c, d));
        }
        chord = chord.max((mid(a, c) - mid(b, d)).abs() * 0.5);
    }
    (Mesh::from_facets(f), chord)
}

fn face_at(world: &World, z: f64) -> GeoSel {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .filter(|fa| fa.normal[2].abs() > 0.99)
        .filter(|fa| fa.loops.iter().flat_map(|l| &l.verts).any(|&v| (f.verts[v as usize][2] - z).abs() < 1e-9))
        .max_by(|a, b| a.area.total_cmp(&b.area))
        .map(|fa| fa.id)
        .unwrap_or_else(|| panic!("a horizontal face at Z{z}"));
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(id) }
}

fn wire_op(four: WireFourAxis) -> WireContourOp {
    WireContourOp {
        top_z: Height::fixed(H),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Punch,
        start: Some([-30.0, 0.0]),
        tabs: None,
        capture: true,
        taper_deg: 0.0,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        four_axis: Some(four),
        skims: Vec::new(),
    }
}

/// The fixture machine mounted, the bulging punch imported and its cut
/// generated with the 4-axis block `four`; the mesh's chord tolerance.
fn punch(four: WireFourAxis) -> (World, History, f64) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(parse_mch(FIXTURE).expect("parses"))).expect("mount"));
    let (lo, up) = profiles();
    let (mesh, chord) = bulging_loft(&lo, &up);
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    let src = SrcRef::Selection(vec![face_at(&world, H), face_at(&world, 0.0)]);
    h.execute(&world, CreateOperation::new(&world, "Punch1", "", src, OpParams::WireContour(wire_op(four))).expect("op"));
    h.execute(&world, GenerateOperation::new(&world, "Punch1").expect("generates"));
    (world, h, chord)
}

/// The block edited through the command history (undoable).
fn edit(world: &World, h: &mut History, four: WireFourAxis) {
    h.execute(world, EditOperationParams::new(world, "Punch1", OpParams::WireContour(wire_op(four))).expect("edit"));
}

/// The block edited and the cut generated again.
fn state(world: &World, h: &mut History, four: WireFourAxis) {
    edit(world, h, four);
    h.execute(world, GenerateOperation::new(world, "Punch1").expect("generates"));
    assert_eq!(status(world), OpStatus::Ok);
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let cps = BUILTINS.iter().find(|(n, _)| *n == WIRE).map(|(_, s)| *s).expect("registered");
    let out = std::env::temp_dir().join(format!("camcraft-wire-tol-{tag}-{}.nc", std::process::id()));
    let posted = posting::post_script(world, &out, cps);
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    assert_eq!(wrote, posted.is_ok(), "{tag}: NC text is written only for a posted program");
    posted.map(|p| p.program)
}

fn status(world: &World) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, "Punch1").expect("op"))
}

/// The deepest reading a gouge names: what a tolerance must cover.
fn depth(problem: &str) -> f64 {
    let tail = problem.split("the deepest reading at any height is ").nth(1).unwrap_or_else(|| panic!("a deepest reading: {problem}"));
    tail.split(' ').next().and_then(|d| d.parse().ok()).unwrap_or_else(|| panic!("a depth: {problem}"))
}

/// Done means: a lofted model tessellated at a stated chord tolerance
/// posts when the op states a verify tolerance at least that large and
/// is refused by name when not.
#[test]
fn a_tessellated_loft_posts_under_its_chord_tolerance_and_is_refused_below_it() {
    let (world, mut h, chord) = punch(block());
    assert!(chord > 0.005 && chord < 0.05, "the facets stand past the ruled surface: {chord}");
    // No tolerance stated: the facets are judged as they stand.
    let clause = posting::wire_clause(&world);
    let [gouge] = clause.problems.as_slice() else { panic!("one gouge: {:?}", clause.problems) };
    assert!(gouge.starts_with("Punch1: wire gouge: the kerf of move "), "{gouge}");
    assert!(gouge.contains("chord tolerance of the model") && gouge.contains("verify tolerance"), "the refusal says what to state: {gouge}");
    assert!(!gouge.contains("past the stated verify tolerance"), "{gouge}");
    let deep = depth(gouge);
    assert!(deep > 1e-3 && deep <= chord + 1e-9, "the reading is the facets' own bulge: {deep} of {chord}");
    let why = post(&world, "none").expect_err("the gouge blocks the post");
    assert!(why.contains("wire gouge"), "{why}");
    // A tolerance below the reading: refused by its name. The wire is
    // the same wire - the tolerance moves no move.
    let cl = |w: &World| model::toolpath_of(w, &model::find_by_name(w, "Punch1").expect("op")).expect("generated").path.moves;
    let before = cl(&world);
    state(&world, &mut h, WireFourAxis { verify_tolerance: deep * 0.25, ..block() });
    assert_eq!(cl(&world), before, "the verify tolerance moves no wire");
    let clause = posting::wire_clause(&world);
    let [gouge] = clause.problems.as_slice() else { panic!("one gouge: {:?}", clause.problems) };
    assert!(gouge.contains(&format!("past the stated verify tolerance {:.4} mm", deep * 0.25)), "{gouge}");
    assert!(!gouge.contains("chord tolerance of the model"), "a stated tolerance is not hinted at again: {gouge}");
    post(&world, "small").expect_err("refused");
    // The chord tolerance stated: posts, and the line says what it used.
    state(&world, &mut h, WireFourAxis { verify_tolerance: chord, ..block() });
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let line = clause.line.expect("the judged line");
    assert!(line.contains(&format!("kerf judged under the stated verify tolerance {chord:.4} mm")), "{line}");
    assert!(line.contains(&format!("enters the model's facets by at most {deep:.4} mm")), "{line}");
    post(&world, "stated").expect("the punch posts under its chord tolerance");
}

/// The widening is stated: the uniform choice is another wire (a stale
/// path), and both keys ride the project file; absent keys are the
/// per-ruling widening and no tolerance.
#[test]
fn the_widening_and_the_tolerance_are_stored_and_bounded_at_the_door() {
    let (world, mut h, _) = punch(block());
    assert_eq!(status(&world), OpStatus::Ok);
    edit(&world, &mut h, WireFourAxis { widening: WireWidening::Uniform, ..block() });
    assert_ne!(status(&world), OpStatus::Ok, "an edited widening is a stale path");
    assert!(h.undo(&world));
    assert_eq!(status(&world), OpStatus::Ok);
    for wild in [f64::NAN, -0.001, VERIFY_TOLERANCE_MAX + 0.001] {
        let four = WireFourAxis { verify_tolerance: wild, ..block() };
        assert!(EditOperationParams::new(&world, "Punch1", OpParams::WireContour(wire_op(four))).is_err(), "{wild} is refused at the door");
    }
    // A tolerance the door lets in and the kerf offset does not: the
    // verify refuses it by name (the fixture's kerf offset is 0.165).
    state(&world, &mut h, WireFourAxis { verify_tolerance: 0.2, ..block() });
    let said = posting::wire_clause(&world).problems.join("\n");
    assert!(said.contains("the verify tolerance 0.2 mm is not zero or a positive number below the kerf offset 0.1650 mm"), "{said}");
    // The project file.
    let plain = project::params_to_json(&OpParams::WireContour(wire_op(block())));
    let keys = &plain["wire_contour"]["four_axis"];
    assert!(keys.get("widening").is_none() && keys.get("verify_tolerance").is_none(), "{plain}");
    assert_eq!(project::params_from_json(&plain).expect("reads"), OpParams::WireContour(wire_op(block())));
    let stated = WireFourAxis { widening: WireWidening::Uniform, verify_tolerance: 0.0125, ..block() };
    let json = project::params_to_json(&OpParams::WireContour(wire_op(stated.clone())));
    let keys = &json["wire_contour"]["four_axis"];
    assert_eq!(keys["widening"], "uniform");
    assert_eq!(keys["verify_tolerance"], 0.0125);
    assert_eq!(project::params_from_json(&json).expect("reads"), OpParams::WireContour(wire_op(stated)));
}
