//! #339 slice 4 (fixture UX), the app side: each fixture's mesh state
//! (closed mesh, open, non-finite, shared box) as the browser reads it,
//! the import-as-fixture command as ONE undo unit, the mark/unmark
//! commands the browser menu queues, and the struck-fixture resolution
//! behind the viewport tint (a hit no single fixture reproduces is
//! reported unresolved, never guessed).
//!
//! Gate map (M4.4): covers app::fixture_view, app::fixture_struck,
//! app::commands::import_fixture, app::commands::fixture, app::posting
//! (fixture_findings: each op judged on its own setup's spindle, #388).

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportFixture, ImportModel, MoveFixture,
    SetSetupMachine, SetStock, TagFixture, UntagFixture,
};
use camcraft_app::fixture;
use camcraft_app::fixture_struck;
use camcraft_app::fixture_view::{self, BoxReason, MeshError, MeshState, MeshStateCache};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A closed, outward box: 12 facets on 8 exact corners.
fn cube(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let c = |i: usize| {
        p(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z })
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads
        .iter()
        .flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))])
        .collect()
}

fn plant(world: &World, history: &mut History, name: &str, tris: Vec<Triangle>) {
    let e = model::spawn(world, name);
    model::set_facets(world, &e, tris.into());
    model::set_rev(world, &e, 1);
    history.execute(world, TagFixture::new(world, name, None).expect("tagged"));
}

fn state_of(world: &World, name: &str) -> Option<MeshState> {
    fixture_view::mesh_states(world, None).into_iter().find(|(n, _)| n == name).map(|(_, s)| s)
}

#[test]
fn a_closed_jaw_is_judged_by_mesh_and_an_open_or_nan_one_by_box() {
    let world = new_world();
    let mut history = History::default();
    plant(&world, &mut history, "Closed", cube(p(0.0, 0.0, 0.0), p(10.0, 5.0, 20.0)));
    let mut open = cube(p(20.0, 0.0, 0.0), p(30.0, 5.0, 20.0));
    open.truncate(open.len() - 1); // one facet missing: no inside
    plant(&world, &mut history, "Open", open);
    let mut nan = cube(p(40.0, 0.0, 0.0), p(50.0, 5.0, 20.0));
    nan[0] = Triangle::new(p(f64::NAN, 0.0, 0.0), nan[0].b, nan[0].c);
    plant(&world, &mut history, "NaN", nan);

    assert_eq!(state_of(&world, "Closed"), Some(MeshState::Mesh));
    assert_eq!(state_of(&world, "Open"), Some(MeshState::BoxOnly(BoxReason::Refused(MeshError::Open))));
    assert_eq!(state_of(&world, "NaN"), Some(MeshState::BoxOnly(BoxReason::Refused(MeshError::NonFinite))));
    // The row and page wording.
    assert_eq!(MeshState::Mesh.label(), "mesh");
    assert_eq!(MeshState::BoxOnly(BoxReason::Refused(MeshError::Open)).label(), "box only: open mesh");
    assert_eq!(MeshState::BoxOnly(BoxReason::Refused(MeshError::NonFinite)).label(), "box only: non-finite");
    assert!(MeshState::BoxOnly(BoxReason::Refused(MeshError::Open)).detail().contains("not closed"));
    // A plain model is no fixture: no row.
    let e = model::spawn(&world, "Part");
    model::set_facets(&world, &e, cube(p(0.0, 0.0, 0.0), p(1.0, 1.0, 1.0)).into());
    assert_eq!(state_of(&world, "Part"), None);
}

#[test]
fn two_fixtures_on_one_box_are_both_box_only() {
    let world = new_world();
    let mut history = History::default();
    plant(&world, &mut history, "A", cube(p(0.0, 0.0, 0.0), p(10.0, 10.0, 10.0)));
    plant(&world, &mut history, "B", cube(p(0.0, 0.0, 0.0), p(10.0, 10.0, 10.0)));
    assert_eq!(state_of(&world, "A"), Some(MeshState::BoxOnly(BoxReason::SharedBox)));
    assert_eq!(state_of(&world, "B").map(|s| s.label()), Some("box only: shared box".to_owned()));
}

#[test]
fn the_mesh_state_cache_recomputes_only_when_the_version_moves() {
    let world = new_world();
    let mut history = History::default();
    plant(&world, &mut history, "Jaw", cube(p(0.0, 0.0, 0.0), p(10.0, 5.0, 20.0)));
    let cache = MeshStateCache::default();
    let v1 = fixture_view::mesh_states_cached(&cache, &world, 1);
    assert_eq!(v1.get("Jaw"), Some(&MeshState::Mesh));
    history.execute(&world, UntagFixture::new(&world, "Jaw").expect("untag"));
    // Same version: the cached answer stands (the view's contract).
    assert!(fixture_view::mesh_states_cached(&cache, &world, 1).contains_key("Jaw"));
    // The version moved: re-read.
    assert!(!fixture_view::mesh_states_cached(&cache, &world, 2).contains_key("Jaw"));
}

fn is_fixture(world: &World, name: &str) -> bool {
    model::find_by_name(world, name).is_some_and(|e| fixture::def_of(world, &e).is_some())
}

#[test]
fn mark_and_unmark_are_undoable_commands() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("Vise", Mesh::from_facets(cube(p(0.0, 0.0, 0.0), p(5.0, 5.0, 5.0)))));
    history.execute(&world, TagFixture::new(&world, "Vise", None).expect("mark"));
    assert!(is_fixture(&world, "Vise"));
    history.undo(&world);
    assert!(!is_fixture(&world, "Vise"), "undo unmarks");
    history.redo(&world);
    assert!(is_fixture(&world, "Vise"), "redo marks again");
    history.execute(&world, UntagFixture::new(&world, "Vise").expect("unmark"));
    assert!(!is_fixture(&world, "Vise"));
    assert!(model::find_by_name(&world, "Vise").is_some(), "the solid stays");
    history.undo(&world);
    assert!(is_fixture(&world, "Vise"), "undo re-marks");
}

#[test]
fn import_fixture_is_one_undo_unit_fresh_or_update() {
    let world = new_world();
    let mut history = History::default();
    let mesh = || Mesh::from_facets(cube(p(0.0, 0.0, 0.0), p(5.0, 5.0, 5.0)));
    let cmd = ImportFixture::new(&world, ImportModel::from_mesh("Vise", mesh()), None).expect("import");
    assert_eq!(cmd.update_summary(), None, "a fresh name spawns");
    history.execute(&world, cmd);
    assert!(is_fixture(&world, "Vise"));
    history.undo(&world);
    assert!(model::find_by_name(&world, "Vise").is_none(), "one undo takes model and marker");
    history.redo(&world);
    assert!(is_fixture(&world, "Vise"), "one redo brings both back");

    // A name already carrying a plain model: update + mark, one unit.
    history.execute(&world, ImportModel::from_mesh("Plate", mesh()));
    let bigger = Mesh::from_facets(cube(p(0.0, 0.0, 0.0), p(9.0, 9.0, 2.0)));
    let cmd = ImportFixture::new(&world, ImportModel::from_mesh("Plate", bigger), None).expect("update");
    assert!(cmd.update_summary().is_some(), "an existing name updates");
    history.execute(&world, cmd);
    assert!(is_fixture(&world, "Plate"));
    history.undo(&world);
    assert!(!is_fixture(&world, "Plate"), "undo drops the marker");
    let e = model::find_by_name(&world, "Plate").expect("the plain model stays");
    let b = model::entity_world_aabb(&world, &e).expect("box");
    assert!((b.max.x - 5.0).abs() < 1e-9, "and its old geometry is back");

    // Refusals at construction: the table's name, an unknown setup.
    assert!(ImportFixture::new(&world, ImportModel::from_mesh("Machine/Table", mesh()), None).is_err());
    assert!(ImportFixture::new(&world, ImportModel::from_mesh("X", mesh()), Some("Nope")).is_err());
}

/// The face world of the fixtures suite: 60 mm box stock, a 4 mm end
/// mill, a face pass at z 19.5 under a 25 mm clearance.
fn face_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    history.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("op"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, history)
}

fn path_y(world: &World) -> (f64, f64, f64, f64) {
    let op = model::find_by_name(world, "Face1").expect("Face1");
    let path = model::toolpath_of(world, &op).expect("generated").path;
    path.moves.iter().flat_map(|m| [m.from, m.to]).fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(x0, y0, x1, y1), q| (x0.min(q.x), y0.min(q.y), x1.max(q.x), y1.max(q.y)),
    )
}

#[test]
fn a_warned_move_names_the_jaw_it_strikes_and_not_the_clear_one() {
    let (world, mut history) = face_world();
    let (x0, y0, x1, y1) = path_y(&world);
    // JawLo reaches 1 mm into the pass's low-y edge; JawHi stands 3 mm
    // clear of the high edge. Two separate closed fixtures.
    plant(&world, &mut history, "JawLo", cube(p(x0 - 10.0, y0 - 25.0, -5.0), p(x1 + 10.0, y0 - 1.0, 21.0)));
    plant(&world, &mut history, "JawHi", cube(p(x0 - 10.0, y1 + 5.0, -5.0), p(x1 + 10.0, y1 + 25.0, 21.0)));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    let s = fixture_struck::struck_by(&world, "Face1").expect("the pass warns");
    assert_eq!(s.fixtures, vec!["JawLo".to_owned()], "{s:?}");
    assert!(s.unresolved.is_empty(), "every warned move resolves: {s:?}");
    assert!(!s.warned.is_empty());
    assert!(s.names("JawLo") && !s.names("JawHi"));
    // The gate (refactored onto the same per-op law) still blocks.
    assert!(posting::verification_gate(&world).iter().any(|pr| pr.contains("strikes a fixture")));
}

#[test]
fn a_hit_the_steel_no_longer_reproduces_is_unresolved_not_guessed() {
    let (world, mut history) = face_world();
    let (x0, y0, x1, _) = path_y(&world);
    plant(&world, &mut history, "JawLo", cube(p(x0 - 10.0, y0 - 25.0, -5.0), p(x1 + 10.0, y0 - 1.0, 21.0)));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    // Move the jaw far away WITHOUT regenerating: the cached warnings
    // stand (workholding never stales a path), but no fixture strikes.
    let away = Location { pos: p(0.0, -500.0, 0.0), ..Location::default() };
    history.execute(&world, MoveFixture::new(&world, "JawLo", away).expect("move"));
    let s = fixture_struck::struck_by(&world, "Face1").expect("the cached path still warns");
    assert!(s.fixtures.is_empty(), "nothing is guessed: {s:?}");
    assert_eq!(s.unresolved, s.warned, "every warned move is unresolved");
}

#[test]
fn an_op_without_fixture_warnings_resolves_to_nothing() {
    let (world, _history) = face_world();
    assert_eq!(fixture_struck::struck_by(&world, "Face1"), None);
    assert_eq!(fixture_struck::struck_by(&world, "NoSuchOp"), None);
}

/// A mill stating only its spindle housing, `d` across and `len` long
/// - no travel, so no derived table joins the steel.
fn housing(d: f64) -> Machine {
    Machine {
        model: format!("Spindle {d}"),
        kind: Some("mill".into()),
        spindle_d: Some(d),
        spindle_len: Some(200.0),
        ..Default::default()
    }
}

fn face_params() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// One setup on a mill with spindle housing `d`: the face world's stock
/// and pass (`op`), and a tall clamp standing `gap` mm off the pass's
/// high-x edge, tagged to that setup alone.
fn housing_setup(world: &World, history: &mut History, setup: &str, op: &str, d: f64, gap: f64) {
    history.execute(world, CreateSetup::new(world, None, None).expect("setup"));
    let mch = Some((format!("spindle-{d}.mch"), Some(housing(d))));
    history.execute(world, SetSetupMachine::new(world, setup, mch).expect("machine"));
    let stock = SetStock::new(world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
    history.execute(world, stock.expect("stock"));
    history.execute(world, CreateOperation::new(world, op, "EM-0400-2FL", SrcRef::Stock, face_params()).expect("op"));
    history.execute(world, GenerateOperation::new(world, op).expect("generate"));
    let e = model::find_by_name(world, op).expect("op");
    let path = model::toolpath_of(world, &e).expect("generated").path;
    let x1 = path.moves.iter().flat_map(|m| [m.from.x, m.to.x]).fold(f64::MIN, f64::max);
    let clamp = format!("Clamp{op}");
    let c = model::spawn(world, &clamp);
    model::set_facets(world, &c, cube(p(x1 + gap, -200.0, -5.0), p(x1 + gap + 20.0, 200.0, 400.0)).into());
    model::set_rev(world, &c, 1);
    history.execute(world, TagFixture::new(world, &clamp, Some(setup)).expect("tagged"));
    history.execute(world, GenerateOperation::new(world, op).expect("regenerate"));
}

#[test]
fn the_struck_check_judges_each_op_on_its_own_setups_spindle() {
    let world = new_world();
    let mut history = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    // The same pass beside the same clamp, 60 mm off: only a 300 mm
    // housing (150 mm radius) reaches it, never a 20 mm one. Setup2 (the
    // small housing) is active last, so a global-machine read would
    // acquit both.
    housing_setup(&world, &mut history, "Setup1", "FaceBig", 300.0, 60.0);
    housing_setup(&world, &mut history, "Setup2", "FaceSmall", 20.0, 60.0);

    let findings = posting::fixture_findings(&world).expect("fresh program");
    assert!(findings.unjudged.is_empty(), "{:?}", findings.unjudged);
    let convicted: Vec<&str> = findings.hits.iter().map(|(n, _)| n.as_str()).collect();
    assert!(convicted.contains(&"FaceBig"), "the big housing strikes the clamp: {convicted:?}");
    assert!(!convicted.contains(&"FaceSmall"), "the small housing clears it: {convicted:?}");

    // op_hits itself, the law the viewport tint shares: same verdicts.
    let hits_of = |name: &str| {
        let e = model::find_by_name(&world, name).expect("op");
        let op = model::operation_of(&world, &e).expect("operation");
        let path = model::toolpath_of(&world, &e).expect("generated").path;
        let (turning, fixed) = fixture::workholding_for(&world, op.setup.as_deref());
        let meshes = fixture::meshes_for(&world, op.setup.as_deref());
        fixture_struck::op_hits(&world, &op, &path, &turning, &fixed, &meshes).expect("judged")
    };
    assert!(!hits_of("FaceBig").is_empty());
    assert!(hits_of("FaceSmall").is_empty());

    // The tint names the clamp for the big setup's op only.
    let s = fixture_struck::struck_by(&world, "FaceBig").expect("the big-housing pass warns");
    assert_eq!(s.fixtures, vec!["ClampFaceBig".to_owned()], "{s:?}");
    assert_eq!(fixture_struck::struck_by(&world, "FaceSmall"), None);
}
