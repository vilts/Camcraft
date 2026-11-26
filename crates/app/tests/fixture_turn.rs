//! #2174: a fixture turned about a world axis - a quarter turn about each
//! axis and a typed angle, each one undo step with exact undo and redo;
//! quarter turns land on exact quaternions (no dust, #2177); refusals in
//! plain English; the collision boxes and the post gate read the turned
//! steel; a part or stock the fixture held is named, never left floating
//! in silence.
//!
//! Gate map (M4.4): covers app::commands::fixture_turn,
//! app::fixture_clash (held_by), app::fixture (boxes_for),
//! app::posting (verification_gate), app::mcp (turn_fixture).

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, SetStock, TagFixture, TurnFixture,
};
use camcraft_app::fixture;
use camcraft_app::geometry::{Point3, Triangle, quat_from_axis_angle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::setup;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

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

/// A fixture `name` with mesh `tris` (its own frame) standing at `at`.
fn plant(world: &World, history: &mut History, name: &str, tris: Vec<Triangle>, at: Location) {
    let e = model::spawn(world, name);
    model::set_facets(world, &e, tris.into());
    model::set_rev(world, &e, 1);
    model::set_transform(world, &e, at);
    history.execute(world, TagFixture::new(world, name, None).expect("tagged"));
}

fn rot_of(world: &World, name: &str) -> [f64; 4] {
    let e = model::find_by_name(world, name).expect("the fixture");
    model::transform_of(world, &e).expect("a pose").rot
}

fn box_of(world: &World, name: &str) -> fixture::FixtureBox {
    fixture::named_boxes_for(world, None).into_iter().find(|(n, _)| n == name).map(|(_, b)| b).expect("its box")
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_quarter_turn_about_each_axis_is_exact_and_undo_and_redo_restore_it() {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: p(100.0, 0.0, 0.0), ..Location::default() };
    plant(&world, &mut history, "Block", cube(p(0.0, 0.0, 0.0), p(10.0, 20.0, 30.0)), at);
    let before = rot_of(&world, "Block");
    let mut want = [[H, 0.0, 0.0, H], [0.0, H, 0.0, H], [0.0, 0.0, H, H]];
    for (axis, q) in want.iter_mut().enumerate() {
        history.execute(&world, TurnFixture::new(&world, "Block", axis, 90.0).expect("a quarter turn"));
        assert_eq!(rot_of(&world, "Block"), *q, "axis {axis}: exact components, no dust");
        // The collision box is the turned steel: about Z the 10 x 20 block
        // lies 20 x 10, its origin unmoved.
        if axis == 2 {
            let b = box_of(&world, "Block");
            assert!(near(b.max.x - b.min.x, 20.0) && near(b.max.y - b.min.y, 10.0), "{b:?}");
            assert!(near(b.max.x, 100.0) && near(b.min.y, 0.0), "turned about its origin: {b:?}");
        }
        assert!(history.undo(&world), "one undo step");
        assert_eq!(rot_of(&world, "Block"), before, "undo restores the exact orientation");
        assert!(history.redo(&world));
        assert_eq!(rot_of(&world, "Block"), *q, "redo lands it again");
        assert!(history.undo(&world));
    }
}

#[test]
fn a_typed_angle_turns_by_it_and_undo_and_redo_restore_it() {
    let world = new_world();
    let mut history = History::default();
    plant(&world, &mut history, "Block", cube(p(0.0, 0.0, 0.0), p(10.0, 20.0, 30.0)), Location::default());
    history.execute(&world, TurnFixture::new(&world, "Block", 2, 30.0).expect("a typed turn"));
    let want = quat_from_axis_angle(2, 30f64.to_radians());
    let got = rot_of(&world, "Block");
    assert!((0..4).all(|i| near(got[i], want[i])), "{got:?} vs {want:?}");
    assert!(history.undo(&world));
    assert_eq!(rot_of(&world, "Block"), [0.0, 0.0, 0.0, 1.0]);
    assert!(history.redo(&world));
    assert_eq!(rot_of(&world, "Block"), got);
}

#[test]
fn four_quarter_turns_are_the_exact_start_and_a_zero_turn_is_refused() {
    let world = new_world();
    let mut history = History::default();
    plant(&world, &mut history, "Block", cube(p(0.0, 0.0, 0.0), p(10.0, 20.0, 30.0)), Location::default());
    for _ in 0..4 {
        history.execute(&world, TurnFixture::new(&world, "Block", 0, 90.0).expect("a quarter turn"));
    }
    assert_eq!(rot_of(&world, "Block"), [0.0, 0.0, 0.0, 1.0], "no rotation dust (#2177)");
    for degrees in [0.0, 360.0, -720.0] {
        let why = TurnFixture::new(&world, "Block", 2, degrees).err().expect("nothing to turn").to_string();
        assert!(why.contains("leaves 'Block' as it stands"), "{why}");
    }
    let why = TurnFixture::new(&world, "Block", 2, f64::NAN).err().expect("refused").to_string();
    assert_eq!(why, "the turn angle must be a number of degrees");
    let why = TurnFixture::new(&world, "Block", 3, 90.0).err().expect("refused").to_string();
    assert_eq!(why, "turn the fixture about X, Y or Z");
    let why = TurnFixture::new(&world, fixture::MACHINE_TABLE, 2, 90.0).err().expect("refused").to_string();
    assert!(why.contains("machine table"), "{why}");
}

/// The fixtures suite's face pass: 60 mm box stock, a 4 mm end mill, a
/// face pass at z 19.5 under a 25 mm clearance.
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

fn path_bounds(world: &World) -> (f64, f64, f64, f64) {
    let op = model::find_by_name(world, "Face1").expect("Face1");
    let path = model::toolpath_of(world, &op).expect("generated").path;
    path.moves.iter().flat_map(|m| [m.from, m.to]).fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(x0, y0, x1, y1), q| (x0.min(q.x), y0.min(q.y), x1.max(q.x), y1.max(q.y)),
    )
}

fn strikes(world: &World) -> bool {
    posting::verification_gate(world).iter().any(|r| r.contains("strikes a fixture"))
}

/// SAFETY: a bar standing clear of the pass is turned into it. The pass
/// was generated before the turn and is not generated again: the post
/// gate reads the steel as it stands now and refuses; undo clears it.
#[test]
fn a_fixture_turned_into_the_tools_path_blocks_the_post_without_a_regenerate() {
    let (world, mut history) = face_world();
    let (x0, y0, x1, y1) = path_bounds(&world);
    let long = (x1 - x0) + 20.0;
    // The bar runs +X from 10 mm past the pass's far edge, through the cut level.
    let at = Location { pos: p(x1, (y0 + y1) / 2.0, 0.0), ..Location::default() };
    plant(&world, &mut history, "Bar", cube(p(10.0, -3.0, -5.0), p(10.0 + long, 3.0, 21.0)), at);
    assert!(!strikes(&world), "the bar stands clear: {:?}", posting::verification_gate(&world));
    let turn = TurnFixture::new(&world, "Bar", 2, 90.0).expect("first quarter");
    history.execute(&world, turn);
    history.execute(&world, TurnFixture::new(&world, "Bar", 2, 90.0).expect("second quarter"));
    let b = box_of(&world, "Bar");
    assert!(b.max.x <= x1 - 10.0 + 1e-9 && b.min.x < x0, "the bar now lies across the pass: {b:?}");
    assert!(strikes(&world), "the gate reads the turned bar: {:?}", posting::verification_gate(&world));
    assert!(history.undo(&world) && history.undo(&world));
    assert!(!strikes(&world), "undo puts the bar back clear");
}

/// The bodies a fixture held stay where they are, and the turn says so
/// by name; a fixture that holds nothing turns without that line.
#[test]
fn a_turned_fixture_names_the_stock_it_held_and_a_free_one_names_nothing() {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    let stock = setup::stock_entity_name(&world, None);
    let body = model::find_by_name(&world, &stock).and_then(|e| model::world_facets(&world, &e)).expect("stock body");
    let (mut lo, mut hi) = (p(f64::MAX, f64::MAX, f64::MAX), p(f64::MIN, f64::MIN, f64::MIN));
    for q in body.iter().flat_map(|t| [t.a, t.b, t.c]) {
        lo = p(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z));
        hi = p(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z));
    }
    // A plate the stock stands on, and a block far away.
    plant(&world, &mut history, "Plate", cube(p(lo.x, lo.y, lo.z - 10.0), p(hi.x, hi.y, lo.z)), Location::default());
    let far = Location { pos: p(500.0, 500.0, 0.0), ..Location::default() };
    plant(&world, &mut history, "Far", cube(p(0.0, 0.0, 0.0), p(10.0, 10.0, 10.0)), far);
    let turn = TurnFixture::new(&world, "Plate", 0, 90.0).expect("turn");
    let w = turn.warning().unwrap_or_default().to_owned();
    assert!(w.contains(&format!("'{stock}' did not turn with the fixture 'Plate' - locate them on it again")), "{w}");
    history.execute(&world, turn);
    let after = model::find_by_name(&world, &stock).and_then(|e| model::world_facets(&world, &e)).expect("stock body");
    assert_eq!(after.len(), body.len());
    assert!(after.iter().zip(body.iter()).all(|(a, b)| a.a == b.a), "the stock stays where it was");
    let turn = TurnFixture::new(&world, "Far", 2, 90.0).expect("turn");
    assert!(!turn.warning().unwrap_or_default().contains("did not turn"), "{:?}", turn.warning());
}
