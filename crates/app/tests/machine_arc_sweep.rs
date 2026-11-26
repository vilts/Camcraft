//! #2710: the machine travel check judges the points that an arc really
//! sweeps - its endpoints, plus each quarter point of its circle only
//! when the sweep passes it. A shallow arc of a 22 m radius inside the
//! stroke posts; the same endpoints the long way round, an arc that
//! bulges past the stroke, a full circle and an arc whose endpoints
//! post as one point keep their refusal.
//!
//! Provenance: RS-274 circular interpolation (G2 clockwise, G3
//! counterclockwise seen from the plane's +normal); every number is our
//! own fixture, the large radius from the issue's report.
//!
//! Gate map (#2710): covers app::machine, cl::moves, cl::plane.

use camcraft_app::cl::{ArcPlane, Move, MoveKind, Toolpath as ClPath};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::machine::{self, AxisLimits, Machine, MachineCheck};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::{Operation, ToolDef, Toolpath};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0800-4FL";

/// A 3-axis mill that states its travel and nothing else: X and Y
/// -3000..3000 (the issue's machine), Z -100..100.
fn mill() -> Machine {
    let axis = |name: &str, lo: f64, hi: f64| AxisLimits { name: name.into(), travel: Some((lo, hi)), ..Default::default() };
    Machine {
        model: "arc sweep mill".into(),
        axes: vec![axis("X", -3000.0, 3000.0), axis("Y", -3000.0, 3000.0), axis("Z", -100.0, 100.0)],
        ..Default::default()
    }
}

/// The machine's judgment of one cached move.
fn judged(mv: Move) -> MachineCheck {
    let w = new_world();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    model::spawn(&w, TOOL).entity_view(&w).set(ToolDef { tool, number: None });
    let e = model::spawn(&w, "Cached");
    e.entity_view(&w)
        .set(Operation {
            tool: TOOL.into(),
            src: SrcRef::Stock,
            params: OpParams::Face {
                holes: Default::default(), tilt: None, blend_turns: false, blend_radius: None,
                stepover: 1.0, clearance_z: Height::fixed(10.0), cut_z: Height::fixed(0.0),
                top_z: None, stepdown: None, feed: 600.0, plunge_feed: 100.0, climb: true,
                picks: Default::default(),
            },
            stamp: None, setup: None, area: Default::default(), coolant: Default::default(),
            after: Default::default(), suppressed: false,
        })
        .set(Toolpath::new(ClPath { moves: vec![mv], ..Default::default() }, Vec::new()));
    machine::check_ops(&w, &mill(), &[("Cached".into(), e)]).expect("the check runs")
}

fn feed(a: [f64; 3], b: [f64; 3]) -> Move {
    Move::new(MoveKind::Feed, Point3::new(a[0], a[1], a[2]), Point3::new(b[0], b[1], b[2]), Some(600.0))
}

/// The issue's move 31: a 100.4 mm chord on a 22170 mm radius, 0.259
/// degrees of sweep. Its circle's center stands 22 m up the Y axis.
fn shallow(ccw: bool) -> Move {
    feed([100.0, 0.0, 0.0], [200.4, 0.0, 0.0]).with_arc(Point2::new(150.2, 22_169.943_17), ccw)
}

#[test]
fn a_shallow_arc_of_a_large_radius_inside_the_stroke_passes() {
    let r = judged(shallow(true));
    assert!(r.problems.is_empty() && r.more == 0, "the arc stays within 0.06 mm of its chord: {r:?}");
}

#[test]
fn the_same_endpoints_the_long_way_round_are_refused_on_their_own_side() {
    let r = judged(shallow(false));
    for reach in ["22320.200 in X", "-22019.800 in X", "44339.943 in Y"] {
        assert!(r.problems.iter().any(|p| p.contains(reach)), "{reach}: {r:?}");
    }
    assert_eq!(r.problems.len(), 5, "three quarter points, two of them out in X and in Y: {r:?}");
}

#[test]
fn an_arc_is_judged_on_the_side_that_it_sweeps() {
    // A half circle of radius 10 about X2995: G3 bulges to X3005, past
    // the stroke; G2 turns back through X2985.
    let half = |ccw| feed([2995.0, -10.0, 0.0], [2995.0, 10.0, 0.0]).with_arc(Point2::new(2995.0, 0.0), ccw);
    let out = judged(half(true));
    assert_eq!(out.problems.len(), 1, "{out:?}");
    assert!(out.problems[0].contains("reaches 3005.000 in X"), "{out:?}");
    let back = judged(half(false));
    assert!(back.problems.is_empty(), "the G2 half stays inside: {back:?}");
}

#[test]
fn a_quarter_point_on_an_endpoint_counts() {
    // The arc ends exactly on its X+ quarter point, past the stroke.
    let r = judged(feed([2995.0, -10.0, 0.0], [3005.0, 0.0, 0.0]).with_arc(Point2::new(2995.0, 0.0), true));
    assert!(r.problems.iter().any(|p| p.contains("reaches 3005.000 in X")), "{r:?}");
}

#[test]
fn a_full_circle_keeps_all_four_quarter_points() {
    // From == to: the block posts as a whole turn. Radius 3005 about
    // the origin, started at 45 degrees: all four quarters are out.
    let s = 3005.0 * std::f64::consts::FRAC_1_SQRT_2;
    for ccw in [true, false] {
        let r = judged(feed([s, s, 0.0], [s, s, 0.0]).with_arc(Point2::new(0.0, 0.0), ccw));
        for reach in ["reaches 3005.000 in X", "reaches -3005.000 in X", "reaches 3005.000 in Y", "reaches -3005.000 in Y"] {
            assert!(r.problems.iter().any(|p| p.contains(reach)), "{reach} (ccw {ccw}): {r:?}");
        }
    }
}

#[test]
fn endpoints_that_post_as_one_point_keep_all_four_quarter_points() {
    // 0.4 um apart: the NC words of the two endpoints are equal, and a
    // control reads equal endpoints as a whole turn.
    let r = judged(feed([2995.0, -10.0, 0.0], [2995.0004, -10.0, 0.0]).with_arc(Point2::new(2995.0, 0.0), true));
    assert!(r.problems.iter().any(|p| p.contains("reaches 3005.000 in X")), "{r:?}");
}

#[test]
fn endpoints_that_round_to_one_inch_point_keep_all_four_quarter_points() {
    // 2 um apart: both endpoints post as X117.9134 Y-0.3937 with
    // four-decimal inch words. The control reads a whole turn.
    let r = judged(
        feed([2994.9995, -10.0, 0.0], [2995.0015, -10.0, 0.0])
            .with_arc(Point2::new(2995.0005, 0.0), true),
    );
    assert!(
        r.problems
            .iter()
            .any(|p| p.contains("reaches 3005.001 in X")),
        "{r:?}"
    );
}

#[test]
fn an_arc_with_no_sweep_to_read_keeps_all_four_quarter_points() {
    // The endpoints stand on one ray from the center: the sweep reads
    // zero and names no side. Radius 20 about X-2990 reaches X-3010.
    let r = judged(feed([-2980.0, 0.0, 0.0], [-2970.0, 0.0, 0.0]).with_arc(Point2::new(-2990.0, 0.0), true));
    assert!(r.problems.iter().any(|p| p.contains("reaches -3010.000 in X")), "{r:?}");
}

#[test]
fn a_vertical_plane_arc_is_judged_on_the_side_that_it_sweeps() {
    // G18, center (X0, Z95), radius 10: G3 from X-10 to X10 passes
    // Z105, past the Z stroke; G2 passes Z85.
    let half = |ccw| feed([-10.0, 0.0, 95.0], [10.0, 0.0, 95.0]).with_arc_in(Point2::new(0.0, 95.0), ccw, ArcPlane::Xz);
    let out = judged(half(true));
    assert_eq!(out.problems.len(), 1, "{out:?}");
    assert!(out.problems[0].contains("reaches 105.000 in Z"), "{out:?}");
    let back = judged(half(false));
    assert!(back.problems.is_empty(), "the G2 half stays inside: {back:?}");
}
