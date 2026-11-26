//! #3096: a rotary kind on a mill-turn turns the bar on the spindle C
//! under a radial live tool. Its path states C, so only the lathe post
//! writes it: C turns to the first point with the turret at its index
//! position, then X and Z go in one rapid. The first rapid answers to
//! #3077's live-tool law. A steady on the station line out beyond the
//! first point refuses the contour by name; the same steady opposite
//! the station (the machine frame's -X) stays clear, as fixed steel is
//! read where it stands, not revolved.
//!
//! Provenance: our own fixtures (c_axis_live's standing bar); the shop
//! rule "never traverse through uncut stock or steel" (machine-tool
//! handbooks). No vendor material.
//!
//! Gate map (#3096): covers app::operation (rotary_traverse, rotary_gate,
//! generate, stamp), sim::verify (live_traverse).
//! Lane shape: all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, History, ImportModel, LoadMachine, SetFixtureFixed, SetStock, TagFixture};
use camcraft_app::geometry::{cube, IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

const TAU: f64 = std::f64::consts::TAU;

fn mill_turn() -> Machine {
    Machine {
        model: "TestBench mill-turn".into(),
        kind: Some("mill-turn".into()),
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Y".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "C".into(), rotary: true, axis: Some([0.0, 0.0, 1.0]), feed: Some(6000.0), rapid: Some(12000.0), ..Default::default() },
        ],
        ..Default::default()
    }
}

/// The turned bar standing on the spindle (c_axis_live's): wall r6,
/// shoulder, neck r5.
fn bar() -> Vec<Triangle> {
    const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];
    let pt = |rho: f64, s: f64, j: usize| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(rho * th.cos(), rho * th.sin(), s)
    };
    let mut f = Vec::new();
    for w in MERIDIAN.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..16 {
            let k = (j + 1) % 16;
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, k), pt(rb, zb, j), pt(rb, zb, k));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

/// The bar in a Ø12.4 bar stock on the mill-turn, with a ball mill.
fn world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    let loc = Location { pos: Point3::new(0.0, 0.0, -16.0), rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Cylinder { diameter: 12.4, height: 32.0 }, loc).expect("stock"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(bar())));
    h.execute(&world, LoadMachine::new(&world, "millturn.mch".into(), Some(mill_turn())).expect("machine"));
    assert_eq!(camcraft_app::machine::kind_of_setup(&world, None), camcraft_app::machine::Kind::MillTurn);
    (world, h)
}

fn contour() -> Operation {
    Operation {
        tool: "BN-0600".into(),
        src: SrcRef::Entity("bar".into()),
        params: OpParams::RotaryContour {
            tolerance: 0.05,
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(6.0),
            bottom_z: Height::fixed(-8.0),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: 60.0,
            stated_axis: None,
            picks: Default::default(),
        },
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

/// A fixed steady, a 4 mm cube about `at` (the machine frame).
fn steady(world: &World, h: &mut History, at: Point3) {
    let e = model::spawn(world, "Steady1");
    let (lo, hi) = (Point3::new(at.x - 2.0, at.y - 2.0, at.z - 2.0), Point3::new(at.x + 2.0, at.y + 2.0, at.z + 2.0));
    model::set_facets(world, &e, cube(lo, hi).facets.to_vec().into());
    model::set_rev(world, &e, 1);
    h.execute(world, TagFixture::new(world, "Steady1", None).expect("tagged"));
    h.execute(world, SetFixtureFixed::new(world, "Steady1", true).expect("declared fixed"));
}

/// The bare contour: it posts the first rapid clear, and the radius and
/// the height of the point that rapid goes to.
fn first_point() -> (Toolpath, f64, f64) {
    let (world, _) = world();
    let (path, _) = operation::generate(&world, &contour(), "C wrap").unwrap_or_else(|e| panic!("the bare contour is clear: {e}"));
    let m = path.moves[0];
    assert!(m.rotary.is_some(), "the path states the spindle C: {m:?}");
    (path, m.to.x.hypot(m.to.y), m.to.z)
}

#[test]
fn a_steady_on_the_station_line_refuses_the_rotary_contour_by_name() {
    let (_, r, z) = first_point();
    let (world, mut h) = world();
    // On the +X station, 150 mm out past the first point, at its height:
    // the rapid from the turret's index position passes through it.
    steady(&world, &mut h, Point3::new(r + 150.0, 0.0, z));
    let err = operation::generate(&world, &contour(), "C wrap").map(|_| ()).expect_err("the steady holds the contour");
    let err = err.to_string();
    for text in ["Rotary Contour: the first rapid comes from the turret's index position", "a fixed fixture", "raise the clearance height"] {
        assert!(err.contains(text), "{text}: {err}");
    }
}

#[test]
fn the_same_steady_opposite_the_station_stays_clear() {
    let (bare, r, z) = first_point();
    let (world, mut h) = world();
    // Fixed steel never turns: at -X it stands off the +X station at
    // every C (read revolved, it would refuse).
    steady(&world, &mut h, Point3::new(-(r + 150.0), 0.0, z));
    let (path, _) = operation::generate(&world, &contour(), "C wrap").unwrap_or_else(|e| panic!("off the station the rapid is clear: {e}"));
    assert_eq!(path.moves, bare.moves, "the steady changes no move");
}
