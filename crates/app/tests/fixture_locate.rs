//! #561: locate the part in an imported fixture (or the fixture on the
//! part) by mates - a face mate lays the part face on the fixture face,
//! normals opposed; floor, jaw and stop (3-2-1) fix it fully; the typed
//! nudge moves it exactly; `LocateFixture` moves the steel instead; every
//! locate is one undo unit, every bad pick is refused by name; a moved
//! part stales its setup's ops (undo un-stales) and a moved fixture is
//! re-judged by the live collision gate.
//! Provenance: our own fixtures over 3-2-1 fixturing practice
//! (machine-tool handbooks); no vendor material consulted.
//!
//! Gate map (#561): covers app::fixture_locate, app::pick::mate,
//! app::commands::locate (LocatePart, LocateFixture, Locate).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CommandError, CreateOperation, CreateSetup, GenerateOperation, History,
    ImportModel, Locate, LocateFixture, LocatePart, MoveFixture, MoveModel, SetStock, TagFixture,
};
use camcraft_app::fixture::{self, MACHINE_TABLE};
use camcraft_app::fixture_locate::{LocateOffset, LocateSpec, Mate, Mover};
use camcraft_app::geometry::{
    Mesh, Point3, Triangle, cube, feature, quat_from_euler, transform_inverse_point, transform_point,
};
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::pick::facets_centroid;
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::setup_align::{self, AlignPreset};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TIGHT: f64 = 1e-9;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A vise of three separate blocks: the floor (top at Z 0, facing +Z),
/// the fixed jaw (face at Y -1, facing +Y) and the stop (face at X -1,
/// facing +X).
fn vise() -> Vec<Triangle> {
    let mut t: Vec<Triangle> = cube(p(0.0, 0.0, -10.0), p(100.0, 60.0, 0.0)).facets.to_vec();
    t.extend(cube(p(0.0, -20.0, 0.5), p(100.0, -1.0, 40.0)).facets.iter());
    t.extend(cube(p(-20.0, 0.0, 0.5), p(-1.0, 60.0, 30.0)).facets.iter());
    t
}

/// The 20 x 10 x 8 part, placed tilted and turned far from the vise, and
/// the vise marked as a fixture.
fn document() -> (World, History) {
    placed(quat_from_euler(90f64.to_radians(), 0.0, 30f64.to_radians()))
}

/// [`document`] with the part turned by `rot`.
fn placed(rot: [f64; 4]) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(p(0.0, 0.0, 0.0), p(20.0, 10.0, 8.0))));
    let tilt = Location { pos: p(150.0, 80.0, 30.0), rot };
    h.execute(&world, MoveModel::new(&world, "part", tilt).expect("place the part"));
    h.execute(&world, ImportModel::from_mesh("vise", Mesh::from_facets(vise())));
    h.execute(&world, TagFixture::new(&world, "vise", None).expect("mark the vise"));
    (world, h)
}

/// The face of `name` whose facets centre on the LOCAL point `c`.
fn face(world: &World, name: &str, c: [f64; 3]) -> GeoSel {
    let e = model::find_by_name(world, name).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let tris = model::facets_of(world, &e).expect("facets");
    let id = f
        .faces
        .iter()
        .find(|f| {
            facets_centroid(&tris, f.facets.iter().map(|&i| i as usize))
                .is_some_and(|m| (0..3).all(|k| (m[k] - c[k]).abs() < 1e-6))
        })
        .map(|f| f.id)
        .unwrap_or_else(|| panic!("{name} has a face centred on {c:?}"));
    GeoSel { entity: name.into(), feature: FeatureSel::Face(id) }
}

/// The vertex of `name` at the LOCAL point `v`.
fn vertex(world: &World, name: &str, v: [f64; 3]) -> GeoSel {
    let e = model::find_by_name(world, name).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let vi = feature::corner_verts(&f).into_iter().find(|&i| f.verts[i as usize] == v).expect("a corner there");
    let id = feature::vertex_id(&f, vi).expect("vertex id");
    GeoSel { entity: name.into(), feature: FeatureSel::Vertex(id) }
}

fn floor(w: &World) -> Mate {
    Mate { part: face(w, "part", [10.0, 5.0, 0.0]), fixture: face(w, "vise", [50.0, 30.0, 0.0]) }
}
fn jaw(w: &World) -> Mate {
    Mate { part: face(w, "part", [10.0, 0.0, 4.0]), fixture: face(w, "vise", [50.0, -1.0, 20.25]) }
}
fn stop(w: &World) -> Mate {
    Mate { part: face(w, "part", [0.0, 5.0, 4.0]), fixture: face(w, "vise", [-1.0, 30.0, 15.25]) }
}

fn spec(mates: Vec<Mate>, offset: LocateOffset) -> LocateSpec {
    LocateSpec { part: "part".into(), fixture: "vise".into(), body: Default::default(), mates, offset }
}

/// The world box of an entity: (min, max).
fn bounds(world: &World, name: &str) -> ([f64; 3], [f64; 3]) {
    let e = model::find_by_name(world, name).expect("entity");
    let b = model::entity_world_aabb(world, &e).expect("bounds");
    ([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z])
}

fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
}

fn loc(world: &World, name: &str) -> Location {
    let e = model::find_by_name(world, name).expect("entity");
    model::transform_of(world, &e).unwrap_or_default()
}

fn refusal(r: Result<impl Sized, CommandError>) -> String {
    match r {
        Ok(_) => panic!("expected a refusal"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn a_face_mate_lays_the_part_face_on_the_fixture_face_normals_opposed() {
    let (world, mut h) = document();
    let c = LocatePart::new(&world, &spec(vec![floor(&world)], LocateOffset::default())).expect("one face mate");
    h.execute(&world, c);
    // The part's bottom face, read back in place: on Z 0, facing -Z.
    let e = model::find_by_name(&world, "part").expect("part");
    let tris = model::world_facets(&world, &e).expect("facets");
    let bottom: Vec<&Triangle> = tris
        .iter()
        .filter(|t| {
            let n = (t.b - t.a).cross(t.c - t.a);
            n.z < 0.0 && n.x.abs() < 1e-9 * n.z.abs().max(1.0) && n.y.abs() < 1e-9 * n.z.abs().max(1.0)
        })
        .collect();
    assert_eq!(bottom.len(), 2, "the bottom faces straight down (normals opposed)");
    for t in bottom {
        for v in [t.a, t.b, t.c] {
            assert!(v.z.abs() < TIGHT, "coplanar with the floor at Z 0: {v:?}");
        }
    }
    let (lo, _) = bounds(&world, "part");
    assert!(lo[2].abs() < TIGHT, "nothing below the floor");
}

#[test]
fn floor_jaw_and_stop_fully_locate_the_part() {
    let (world, mut h) = document();
    let mates = vec![floor(&world), jaw(&world), stop(&world)];
    h.execute(&world, LocatePart::new(&world, &spec(mates, LocateOffset::default())).expect("3-2-1"));
    let (lo, hi) = bounds(&world, "part");
    assert!(close(lo, [-1.0, -1.0, 0.0], TIGHT), "against the stop, the jaw and the floor: {lo:?}");
    assert!(close(hi, [19.0, 9.0, 8.0], TIGHT), "square in the vise: {hi:?}");
    let l = loc(&world, "part");
    assert!(setup_align::is_identity_rot(l.rot), "turned back square: {:?}", l.rot);
}

#[test]
fn the_typed_offset_moves_the_part_exactly() {
    let (world, mut h) = document();
    let offset = LocateOffset { dx: 10.0, dy: 2.5, dz: 5.0, turn_deg: 0.0 };
    let mates = vec![floor(&world), jaw(&world), stop(&world)];
    h.execute(&world, LocatePart::new(&world, &spec(mates.clone(), offset)).expect("nudged"));
    let (lo, hi) = bounds(&world, "part");
    assert!(close(lo, [9.0, 1.5, 5.0], TIGHT), "10 in from the stop, 2.5 off the jaw, 5 above the floor: {lo:?}");
    assert!(close(hi, [29.0, 11.5, 13.0], TIGHT), "{hi:?}");
    // A quarter turn about the floor's normal, through its centre.
    h.undo(&world);
    let turned = LocateOffset { turn_deg: 90.0, ..LocateOffset::default() };
    h.execute(&world, LocatePart::new(&world, &spec(mates, turned)).expect("turned"));
    let (lo, hi) = bounds(&world, "part");
    // (x, y) -> (50 + 30 - y, -50 + 30 + x) about (50, 30): X span 10, Y span 20.
    assert!(close(lo, [71.0, -21.0, 0.0], 1e-9), "{lo:?}");
    assert!(close(hi, [81.0, -1.0, 8.0], 1e-9), "{hi:?}");
}

#[test]
fn locate_fixture_moves_the_fixture_not_the_part() {
    let (world, mut h) = document();
    let part_before = loc(&world, "part");
    let mates = vec![floor(&world), jaw(&world), stop(&world)];
    let c = LocateFixture::new(&world, &spec(mates, LocateOffset::default())).expect("the vise comes");
    h.execute(&world, c);
    assert_eq!(loc(&world, "part"), part_before, "the part keeps its place");
    // The same relative placement as a part locate: the part's corner at
    // the vise's (-1, -1, 0), square in it.
    let v = loc(&world, "vise");
    let corner = transform_point(p(0.0, 0.0, 0.0), part_before.pos, part_before.rot);
    let seen = transform_inverse_point(corner, v.pos, v.rot);
    assert!(close([seen.x, seen.y, seen.z], [-1.0, -1.0, 0.0], 1e-9), "{seen:?}");
    // Undo puts the vise back, redo brings it again.
    let moved = loc(&world, "vise");
    h.undo(&world);
    assert_eq!(loc(&world, "vise"), Location::default());
    h.redo(&world);
    assert_eq!(loc(&world, "vise"), moved);
    // And it really sits around the part: locating the part now is a no-op.
    let again = spec(vec![floor(&world), jaw(&world), stop(&world)], LocateOffset::default());
    assert!(refusal(LocatePart::new(&world, &again)).contains("nothing would move"));
}

#[test]
fn undo_and_redo_walk_a_part_locate() {
    let (world, mut h) = document();
    let before = loc(&world, "part");
    let mates = vec![floor(&world), jaw(&world)];
    h.execute(&world, Locate::new(&world, &spec(mates, LocateOffset::default()), Mover::Part).expect("two mates"));
    let after = loc(&world, "part");
    assert_ne!(after, before);
    h.undo(&world);
    assert_eq!(loc(&world, "part"), before);
    h.redo(&world);
    assert_eq!(loc(&world, "part"), after);
}

#[test]
fn point_mates_land_the_corner_on_the_corner() {
    let (world, mut h) = document();
    // Floor first, then the part's corner onto the floor's corner.
    let corner = Mate { part: vertex(&world, "part", [0.0, 0.0, 0.0]), fixture: vertex(&world, "vise", [0.0, 0.0, 0.0]) };
    h.execute(&world, LocatePart::new(&world, &spec(vec![floor(&world), corner], LocateOffset::default())).expect("face + point"));
    let e = model::find_by_name(&world, "part").expect("part");
    let l = model::transform_of(&world, &e).unwrap_or_default();
    assert!(close([l.pos.x, l.pos.y, l.pos.z], [0.0, 0.0, 0.0], TIGHT), "the corner sits on the corner: {:?}", l.pos);
}

#[test]
fn degenerate_and_conflicting_picks_are_refused_by_name() {
    let (world, _h) = document();
    let none = LocateOffset::default();
    // An edge is no mate.
    let e = model::find_by_name(&world, "part").expect("part");
    let f = model::features_of(&world, &e).expect("features");
    let chain = feature::chains(&f).first().map(|c| c.id).expect("an edge");
    let edge = Mate { part: GeoSel { entity: "part".into(), feature: FeatureSel::Chain(chain) }, fixture: floor(&world).fixture };
    assert!(refusal(LocatePart::new(&world, &spec(vec![edge], none))).contains("is an edge"));
    // The same part face twice: coincident.
    let twice = Mate { part: floor(&world).part, fixture: jaw(&world).fixture };
    assert!(refusal(LocatePart::new(&world, &spec(vec![floor(&world), twice], none))).contains("coincident"));
    // The same corner twice: coincident points.
    let pt = Mate { part: vertex(&world, "part", [0.0, 0.0, 0.0]), fixture: vertex(&world, "vise", [0.0, 0.0, 0.0]) };
    let pt2 = Mate { part: vertex(&world, "part", [0.0, 0.0, 0.0]), fixture: vertex(&world, "vise", [100.0, 0.0, 0.0]) };
    assert!(refusal(LocatePart::new(&world, &spec(vec![pt, pt2], none))).contains("coincident"));
    // The part's top cannot sit on the jaw while its bottom is on the floor.
    let top = Mate { part: face(&world, "part", [10.0, 5.0, 8.0]), fixture: jaw(&world).fixture };
    assert!(refusal(LocatePart::new(&world, &spec(vec![floor(&world), top], none))).contains("cannot meet face to face"));
    // A non-finite nudge.
    let nan = LocateOffset { dz: f64::NAN, ..none };
    assert!(refusal(LocatePart::new(&world, &spec(vec![floor(&world)], nan))).contains("finite"));
    // No mates, too many mates.
    assert!(refusal(LocatePart::new(&world, &spec(vec![], none))).contains("at least one mate"));
    let four = vec![floor(&world), jaw(&world), stop(&world), floor(&world)];
    assert!(refusal(LocatePart::new(&world, &spec(four, none))).contains("three mates"));
    // No fixture, the table, no part, a pick on the wrong body.
    let mut s = spec(vec![floor(&world)], none);
    s.fixture = "part".into();
    assert!(refusal(LocatePart::new(&world, &s)).contains("is not a fixture"));
    s.fixture = MACHINE_TABLE.into();
    assert!(refusal(LocateFixture::new(&world, &s)).contains("machine table"));
    s.fixture = "vise".into();
    s.part = "ghost".into();
    assert!(refusal(LocatePart::new(&world, &s)).contains("no model named 'ghost'"));
    let swapped = Mate { part: floor(&world).fixture, fixture: floor(&world).part };
    assert!(refusal(LocatePart::new(&world, &spec(vec![swapped], none))).contains("is not on the part"));
}

#[test]
fn an_offset_into_the_floor_warns_by_name() {
    let (world, _h) = document();
    let sunk = LocateOffset { dz: -3.0, ..LocateOffset::default() };
    let c = LocatePart::new(&world, &spec(vec![floor(&world), jaw(&world), stop(&world)], sunk)).expect("built");
    let w = c.warning().expect("the part sinks into the floor");
    assert!(w.contains("'part' would sit inside the fixture 'vise'"), "{w}");
    let clear = LocatePart::new(&world, &spec(vec![floor(&world), jaw(&world), stop(&world)], LocateOffset::default()))
        .expect("built");
    assert_eq!(clear.warning(), None, "touching faces are not interpenetration");
}

/// The part with its stock and a generated face op in Setup1.
fn machining_document() -> (World, History) {
    let (world, mut h) = placed([0.0, 0.0, 0.0, 1.0]);
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let b = bounds(&world, "part");
    let shape = StockShape::Box { dx: b.1[0] - b.0[0], dy: b.1[1] - b.0[1], dz: b.1[2] - b.0[2] };
    let at = Location { pos: p(0.5 * (b.0[0] + b.1[0]), 0.5 * (b.0[1] + b.1[1]), b.0[2]), ..Location::default() };
    h.execute(&world, SetStock::new(&world, shape, at).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    let top = b.1[2];
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(top + 25.0),
        cut_z: Height::fixed(top - 0.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("op"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generated"));
    (world, h)
}

fn status(world: &World, op: &str) -> OpStatus {
    let e = model::find_by_name(world, op).expect("op");
    operation::status_of(world, &e)
}

#[test]
fn a_located_part_stales_its_setup_ops_and_undo_unstales_them() {
    let (world, mut h) = machining_document();
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    let stock = camcraft_app::setup::stock_entity_name(&world, Some("Setup1"));
    let stock_before = loc(&world, &stock);
    h.execute(&world, LocatePart::new(&world, &spec(vec![floor(&world), jaw(&world), stop(&world)], LocateOffset::default())).expect("located"));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale, "the part moved: regenerate");
    assert_ne!(loc(&world, &stock), stock_before, "the stock follows the part");
    let (lo, _) = bounds(&world, &stock);
    assert!(close(lo, [-1.0, -1.0, 0.0], 1e-9), "the stock sits in the vise with the part: {lo:?}");
    h.undo(&world);
    assert_eq!(status(&world, "Face1"), OpStatus::Ok, "undo restores what the op read");
    assert_eq!(loc(&world, &stock), stock_before);
}

#[test]
fn a_located_fixture_is_rejudged_live_and_never_stales_the_path() {
    let (world, mut h) = machining_document();
    // Clear of the part first: the gate finds nothing.
    let far = Location { pos: p(500.0, 500.0, 0.0), ..Location::default() };
    h.execute(&world, MoveFixture::new(&world, "vise", far).expect("far away"));
    let found = posting::fixture_findings(&world).expect("judged");
    assert!(found.hits.is_empty(), "nothing near: {:?}", found.hits);
    let boxes_before = fixture::boxes_for(&world, None);
    // The vise comes to the part with its jaw standing above the stock
    // top - the facing pass now runs into it.
    let mates = vec![floor(&world), jaw(&world), stop(&world)];
    h.execute(&world, LocateFixture::new(&world, &spec(mates, LocateOffset::default())).expect("the vise comes"));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok, "steel never stales a toolpath (the live-gate law)");
    assert_ne!(fixture::boxes_for(&world, None), boxes_before, "the gate reads the new placement");
    let found = posting::fixture_findings(&world).expect("judged");
    assert!(!found.hits.is_empty(), "the jaw beside the part is struck by the facing pass");
    h.undo(&world);
    assert!(posting::fixture_findings(&world).expect("judged").hits.is_empty(), "undo: clear again");
}

#[test]
fn in_a_turned_setup_the_locate_composes_into_its_alignment() {
    let (world, mut h) = document();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup1", AlignPreset::TurnZ90).expect("turned"));
    let doc_before = loc(&world, "part");
    let mates = vec![floor(&world), jaw(&world), stop(&world)];
    h.execute(&world, LocatePart::new(&world, &spec(mates, LocateOffset::default())).expect("located"));
    assert_eq!(loc(&world, "part"), doc_before, "the document placement is the other setups' - untouched");
    let e = model::find_by_name(&world, "part").expect("part");
    let b = setup_align::entity_aabb_in(&world, &e, Some("Setup1")).expect("bounds in Setup1");
    assert!(close([b.min.x, b.min.y, b.min.z], [-1.0, -1.0, 0.0], 1e-9), "{:?}", b.min);
    assert!(close([b.max.x, b.max.y, b.max.z], [19.0, 9.0, 8.0], 1e-9), "{:?}", b.max);
    h.undo(&world);
    let back = setup_align::stored(&world, "Setup1").expect("still turned");
    assert!(setup_align::preset_of(back.rot) == Some(AlignPreset::TurnZ90));
}
