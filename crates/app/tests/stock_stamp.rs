//! Issue #156: an op's verdict reads the setup's stock (the rapid-in-
//! stock laws, the prior acquittal), so its stamp digests that stock - a
//! stock edit after generation marks it Stale and the post refuses until
//! it is regenerated; an edit the verdict never read stales nothing, and
//! undoing the stock edit returns the op to Ok.
//!
//! Provenance: our own fixtures (vertical_rapid_stock's 10 x 10 x 5 box
//! under a contour, in a 100 x 100 block), specced from the Prime
//! Directive and the shop rule "never traverse through uncut stock"; no
//! vendor material.
//!
//! Gate map (#156): covers camcraft_app::operation (stamp, stamp_stock),
//! camcraft_app::posting (the stale gate).
//! Lane shape: all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, SetWorkMaterial,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{WorkMaterial, catalog};

/// A contour of the 5 mm box: top 5, clearance 10, retract 8 - the
/// plunge-height rapid lands at Z 7, beside the box.
fn contour() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        chain_overrides: Vec::new(),
        picks: Default::default(),
    }
}

fn block(dz: f64) -> StockShape {
    StockShape::Box { dx: 100.0, dy: 100.0, dz }
}

/// The contour generated clean beside a block standing to its top.
fn generated() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, block(5.0), Default::default()).expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), contour());
    h.execute(&world, cmd.expect("create contour"));
    h.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate"));
    assert_eq!(status(&world), OpStatus::Ok, "the fixture generates Ok");
    (world, h)
}

fn status(world: &World) -> OpStatus {
    let e = model::find_by_name(world, "Contour1").expect("op");
    operation::status_of(world, &e)
}

fn post(world: &World, tag: &str) -> Result<(), String> {
    let p = std::env::temp_dir().join(format!("camcraft-stockstamp-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    let r = posting::post_setup(world, &p).map(|_| ());
    let _ = std::fs::remove_file(&p);
    r
}

#[test]
fn enlarging_the_stock_over_a_contours_dive_stales_it_and_blocks_the_post() {
    let (world, mut h) = generated();
    assert!(post(&world, "before").is_ok(), "the clean contour posts");
    // The block grows to Z 10: the rapid 10 -> 7 beside the box now
    // dives through 3 mm of uncut stock the cached verdict never saw.
    h.execute(&world, SetStock::new(&world, block(10.0), Default::default()).expect("taller stock"));
    assert_eq!(status(&world), OpStatus::Stale, "the verdict was judged against the old stock");
    match post(&world, "grown") {
        Ok(()) => panic!("a contour judged against the old stock posted"),
        Err(e) => assert!(e.contains("Contour1") && e.contains("regenerate"), "refused, but not as stale: {e}"),
    }
}

#[test]
fn moving_the_stock_stales_the_contour() {
    let (world, mut h) = generated();
    let at = camcraft_app::schema::Location { pos: Point3::new(5.0, 0.0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, block(5.0), at).expect("moved stock"));
    assert_eq!(status(&world), OpStatus::Stale, "a moved stock is a moved input");
}

#[test]
fn undoing_the_stock_edit_returns_the_contour_to_ok() {
    let (world, mut h) = generated();
    h.execute(&world, SetStock::new(&world, block(10.0), Default::default()).expect("taller stock"));
    assert_eq!(status(&world), OpStatus::Stale);
    assert!(h.undo(&world), "the stock edit undoes");
    assert_eq!(status(&world), OpStatus::Ok, "the stock the verdict read is back");
    assert!(post(&world, "undone").is_ok(), "and the program posts again");
}

#[test]
fn the_work_material_is_not_a_stock_input() {
    let (world, mut h) = generated();
    h.execute(&world, SetWorkMaterial::new(&world, Some(WorkMaterial::MildSteel)).expect("material"));
    assert_eq!(status(&world), OpStatus::Ok, "the material name never reached the verdict");
}
