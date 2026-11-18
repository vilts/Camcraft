//! #2177: a stock held in a vise is simulated. A locate that does not
//! turn the part leaves the stock's turn exactly as it was (no float
//! noise from the solve lands on it); a stock turned by a quarter turn
//! is simulated as its unturned twin; an oblique stock is refused in
//! plain words. Provenance: our own fixtures (a three-face vise, a
//! block part); nothing from vendor binaries.
//!
//! Gate map (M4.4): covers app::commands::MoveModel, app::commands::LocatePart, app::stock_follow::sim_frame, app::stock_sim::stock_grid, app::stock_view::field_view.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, LocatePart, MoveModel, SetStock, TagFixture,
};
use camcraft_app::fixture_locate::{LocateOffset, LocateSpec, Mate};
use camcraft_app::geometry::{Aabb, IDENTITY_Q, Mesh, Point3, Triangle, cube, quat_from_euler};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::pick::facets_centroid;
use camcraft_app::schema::Location;
use camcraft_app::stock::{StockShape, fit_box_sides};
use camcraft_app::stock_follow::OBLIQUE_STOCK;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::stock_view::field_view;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A vise: a floor, a fixed jaw on -Y, a stop on -X.
fn vise() -> Vec<Triangle> {
    let mut t: Vec<Triangle> = cube(p(0.0, 0.0, -10.0), p(100.0, 60.0, 0.0)).facets.to_vec();
    t.extend(cube(p(0.0, -20.0, 0.5), p(100.0, -1.0, 40.0)).facets.iter());
    t.extend(cube(p(-20.0, 0.0, 0.5), p(-1.0, 60.0, 30.0)).facets.iter());
    t
}

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

fn stock_entity(world: &World) -> flecs_ecs::prelude::Entity {
    let name = camcraft_app::setup::stock_entity_name(world, Some("Setup1"));
    model::find_by_name(world, &name).expect("stock")
}

/// A face op 0.5 mm under the stock top, generated.
fn face_op(world: &World, h: &mut History) {
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    h.execute(world, AddTool::new(world, "EM-0400-2FL", tool).expect("tool"));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, -0.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    h.execute(world, CreateOperation::new(world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("op"));
    h.execute(world, GenerateOperation::new(world, "Face1").expect("generated"));
}

/// The part placed at `rot`, a fitted box stock, then located in the vise.
fn located(rot: [f64; 4]) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(p(0.0, 0.0, 0.0), p(20.0, 10.0, 8.0))));
    h.execute(&world, MoveModel::new(&world, "part", Location { pos: p(150.0, 80.0, 30.0), rot }).expect("placed"));
    h.execute(&world, ImportModel::from_mesh("vise", Mesh { facets: vise().into() }));
    h.execute(&world, TagFixture::new(&world, "vise", None).expect("mark the vise"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let e = model::find_by_name(&world, "part").expect("part");
    let b = model::entity_world_aabb(&world, &e).expect("bounds");
    // Snapped: the 1e-13 turn widens the part's box by 1e-12, which
    // the sim's lattice would round up to one more cell.
    let snap = |q: Point3| p((q.x * 1e6).round() / 1e6, (q.y * 1e6).round() / 1e6, (q.z * 1e6).round() / 1e6);
    let (shape, spos) = fit_box_sides(&Aabb { min: snap(b.min), max: snap(b.max) }, [1.0; 6]).expect("fits");
    h.execute(&world, SetStock::new(&world, shape, Location { pos: spos, ..Location::default() }).expect("stock"));
    let mates = vec![
        Mate { part: face(&world, "part", [10.0, 5.0, 0.0]), fixture: face(&world, "vise", [50.0, 30.0, 0.0]) },
        Mate { part: face(&world, "part", [10.0, 0.0, 4.0]), fixture: face(&world, "vise", [50.0, -1.0, 20.25]) },
        Mate { part: face(&world, "part", [0.0, 5.0, 4.0]), fixture: face(&world, "vise", [-1.0, 30.0, 15.25]) },
    ];
    let spec = LocateSpec { part: "part".into(), fixture: "vise".into(), body: Default::default(), mates, offset: LocateOffset::default() };
    h.execute(&world, LocatePart::new(&world, &spec).expect("locate"));
    (world, h)
}

/// The stock top, and the highest point of the simulated sheet.
fn top_and_cut(world: &World) -> (f64, f64) {
    let b = model::entity_world_aabb(world, &stock_entity(world)).expect("stock bounds");
    let sim = simulate_document(world).expect("the stock in the vise is simulated");
    let high = sim.surface.verts.iter().map(|v| v.z).fold(f64::NEG_INFINITY, f64::max);
    (b.max.z, high)
}

#[test]
fn a_located_stock_keeps_no_rotation_noise_and_is_simulated() {
    // A part that carries a turn below the move's own tolerance: the
    // solve's float noise once landed on the stock (#2177).
    let (world, mut h) = located(quat_from_euler(0.0, 0.0, 1e-13));
    let at = model::transform_of(&world, &stock_entity(&world)).expect("placed");
    assert_eq!(at.rot, IDENTITY_Q, "the stock keeps its own turn exactly");
    face_op(&world, &mut h);
    let (top, high) = top_and_cut(&world);
    assert!(high < top - 0.4, "material is removed: sheet top {high}, stock top {top}");
    field_view(&world).expect("the viewport's traced stock draws it");
}

#[test]
fn a_quarter_turned_stock_is_simulated_and_an_oblique_one_refused() {
    let (world, mut h) = located(IDENTITY_Q);
    face_op(&world, &mut h);
    let e = stock_entity(&world);
    let at = model::transform_of(&world, &e).expect("placed");
    let (top, high) = top_and_cut(&world);
    // The same box, turned a quarter about its own vertical axis.
    let shape = model::stock_def_of(&world, &e).expect("shape");
    let StockShape::Box { dx, dy, dz } = shape else { panic!("a box stock") };
    let turned = Location { pos: at.pos, rot: quat_from_euler(0.0, 0.0, std::f64::consts::FRAC_PI_2) };
    let twin = StockShape::Box { dx: dy, dy: dx, dz };
    h.execute(&world, SetStock::new(&world, twin, turned).expect("turned stock"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerated"));
    let (top_q, high_q) = top_and_cut(&world);
    assert!((top_q - top).abs() < 1e-9, "the same solid: top {top_q} vs {top}");
    assert!((high_q - high).abs() < 1e-6, "the same cut: {high_q} vs {high}");
    field_view(&world).expect("the quarter-turned stock draws");
    // Turned 0.7 rad: no square twin - refused in plain words.
    let oblique = Location { pos: at.pos, rot: quat_from_euler(0.0, 0.0, 0.7) };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx, dy, dz }, oblique).expect("oblique stock"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerated"));
    let err = simulate_document(&world).expect_err("oblique stock");
    assert!(err.contains(OBLIQUE_STOCK), "{err}");
    let eye = field_view(&world).expect_err("oblique stock");
    assert!(eye.contains(OBLIQUE_STOCK), "{eye}");
}
