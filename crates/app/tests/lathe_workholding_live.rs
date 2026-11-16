//! The workholding rpm limit judges the WORK spindle, never a live
//! tool's (#1174). A setup's chuck or collet rating (#1031) holds the
//! spindle that turns the work: a constant-rpm turning section at its
//! stated rpm (`lathe_workholding_rpm` pins those), and a C-axis
//! live-tool section at the most its machine's C axis turns - the
//! lathe post puts the work spindle in C mode there and gives the
//! section's S to the live spindle, so the tool's rpm is not the
//! work's. A cross (radial) drill is a driven tool plunging toward a
//! work held still: the limit does not judge it at all. A C section on
//! a machine stating no C rate cannot be held to the limit, and is
//! refused by name while a limit is stated.
//!
//! Gate map (M4.4): covers app::setup_workholding (the program gate's
//! work-speed split), app::posting (verification_gate).
//! Lane shape (M4.4): all tests here run in seconds.

mod cross_support;

use cross_support::*;

use camcraft_app::commands::{CreateSetup, SelectPost, SetSetupWorkholdingRpm};
use camcraft_app::schema::PostRef;
use camcraft_app::machine::{AxisLimits, LiveToolCodes};
use camcraft_app::posting;

/// The document posts through the Fanuc lathe script (a mill-turn's
/// default is the mill post, whose S drives the tool - #1174).
fn lathe_post(world: &World, h: &mut History) {
    h.execute(world, SelectPost::new(world, PostRef::Named("Fanuc Lathe".into())).expect("the lathe post"));
}

fn chuck_lines(world: &World) -> Vec<String> {
    posting::verification_gate(world).into_iter().filter(|p| p.contains("workholding")).collect()
}

/// The cross-drill bar on a mill-turn, Setup1 stating `chuck` rpm, the
/// drill at 10000 rpm, cross drill CD1 generated.
fn cross_drilled(chuck: Option<f64>) -> World {
    let (world, mut h) = bar_stock_world("mill-turn");
    let fast = Tool { rpm: 10_000.0, flute_len: 50.0, ..catalog_tool("DR-0400") };
    h.execute(&world, camcraft_app::commands::ChangeTool::new(&world, "DR-0400", fast).expect("10000 rpm drill"));
    lathe_post(&world, &mut h);
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    if chuck.is_some() {
        h.execute(&world, SetSetupWorkholdingRpm::new(&world, "Setup1", chuck).expect("chuck limit"));
    }
    create(&world, &mut h, "CD1", cross_params(typed(), None));
    assert_eq!(status(&world, "CD1"), OpStatus::Ok);
    world
}

#[test]
fn a_10000_rpm_cross_drill_is_not_judged_by_a_100_rpm_chuck() {
    let held = cross_drilled(Some(100.0));
    assert!(chuck_lines(&held).is_empty(), "the drill's spindle is not the chuck's: {:?}", chuck_lines(&held));
    // The limit adds nothing to the gate: the same verdicts as no limit.
    assert_eq!(posting::verification_gate(&held), posting::verification_gate(&cross_drilled(None)));
}

/// The test bench's live-tool words - placeholders, no builder's codes.
fn codes() -> LiveToolCodes {
    LiveToolCodes {
        c_axis_on: "M1001".into(),
        c_axis_off: "M1002".into(),
        spindle_on: "M1003".into(),
        spindle_off: "M1004".into(),
    }
}

/// A mill-turn whose C turns at `c` = (feed, rapid) deg/min.
fn mill_turn(c: (Option<f64>, Option<f64>)) -> Machine {
    Machine {
        model: "TestBench mill-turn".into(),
        kind: Some("mill-turn".into()),
        spindle_max: Some(6000.0),
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Y".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits {
                name: "C".into(),
                rotary: true,
                axis: Some([0.0, 0.0, 1.0]),
                feed: c.0,
                rapid: c.1,
                ..Default::default()
            },
        ],
        live_tool: Some(codes()),
        ..Default::default()
    }
}

/// The turned bar standing on the spindle: wall r6, shoulder, neck r5.
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

/// A C-axis rotary contour W1 by a 10000 rpm live ball on machine `m`,
/// Setup1 stating `chuck` rpm - generated.
fn wrapped(m: Machine, chuck: Option<f64>) -> World {
    let world = new_world();
    let mut h = History::default();
    let ball = Tool { rpm: 10_000.0, ..catalog_tool("BN-0600") };
    h.execute(&world, AddTool::new(&world, "BN-0600", ball).expect("tool"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 14.0, dy: 14.0, dz: 34.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(bar())));
    h.execute(&world, LoadMachine::new(&world, "millturn.mch".into(), Some(m)).expect("machine"));
    lathe_post(&world, &mut h);
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    if chuck.is_some() {
        h.execute(&world, SetSetupWorkholdingRpm::new(&world, "Setup1", chuck).expect("chuck limit"));
    }
    let params = OpParams::RotaryContour {
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
    };
    let op = CreateOperation::new(&world, "W1", "BN-0600", SrcRef::Entity("bar".into()), params);
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "W1").expect("generate"));
    assert_eq!(status(&world, "W1"), OpStatus::Ok);
    let path = cache(&world, "W1").expect("a cached path");
    assert!(path.path.moves.iter().any(|m| m.rotary.is_some()), "a C section: its moves state C");
    world
}

/// C at 6000 deg/min feed, 12000 deg/min rapid: the work turns at most
/// 12000 / 360 = 33.3 rpm.
const RATED: (Option<f64>, Option<f64>) = (Some(6000.0), Some(12_000.0));

#[test]
fn a_c_section_is_judged_by_its_c_rate_never_by_the_live_tools_rpm() {
    // 10000 rpm live tool, 100 rpm chuck, C at most 33 rpm: allowed.
    let held = wrapped(mill_turn(RATED), Some(100.0));
    assert!(chuck_lines(&held).is_empty(), "{:?}", chuck_lines(&held));
    assert_eq!(posting::verification_gate(&held), posting::verification_gate(&wrapped(mill_turn(RATED), None)));
    // A 20 rpm collet under a C that turns the work at 33 rpm: refused by name.
    let slow = wrapped(mill_turn(RATED), Some(20.0));
    let lines = chuck_lines(&slow);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let l = &lines[0];
    assert!(l.contains("W1") && l.contains("C axis") && l.contains("33 rpm"), "names the op and the work's speed: {l}");
    assert!(l.contains("12000 degrees per minute"), "names the C rate it read: {l}");
    assert!(l.contains("setup 'Setup1'") && l.contains("workholding maximum of 20 rpm"), "names the setup: {l}");
    assert!(!l.contains("10000"), "never the live tool's rpm: {l}");
}

#[test]
fn a_c_section_on_an_unrated_c_is_refused_only_under_a_stated_limit() {
    let lines = chuck_lines(&wrapped(mill_turn((None, None)), Some(2000.0)));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("W1") && lines[0].contains("states no C rate"), "{}", lines[0]);
    assert!(lines[0].contains("setup 'Setup1'") && lines[0].contains("2000 rpm"), "{}", lines[0]);
    assert!(chuck_lines(&wrapped(mill_turn((None, None)), None)).is_empty(), "no limit stated: nothing to hold");
}
