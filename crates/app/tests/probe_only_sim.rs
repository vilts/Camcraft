//! #592: a probe-only program posted with "stock sim did not judge this
//! program: invalid stock simulation setup ... box non-empty" - the
//! probe stays out of the sim's plan (it removes nothing), so no tool
//! sized the lattice and the cell came out infinite. Now the uncut
//! stock is judged on a span-sized lattice: the program sim speaks, and
//! a stock that equals the part is clean. A genuinely missing stock
//! still refuses by name.
//!
//! Gate map (#592): covers app::stock_sim, sim::stock_sim (stock_grid's lattice sizing,
//! simulate_document), app::posting (verification_report's sim clause).
//! Provenance: our own fixtures.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::probing::{ProbeDir, ProbeMode};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// A 76 x 51 x 25 mm part (3 x 2 x 1 in), Z -25..0, and one top-face
/// probe touch at its centre; `stock` the stock to give it, if any.
fn probe_only(stock: Option<StockShape>) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-38.1, -25.4, -25.4), Point3::new(38.1, 25.4, 0.0))));
    if let Some(shape) = stock {
        let at = Location { pos: Point3::new(0.0, 0.0, -25.4), rot: IDENTITY_Q };
        h.execute(&world, SetStock::new(&world, shape, at).expect("stock"));
    }
    let probe = catalog().into_iter().find(|(n, _)| *n == "PRB-0400").map(|(_, t)| t).expect("catalog probe");
    h.execute(&world, AddTool::new(&world, "PRB-0400", probe).expect("probe tool"));
    let p = OpParams::Probe {
        clearance_z: Height::of(HeightReference::TopOfModel, 25.0),
        mode: ProbeMode::Face,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: [0.0, 0.0, 0.0],
        radius: 10.0,
        approach: 5.0,
        overtravel: 1.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        set_wcs: Some(0),
        tolerance: 0.5,
    };
    let src = SrcRef::Entity("part".into());
    h.execute(&world, CreateOperation::new(&world, "Probe1", "PRB-0400", src, p).expect("probe op"));
    h.execute(&world, GenerateOperation::new(&world, "Probe1").expect("generate"));
    world
}

#[test]
fn a_probe_only_program_is_judged_not_refused_as_an_invalid_setup() {
    // The stock is the part itself: nothing to cut, nothing left.
    let world = probe_only(Some(StockShape::Box { dx: 76.2, dy: 50.8, dz: 25.4 }));
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(report.not_judged, None, "the program sim judged the uncut stock - no misleading note");
}

#[test]
fn a_probe_only_program_without_a_stock_still_says_why() {
    let world = probe_only(None);
    let report = posting::verification_report(&world);
    let line = report.not_judged.expect("a program with no stock is not judged - and says so");
    assert!(!line.contains("invalid stock simulation setup"), "never the wrong cause: {line}");
    assert!(line.contains("no stock"), "the refusal names its own reason: {line}");
}
