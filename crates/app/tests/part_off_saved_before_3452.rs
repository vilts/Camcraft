//! #3452: a project saved before the part-off kerf moved to the chuck
//! side of the end face holds a cached part-off whose blade cut half a
//! blade into the part (CL on the plane Z-70, posted Z-71.5). The Part
//! stamp carries a salt since #3452, so that path loads Stale and the
//! post refuses it until the op is generated again; the groove beside
//! it (a blade kind whose path did not move) loads Ok.
//!
//! The fixture `fixtures/part_off_pre_3452.camproj` was recorded by
//! `record_the_pre_3452_fixture` on the tree before #3452
//! (`RECORD_PRE_3452_PART_OFF=1`, main at 6679b1b4b). Never record it
//! again on a later tree: it must hold the old law's path.
//!
//! Gate map (M4.4): covers app::operation (the Part stamp), app::project
//! (load), app::posting (the Stale refusal).
//! Lane shape: seconds.

use std::path::PathBuf;

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
/// The issue's pin with a 3 mm groove to r13 at Z-33..-30.
const PIN: [(f64, f64); 9] = [
    (0.0, -70.0),
    (14.0, -70.0),
    (15.0, -69.0),
    (15.0, -33.0),
    (13.0, -33.0),
    (13.0, -30.0),
    (15.0, -30.0),
    (15.0, 0.0),
    (0.0, 0.0),
];

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/part_off_pre_3452.camproj")
}

fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// The document as the issue's user saved it: the bar, the blade, a
/// groove finish G1 and the part-off P1 at the part's end face Z-70.
fn saved_world() -> World {
    let world = new_world();
    let mut h = History::default();
    let lathe = Machine {
        model: "TestBench lathe".into(),
        kind: Some("lathe".into()),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    };
    h.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    h.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("post"));
    let blade = catalog().into_iter().find(|(n, _)| *n == "GT-0300").map(|(_, t)| t).expect("blade");
    h.execute(&world, AddTool::new(&world, "GT-0300", blade).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("pin", revolve(&PIN, 16)));
    let at = Location { pos: Point3::new(0.0, 0.0, -84.0), ..Location::default() };
    let stock = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 34.0, height: 84.0 }, at);
    h.execute(&world, stock.expect("stock"));
    let groove = OpParams::GrooveFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(10.0),
        top_z: Height::fixed(-30.0),
        bottom_z: Height::fixed(-33.0),
        feed: 120.0,
        plunge_feed: 60.0,
        picks: Default::default(),
    };
    let part = OpParams::Part {
        tolerance: 0.05,
        clearance_z: Height::fixed(10.0),
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-70.0),
        stepdown: 3.0,
        feed: 120.0,
        plunge_feed: 60.0,
        bar_load: false,
        bar_advance: 0.0,
        bar_unload: false,
        bar_transfer: false,
        inspect_tol: 0.0,
        kerf: Default::default(),
        picks: Default::default(),
    };
    for (name, params) in [("G1", groove), ("P1", part)] {
        h.execute(&world, CreateOperation::new(&world, name, "GT-0300", SrcRef::Entity("pin".into()), params).expect("op"));
        h.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
    }
    world
}

fn status(world: &World, name: &str) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, name).expect("op"))
}

/// The cached path's feed stations (world Z), as stored.
fn cached_feed_z(world: &World, name: &str) -> Vec<f64> {
    let e = model::find_by_name(world, name).expect("op");
    let path = model::toolpath_of(world, &e).expect("cached").path;
    path.moves.iter().filter(|m| m.kind != MoveKind::Rapid).map(|m| m.to.z).collect()
}

#[test]
fn record_the_pre_3452_fixture() {
    // Records only on request, and only on a tree before #3452.
    if std::env::var_os("RECORD_PRE_3452_PART_OFF").is_none() {
        return;
    }
    let world = saved_world();
    assert_eq!(status(&world, "P1"), OpStatus::Ok);
    project::save(&world, &fixture()).expect("save the fixture");
}

#[test]
fn a_part_off_saved_before_3452_loads_stale_and_does_not_post() {
    let world = new_world();
    project::load_noted(&world, &fixture()).expect("the fixture opens");
    // The fixture holds the old law: the CL on the end face Z-70.
    let old = cached_feed_z(&world, "P1");
    assert!(!old.is_empty() && old.iter().all(|z| (z + 70.0).abs() < 1e-6), "the old centred kerf: {old:?}");
    assert_eq!(status(&world, "P1"), OpStatus::Stale, "a part-off cached before #3452 must be generated again");
    assert_eq!(status(&world, "G1"), OpStatus::Ok, "the groove's path did not move: its stamp must not change");
    let err = posting::post_setup_unwritten(&world).map(|_| ()).expect_err("a stale part-off never posts");
    assert!(err.contains("P1: inputs moved since generation"), "{err}");
    // Generated again, it follows the new law and posts.
    let mut h = History::default();
    h.execute(&world, GenerateOperation::new(&world, "P1").expect("generate"));
    assert_eq!(status(&world, "P1"), OpStatus::Ok);
    let new = cached_feed_z(&world, "P1");
    assert!(new.iter().all(|z| (z + 71.5).abs() < 1e-6), "the CL half a blade behind the face: {new:?}");
    posting::post_setup_unwritten(&world).expect("it posts once generated again");
}
