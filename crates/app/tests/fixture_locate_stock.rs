//! #561 review: a located part's box stock stays square to the setup -
//! the vise holds the blank square, so a locate that turns the part back
//! square re-fits a square stock round the part with the allowance it had
//! on each side (top stays top), instead of turning the block with the
//! part; a Face op then regenerates at the new stock top; a stock whose
//! allowance would sink it into the fixture warns by name; undo puts the
//! block back. A plain `MoveModel` turn keeps the same rule since #571
//! (pinned in `move_model_stock` and `commands_undo`).
//! Provenance: our own fixtures over plain vise practice (machine-tool
//! handbooks); no vendor material consulted.
//!
//! Gate map (#561): covers app::stock_follow, app::commands::locate,
//! app::commands::move_model (`MoveModel::locating`),
//! app::commands::setup_align (`AlignPartToSetup::locating`).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, LocatePart,
    MoveModel, SetStock, TagFixture,
};
use camcraft_app::fixture_locate::{LocateOffset, LocateSpec, Mate};
use camcraft_app::geometry::{Aabb, Mesh, Point3, Triangle, cube, quat_from_euler};
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, HeightReference, OpParams, OpStatus, SrcRef};
use camcraft_app::pick::facets_centroid;
use camcraft_app::schema::Location;
use camcraft_app::setup_align::{self, AlignPreset};
use camcraft_app::stock::{StockShape, fit_box_sides};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Floor top at Z 0 (+Z), fixed jaw face at Y -1 (+Y), stop face at X -1 (+X).
fn vise() -> Vec<Triangle> {
    let mut t: Vec<Triangle> = cube(p(0.0, 0.0, -10.0), p(100.0, 60.0, 0.0)).facets.to_vec();
    t.extend(cube(p(0.0, -20.0, 0.5), p(100.0, -1.0, 40.0)).facets.iter());
    t.extend(cube(p(-20.0, 0.0, 0.5), p(-1.0, 60.0, 30.0)).facets.iter());
    t
}

/// The 20 x 10 x 8 part turned 90 deg about X then 30 deg about Z, far
/// from the vise; Setup1; a square box stock round the turned part with
/// `sides` = [-X, +X, -Y, +Y, bottom, top] allowance.
fn document(sides: [f64; 6]) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(p(0.0, 0.0, 0.0), p(20.0, 10.0, 8.0))));
    let rot = quat_from_euler(90f64.to_radians(), 0.0, 30f64.to_radians());
    h.execute(&world, MoveModel::new(&world, "part", Location { pos: p(150.0, 80.0, 30.0), rot }).expect("turned"));
    h.execute(&world, ImportModel::from_mesh("vise", Mesh::from_facets(vise())));
    h.execute(&world, TagFixture::new(&world, "vise", None).expect("mark the vise"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let (lo, hi) = bounds(&world, "part");
    let b = Aabb { min: p(lo[0], lo[1], lo[2]), max: p(hi[0], hi[1], hi[2]) };
    let (shape, pos) = fit_box_sides(&b, sides).expect("fits");
    h.execute(&world, SetStock::new(&world, shape, Location { pos, ..Location::default() }).expect("square stock"));
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

/// Floor, jaw and stop: the part fully located square in the vise.
fn spec(world: &World) -> LocateSpec {
    let mates = vec![
        Mate { part: face(world, "part", [10.0, 5.0, 0.0]), fixture: face(world, "vise", [50.0, 30.0, 0.0]) },
        Mate { part: face(world, "part", [10.0, 0.0, 4.0]), fixture: face(world, "vise", [50.0, -1.0, 20.25]) },
        Mate { part: face(world, "part", [0.0, 5.0, 4.0]), fixture: face(world, "vise", [-1.0, 30.0, 15.25]) },
    ];
    LocateSpec { part: "part".into(), fixture: "vise".into(), body: Default::default(), mates, offset: LocateOffset::default() }
}

fn bounds(world: &World, name: &str) -> ([f64; 3], [f64; 3]) {
    let e = model::find_by_name(world, name).expect("entity");
    let b = model::entity_world_aabb(world, &e).expect("bounds");
    ([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z])
}

fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
}

fn stock_name(world: &World) -> String {
    camcraft_app::setup::stock_entity_name(world, Some("Setup1"))
}

fn stock_state(world: &World) -> (StockShape, Location) {
    let e = model::find_by_name(world, &stock_name(world)).expect("stock");
    (model::stock_def_of(world, &e).expect("def"), model::transform_of(world, &e).unwrap_or_default())
}

/// A Face op 0.5 below the stock top, generated.
fn face_op(world: &World, h: &mut History) {
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    h.execute(world, AddTool::new(world, "EM-0400-2FL", tool).expect("tool"));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, -0.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    h.execute(world, CreateOperation::new(world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("op"));
    h.execute(world, GenerateOperation::new(world, "Face1").expect("generated"));
}

/// The planar feed levels of the op's path.
fn levels(world: &World) -> Vec<f64> {
    let e = model::find_by_name(world, "Face1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    tp.path.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z).map(|m| m.to.z).collect()
}

#[test]
fn a_located_turned_part_keeps_its_square_stock_square_with_its_allowance() {
    // Flush on the faces that go to the stop, the jaw and the floor; 1 mm
    // on the free sides, 2 mm on top.
    let (world, mut h) = document([0.0, 1.0, 0.0, 1.0, 0.0, 2.0]);
    face_op(&world, &mut h);
    let before = stock_state(&world);
    let c = LocatePart::new(&world, &spec(&world)).expect("located");
    assert_eq!(c.warning(), None, "flush on the mated sides: nothing sinks into the vise");
    h.execute(&world, c);
    let (shape, at) = stock_state(&world);
    assert!(setup_align::is_identity_rot(at.rot), "the stock stays square to the setup: {:?}", at.rot);
    let (lo, hi) = bounds(&world, &stock_name(&world));
    assert!(close(lo, [-1.0, -1.0, 0.0], 1e-6), "on the stop, the jaw and the floor with the part: {lo:?}");
    assert!(close(hi, [20.0, 10.0, 10.0], 1e-6), "the part's new place plus its allowance: {hi:?}");
    assert_eq!(shape, StockShape::Box { dx: 21.0, dy: 11.0, dz: 10.0 }, "re-fitted round the square part");
    // The Face op reads the new stock top.
    let e = model::find_by_name(&world, "Face1").expect("op");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Stale);
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerated"));
    let z = levels(&world);
    assert!(!z.is_empty() && z.iter().all(|z| (z - 9.5).abs() < 1e-6), "faces 0.5 under the 10 mm top: {z:?}");
    // Undo restores the block and its place, bytes for bytes.
    h.undo(&world);
    h.undo(&world);
    assert_eq!(stock_state(&world), before);
}

#[test]
fn a_stock_allowance_under_the_floor_mate_warns_by_name() {
    let (world, _h) = document([0.0, 1.0, 0.0, 1.0, 3.0, 2.0]);
    let c = LocatePart::new(&world, &spec(&world)).expect("built");
    let w = c.warning().expect("3 mm of stock under the part goes through the floor");
    let name = stock_name(&world);
    assert!(w.contains(&format!("the stock '{name}' would sink into the fixture 'vise'")), "{w}");
    assert!(!w.contains("'part' would sit inside"), "the part itself sits on the floor: {w}");
}

#[test]
fn in_a_turned_setup_the_located_stock_stays_square_with_its_allowance() {
    // The quarter turn carries the allowances round with the block: -X 0,
    // +X 1, -Y 0, +Y 1 once turned.
    let (world, mut h) = document([0.0, 1.0, 1.0, 0.0, 0.0, 2.0]);
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup1", AlignPreset::TurnZ90).expect("turned"));
    let c = LocatePart::new(&world, &spec(&world)).expect("located");
    assert_eq!(c.warning(), None);
    h.execute(&world, c);
    let (_, at) = stock_state(&world);
    assert!(setup_align::is_identity_rot(at.rot), "{:?}", at.rot);
    let (lo, hi) = bounds(&world, &stock_name(&world));
    assert!(close(lo, [-1.0, -1.0, 0.0], 1e-6), "{lo:?}");
    assert!(close(hi, [20.0, 10.0, 10.0], 1e-6), "{hi:?}");
}
