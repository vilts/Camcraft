//! #582: a plain Model-page `MoveModel` (a move or a turn) runs the same
//! sampled part-and-stock-versus-fixture check a locate runs (#561) and
//! carries a by-name warning - the body, the fixture, a point - never a
//! refusal. A clear move warns nothing; a document without fixtures runs
//! no check; undo puts everything back as before. A Fixtures-page
//! `MoveFixture` into the part warns the same way.
//! Provenance: our own fixtures over plain vise practice (machine-tool
//! handbooks); no vendor material consulted.
//!
//! Gate map (#582): covers app::fixture_clash, app::commands::move_model
//! (`MoveModel::warning`), app::commands::fixture (`MoveFixture::warning`).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateSetup, History, ImportModel, MoveFixture, MoveModel, SetStock, TagFixture};
use camcraft_app::geometry::{Aabb, Mesh, Point3, Triangle, cube, quat_from_euler};
use camcraft_app::model;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock::fit_box_sides;
use camcraft_app::world::new_world;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Floor top at Z 0, fixed jaw Y -20..-1 (Z 0.5..40), stop X -20..-1.
fn vise() -> Vec<Triangle> {
    let mut t: Vec<Triangle> = cube(p(0.0, 0.0, -10.0), p(100.0, 60.0, 0.0)).facets.to_vec();
    t.extend(cube(p(0.0, -20.0, 0.5), p(100.0, -1.0, 40.0)).facets.iter());
    t.extend(cube(p(-20.0, 0.0, 0.5), p(-1.0, 60.0, 30.0)).facets.iter());
    t
}

/// The 20 x 10 x 8 part well above the vise; with `fixture` the vise is
/// imported and marked; Setup1; with `bottom` a box stock round the part
/// with 1 mm on each side and `bottom` mm under it.
fn document(fixture: bool, bottom: Option<f64>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(p(0.0, 0.0, 0.0), p(20.0, 10.0, 8.0))));
    h.execute(&world, MoveModel::new(&world, "part", at(40.0, 20.0, 50.0)).expect("placed"));
    if fixture {
        h.execute(&world, ImportModel::from_mesh("vise", Mesh::from_facets(vise())));
        h.execute(&world, TagFixture::new(&world, "vise", None).expect("mark the vise"));
    }
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    if let Some(b) = bottom {
        box_stock(&world, &mut h, [1.0, 1.0, 1.0, 1.0, b, 1.0]);
    }
    (world, h)
}

/// A square box stock round the part: -X, +X, -Y, +Y, under, top.
fn box_stock(world: &World, h: &mut History, sides: [f64; 6]) {
    let e = model::find_by_name(world, "part").expect("part");
    let bb = model::entity_world_aabb(world, &e).expect("bounds");
    let (shape, pos) = fit_box_sides(&Aabb { min: bb.min, max: bb.max }, sides).expect("fits");
    h.execute(world, SetStock::new(world, shape, Location { pos, ..Location::default() }).expect("stock"));
}

fn at(x: f64, y: f64, z: f64) -> Location {
    Location { pos: p(x, y, z), ..Location::default() }
}

fn loc(world: &World, name: &str) -> Location {
    let e = model::find_by_name(world, name).expect("entity");
    model::transform_of(world, &e).unwrap_or_default()
}

fn stock_loc(world: &World) -> Location {
    loc(world, &camcraft_app::setup::stock_entity_name(world, Some("Setup1")))
}

#[test]
fn a_move_that_sinks_the_part_into_the_floor_warns_by_name() {
    let (world, mut h) = document(true, None);
    let c = MoveModel::new(&world, "part", at(10.0, 10.0, -3.0)).expect("a warning is not a refusal");
    let w = c.warning().expect("3 mm of the part goes through the floor").to_owned();
    assert!(w.starts_with("'part' would sit inside the fixture 'vise' (near "), "{w}");
    h.execute(&world, c);
    assert!((loc(&world, "part").pos.z + 3.0).abs() < 1e-9, "the move lands");
}

#[test]
fn a_turn_that_swings_the_part_into_the_jaw_warns_by_name() {
    let (world, _h) = document(true, None);
    // Yaw 90: the part spans X 40..50, Y -5..15 - 4 mm into the fixed jaw.
    let turned = Location { pos: p(50.0, -5.0, 5.0), rot: quat_from_euler(0.0, 0.0, 90f64.to_radians()) };
    let c = MoveModel::new(&world, "part", turned).expect("turned");
    let w = c.warning().expect("the turned part enters the jaw");
    assert!(w.contains("'part' would sit inside the fixture 'vise'"), "{w}");
}

#[test]
fn a_move_whose_stock_allowance_sinks_into_the_floor_warns_for_the_stock_only() {
    let (world, _h) = document(true, Some(3.0));
    let stock = camcraft_app::setup::stock_entity_name(&world, Some("Setup1"));
    // The part rests on the floor; its 3 mm of stock underneath does not.
    let c = MoveModel::new(&world, "part", at(10.0, 10.0, 0.0)).expect("moved");
    let w = c.warning().expect("the stock goes through the floor");
    assert!(w.starts_with(&format!("the stock '{stock}' would sink into the fixture 'vise' (near ")), "{w}");
    assert!(!w.contains("'part' would sit"), "the part itself only touches: {w}");
}

#[test]
fn a_turn_whose_regrown_square_stock_sinks_into_the_jaw_warns_for_the_stock() {
    // 3 mm all round: a 26 x 16 x 14 block, square to the machine.
    let (world, mut h) = document(true, None);
    box_stock(&world, &mut h, [3.0; 6]);
    let stock = camcraft_app::setup::stock_entity_name(&world, Some("Setup1"));
    // Yaw 90: the part spans X 40..50, Y 0..20, Z 10..18 - clear of the
    // jaw face at Y -1. #571 regrows the block square round it, 16 x 26:
    // Y -3..23, 2 mm into the jaw. The pre-turn 26 x 16 block on the same
    // centre (Y 2..18), or left where it was (Z 47..61), touches nothing -
    // so this warns only when the check reads the re-fitted block.
    let turned = Location { pos: p(50.0, 0.0, 10.0), rot: quat_from_euler(0.0, 0.0, 90f64.to_radians()) };
    let c = MoveModel::new(&world, "part", turned).expect("a warning is not a refusal");
    let w = c.warning().expect("the regrown stock enters the jaw").to_owned();
    assert!(w.starts_with(&format!("the stock '{stock}' would sink into the fixture 'vise' (near ")), "{w}");
    assert!(!w.contains("'part' would sit"), "the part itself is clear: {w}");
    h.execute(&world, c);
    let se = model::find_by_name(&world, &stock).expect("stock");
    match model::stock_def_of(&world, &se) {
        Some(StockShape::Box { dx, dy, .. }) => {
            assert!((dx - 16.0).abs() < 1e-6 && (dy - 26.0).abs() < 1e-6, "regrown, not carried: {dx} x {dy}");
        }
        other => panic!("still a box stock: {other:?}"),
    }
}

#[test]
fn a_clear_move_warns_nothing() {
    let (world, _h) = document(true, Some(3.0));
    let c = MoveModel::new(&world, "part", at(10.0, 10.0, 3.0)).expect("moved");
    assert_eq!(c.warning(), None, "the stock rests on the floor, the part above it");
}

#[test]
fn no_fixtures_means_no_check() {
    let (world, _h) = document(false, Some(3.0));
    let c = MoveModel::new(&world, "part", at(10.0, 10.0, -3.0)).expect("moved");
    assert_eq!(c.warning(), None, "nothing marked as workholding: nothing to sink into");
}

#[test]
fn undo_of_a_warned_move_puts_part_and_stock_back() {
    let (world, mut h) = document(true, Some(3.0));
    let (part0, stock0) = (loc(&world, "part"), stock_loc(&world));
    let c = MoveModel::new(&world, "part", at(10.0, 10.0, -3.0)).expect("moved");
    assert!(c.warning().is_some());
    h.execute(&world, c);
    assert!((stock_loc(&world).pos.z - stock0.pos.z).abs() > 1.0, "the stock followed");
    h.undo(&world);
    assert_eq!(loc(&world, "part"), part0);
    assert_eq!(stock_loc(&world), stock0);
    h.redo(&world);
    assert!((loc(&world, "part").pos.z + 3.0).abs() < 1e-9, "redo replays the warned move");
}

#[test]
fn moving_the_fixture_into_the_part_warns_by_name() {
    let (world, _h) = document(true, None);
    // Lifted 55 mm the floor spans Z 45..55: 5 mm into the part (Z 50..58).
    let c = MoveFixture::new(&world, "vise", at(0.0, 0.0, 55.0)).expect("moved");
    let w = c.warning().expect("the floor rises into the part");
    assert!(w.starts_with("'part' would sit inside the fixture 'vise' (near "), "{w}");
    let clear = MoveFixture::new(&world, "vise", at(0.0, 0.0, 5.0)).expect("moved");
    assert_eq!(clear.warning(), None);
}
