//! #3004: a tilted (3+2) operation reads the stock's cap along its own
//! tool axis. The tilted verify once started its cap from the WORLD
//! stock top (Z 70 on a 60 x 60 x 70 blank) and only raised it with the
//! projected box corners, so side work at A +90 or A -90, whose real
//! local cap is 30, was judged against 70: a shank standing 11 mm clear
//! of the stock was named a `ShankHit`, and a clearance plane 10 mm over
//! the side face was named low. A shank that really enters the stock
//! still is.
//!
//! Provenance: our own synthetic fixtures (the issue's blank and tool),
//! rigid-frame projection by hand; no vendor material.
//!
//! Gate map (#3004): covers app::operation (generate's tilted verify
//! wiring, first_traverse), cl::frame, sim::verify (shank law).
//! Lane shape: all tests here run in seconds.

mod pocket_support;

use camcraft_app::cl::TiltedFrame;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef, Tilt};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use flecs_ecs::prelude::World;
use pocket_support::block_with_pocket;

fn from_stock(offset: f64) -> Height {
    Height { reference: HeightReference::TopOfStock, offset }
}

fn tilt(a_deg: f64) -> Tilt {
    Tilt { basis_version: 1, a_deg, b_deg: 0.0, origin: [0.0; 3] }
}

fn frame(a_deg: f64) -> TiltedFrame {
    TiltedFrame::from_angles(Point3::ZERO, a_deg, 0.0).expect("a side frame")
}

/// The local Y of the blank's mid height (world Z 35) in the plane.
fn mid_y(a_deg: f64) -> f64 {
    frame(a_deg).to_local(Point3::new(0.0, 0.0, 35.0)).y
}

/// The issue's blank (X/Y -30..30, Z 0..70) and its 6 mm end mill
/// (15 mm flutes, no holder).
fn bench() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let stock = StockShape::Box { dx: 60.0, dy: 60.0, dz: 70.0 };
    h.execute(&world, SetStock::new(&world, stock, Default::default()).expect("stock"));
    let (_, tool) = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").expect("catalog");
    assert!((tool.flute_len - 15.0).abs() < 1e-9, "the fixture: 15 mm flutes, got {}", tool.flute_len);
    h.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    (world, h)
}

fn generate(world: &World, h: &mut History, src: SrcRef, params: OpParams) -> Vec<Warning> {
    h.execute(world, CreateOperation::new(world, "Side", "EM-0600-4FL", src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "Side").expect("generate"));
    let e = model::find_by_name(world, "Side").expect("op");
    let tp = model::toolpath_of(world, &e).expect("the side op generates");
    assert!(!tp.path.moves.is_empty(), "a path: {:?}", tp.path.notes);
    tp.warnings
}

/// A 4 mm slot along local X on the side face at world Z 35, every
/// height from Top of stock (clearance 10 over the face).
fn slot(a_deg: f64) -> OpParams {
    let y = mid_y(a_deg);
    OpParams::Slot {
        blend_turns: false,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        a: Point2::new(-10.0, y),
        b: Point2::new(10.0, y),
        width: 6.0,
        stock_to_leave: 0.0,
        clearance_z: from_stock(10.0),
        retract_z: None,
        top_z: from_stock(0.0),
        bottom_z: from_stock(-4.0),
        stepdown: 2.0,
        stepover: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: Some(tilt(a_deg)),
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

/// The blank as a part with a 20 x 10 pocket `depth` deep in the side
/// face that the A `a_deg` plane looks at, centred on world Z 35: built
/// in the plane's frame (local Z -30..30, the face at 30) and carried to
/// the world by the frame - a rigid move keeps it watertight.
fn side_pocket_part(a_deg: f64, depth: f64) -> Mesh {
    let ring = [(10.0, -5.0), (10.0, 5.0), (-10.0, 5.0), (-10.0, -5.0)];
    let block = block_with_pocket(30.0, 35.0, &ring, 60.0 - depth, 60.0);
    let (f, y) = (frame(a_deg), mid_y(a_deg));
    let w = |p: Point3| f.to_world(Point3::new(p.x, p.y + y, p.z - 30.0));
    Mesh::from_facets(block.facets.iter().map(|t| Triangle::new(w(t.a), w(t.b), w(t.c))).collect())
}

fn pocket(a_deg: f64, depth: f64) -> OpParams {
    OpParams::Pocket { pass_start: Default::default(),
        rest: None,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: from_stock(10.0),
        retract_z: None,
        top_z: from_stock(0.0),
        bottom_z: from_stock(-depth),
        stepdown: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: 0.4,
        blend_turns: false,
        tilt: Some(tilt(a_deg)),
        corner_relief: Default::default(),
        picks: Default::default(),
    }
}

fn side_pocket(a_deg: f64, depth: f64) -> Vec<Warning> {
    let (world, mut h) = bench();
    h.execute(&world, ImportModel::from_mesh("part", side_pocket_part(a_deg, depth)));
    generate(&world, &mut h, SrcRef::Entity("part".into()), pocket(a_deg, depth))
}

fn shank_hits(w: &[Warning]) -> Vec<&Warning> {
    w.iter().filter(|w| matches!(w, Warning::ShankHit { .. })).collect()
}

#[test]
fn a_side_pocket_reads_the_local_stock_cap_not_the_world_top() {
    // Local cap 30, floor 26: the shank's foot stands at 26 + 15 = 41,
    // 11 mm over the side face. On main the cap read the world top 70.
    for a in [90.0, -90.0] {
        let w = side_pocket(a, 4.0);
        assert_eq!(shank_hits(&w), Vec::<&Warning>::new(), "A {a}: the shank stands clear of the cap 30: {w:?}");
    }
}

#[test]
fn a_shank_that_enters_the_side_face_still_convicts() {
    // Floor 10 (20 deep): the shank's foot at 10 + 15 = 25 is 5 mm
    // under the side face - a real collision with uncut stock.
    for a in [90.0, -90.0] {
        let w = side_pocket(a, 20.0);
        assert!(!shank_hits(&w).is_empty(), "A {a}: the shank 5 mm into the stock: {w:?}");
    }
}

#[test]
fn a_side_slot_reads_its_clearance_against_the_local_cap() {
    // Clearance 40 along the axis stands 10 over the side face (cap 30).
    // On main the cap read 70: the first traverse was named low and the
    // drop to the face a rapid in stock.
    for a in [90.0, -90.0] {
        let (world, mut h) = bench();
        let w = generate(&world, &mut h, SrcRef::Stock, slot(a));
        assert_eq!(w, Vec::<Warning>::new(), "A {a}: the clearance stands over the cap 30: {w:?}");
    }
}
