//! M13.5 B rotary axis-letter tests at the app level: the rotary
//! kinds read their letter off the MOUNTED machine's rotary
//! inventory. What "right" means here: a Rotary Parallel over a bar
//! laid along world X generates on a mill stating an A trunnion
//! about X; the same op refuses by name on a mill whose only rotary
//! is a B about Y; a bar standing on the spindle of a mill-turn
//! (its C about Z) is admitted since issue #33 and its path states
//! the C angle on every move (the C law itself lives in
//! `c_axis_live.rs`); an unmounted machine checks nothing. The
//! letter law itself is unit-pinned in `operation::rotary_gate`;
//! this file is the document.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the rotary arms'
//! letter door), camcraft_app::machine, post::machine (the rotary inventory),
//! camcraft_app::commands (LoadMachine, Create/GenerateOperation).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

const TAU: f64 = std::f64::consts::TAU;

/// The turned bar's meridian (rho, s): wall r6, shoulder, neck r5.
const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

/// The bar along `axis` (0 = world X, laid; 2 = world Z, standing).
fn bar(axis: usize) -> Vec<Triangle> {
    let pt = |rho: f64, s: f64, j: usize| {
        let th = TAU * j as f64 / 16.0;
        match axis {
            0 => Point3::new(s, rho * th.cos(), rho * th.sin()),
            _ => Point3::new(rho * th.cos(), rho * th.sin(), s),
        }
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

fn rotary(name: &str, axis: [f64; 3]) -> AxisLimits {
    AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() }
}

fn machine(word: &str, axes: Vec<AxisLimits>) -> Machine {
    Machine { model: format!("TestBench {word}"), kind: Some(word.into()), axes, ..Default::default() }
}

fn world(axis: usize, machine: Option<Machine>) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    // A tight box about either stance of the bar: the rotary verdict's
    // field rides the stock's skin, so a loose block is pure time.
    let (dx, dz) = if axis == 0 { (34.0, 14.0) } else { (14.0, 34.0) };
    let stock = SetStock::new(&world, StockShape::Box { dx, dy: 14.0, dz }, Location::default());
    h.execute(&world, stock.expect("stock"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(bar(axis))));
    if let Some(m) = machine {
        h.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    }
    world
}

fn op() -> Operation {
    Operation {
        tool: "BN-0600".into(),
        src: SrcRef::Entity("bar".into()),
        params: OpParams::RotaryParallel {
            tolerance: 0.1,
            stepover: Some(2.0),
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
        },
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

#[test]
fn a_table_trunnion_on_the_bars_line_admits_the_rotary_op() {
    // One generation only - a rotary verdict is the debug lane's
    // slowest minute. The unmounted case (checks nothing) is the
    // rotary suites' own ground: none of them mounts a machine.
    let w = world(0, Some(machine("mill", vec![rotary("A", [1.0, 0.0, 0.0])])));
    operation::generate(&w, &op(), "").expect("the A trunnion holds the laid bar");
}

#[test]
fn a_machine_without_the_bars_line_refuses_by_name() {
    let w = world(0, Some(machine("mill", vec![rotary("B", [0.0, 1.0, 0.0])])));
    let e = operation::generate(&w, &op(), "").expect_err("no rotary about X");
    let msg = format!("{e}");
    assert!(msg.contains("states no rotary on that line") && msg.contains("B"), "{msg}");
}

#[test]
fn the_mill_turn_spindle_as_c_axis_answers_c_and_stamps_it() {
    // A standing bar is the contour's stance (the parallel lays it down).
    let w = world(2, Some(machine("mill-turn", vec![rotary("C", [0.0, 0.0, 1.0])])));
    let mut o = op();
    if let OpParams::RotaryParallel { tolerance, clearance_z, retract_z, top_z, bottom_z, feed, plunge_feed, climb, lead_deg, lag_deg, max_tilt_deg, .. } = o.params.clone() {
        o.params = OpParams::RotaryContour { tolerance, clearance_z, retract_z, top_z, bottom_z, feed, plunge_feed, climb, lead_deg, lag_deg, max_tilt_deg };
    }
    let (path, _) = operation::generate(&w, &o, "").expect("the spindle's C holds the standing bar");
    assert!(path.moves.iter().all(|m| m.rotary.is_some()), "every move states its C");
}
