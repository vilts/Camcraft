//! #406: the incoming stock's sheet for the viewport. One measurement
//! answers the page's box and volume and the sheet; the sheet stands in
//! the ACTIVE setup's frame - as measured when the receiving setup is the
//! active one, carried back by its alignment when another setup is - and
//! a setup that receives nothing is named, never an empty mesh.
//! Provenance: our own fixtures (a faced block, flipped over X); no
//! vendor material consulted.
//!
//! Gate map (#406): covers app::setup_stock_view.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    ActivateSetup, AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel,
    SetStock,
};
use camcraft_app::geometry::{Point3, cube, transform_point};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::{AlignPreset, is_identity_rot};
use camcraft_app::setup_stock;
use camcraft_app::setup_stock_view::{incoming_shape, to_view};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

fn add_face(world: &World, h: &mut History, name: &str, cut: f64) {
    let op = CreateOperation::new(world, name, TOOL, SrcRef::Stock, face(cut)).expect("create");
    h.execute(world, op);
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

/// A 60 x 60 x 16 part in a stock Z -2..18, Setup1 faced to 16, Setup2
/// flipped over X (so it takes what Setup1 leaves) and faced too.
fn flipped() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 16.0))));
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, at).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    add_face(&world, &mut h, "Face1", 16.0);
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip"));
    add_face(&world, &mut h, "Face2", 16.0);
    (world, h)
}

#[test]
fn the_sheet_stands_in_the_active_setups_frame() {
    let (world, mut h) = flipped();
    h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate Setup2"));
    let own = incoming_shape(&world, "Setup2").expect("Setup2 receives Setup1's body");
    assert_eq!(Ok(own.incoming.clone()), setup_stock::incoming(&world, "Setup2"), "the page's box and volume, one law");
    let t = to_view(&world, "Setup2");
    assert!(is_identity_rot(t.rot) && t.pos == Point3::ZERO, "its own frame is the view's: {t:?}");
    let top = own.mesh.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::MIN, f64::max);
    assert!((top - own.incoming.max[2]).abs() < 1e-9, "the sheet's top is the body's: {top} vs {:?}", own.incoming);
    // Hand check of #519: the top sheet alone lay buried in the part; the
    // mesh is a closed block, its floor at the body's bottom.
    let floor = own.mesh.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::MAX, f64::min);
    assert!((floor - own.incoming.min[2]).abs() < 1e-9, "the block's floor is the body's: {floor} vs {:?}", own.incoming);

    h.execute(&world, ActivateSetup::new(&world, Some("Setup1")).expect("activate Setup1"));
    let back = incoming_shape(&world, "Setup2").expect("still measured");
    assert_eq!(back.incoming, own.incoming, "the box stays in the receiving setup's frame");
    let t = to_view(&world, "Setup2");
    assert!(!is_identity_rot(t.rot), "Setup1 holds the part unflipped: the sheet is carried back");
    assert_eq!(back.mesh.len(), own.mesh.len());
    for (a, b) in own.mesh.iter().zip(&back.mesh) {
        for (p, q) in [(a.a, b.a), (a.b, b.b), (a.c, b.c)] {
            let m = transform_point(p, t.pos, t.rot);
            assert!((m.x - q.x).abs() < 1e-9 && (m.y - q.y).abs() < 1e-9 && (m.z - q.z).abs() < 1e-9, "{m:?} vs {q:?}");
        }
    }
    // Seen as Setup1 holds the part, the block stands back where Setup1
    // left it: its uncut underside at Z -2, its faced top at Z 16.
    let zs: Vec<f64> = back.mesh.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).collect();
    let (lo, hi) = zs.iter().fold((f64::MAX, f64::MIN), |(l, h), &z| (l.min(z), h.max(z)));
    assert!((lo + 2.0).abs() < 1e-6 && (hi - 16.0).abs() < 1e-6, "the block spans Z -2 .. 16, got {lo} .. {hi}");
}

#[test]
fn the_first_setup_draws_its_own_block_and_an_unknown_setup_is_named() {
    let (world, _h) = flipped();
    // The program's first setup starts from its own stock: the sheet is
    // its block's top, not a refusal.
    let first = incoming_shape(&world, "Setup1").expect("its own block");
    assert!((first.incoming.max[2] - 18.0).abs() < 1e-9, "{:?}", first.incoming);
    let err = incoming_shape(&world, "NoSuchSetup").expect_err("no such setup");
    assert!(!err.is_empty());
}

/// Hand check of #519: the incoming stock is drawn as a closed block -
/// every edge of its mesh is shared by exactly two facets.
#[test]
fn the_incoming_stock_is_a_closed_block() {
    let (world, _h) = flipped();
    let shape = incoming_shape(&world, "Setup2").expect("Setup2 receives Setup1's body");
    let key = |p: Point3| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64, (p.z * 1e6).round() as i64);
    let mut edges = std::collections::HashMap::new();
    for t in &shape.mesh {
        let v = [key(t.a), key(t.b), key(t.c)];
        for i in 0..3 {
            let (a, b) = (v[i], v[(i + 1) % 3]);
            *edges.entry(if a < b { (a, b) } else { (b, a) }).or_insert(0u32) += 1;
        }
    }
    let open = edges.values().filter(|n| **n != 2).count();
    assert_eq!(open, 0, "{open} of {} edges are not shared by two facets", edges.len());
}
