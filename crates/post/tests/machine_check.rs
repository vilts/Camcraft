//! Pure machine limits, available without an ECS document (#2585).
//! Provenance: general machine-tool limits and the existing app machine checks.
//! Gate map (M4.4): covers post::machine.

use camcraft_post::cl::{Move, MoveKind, Toolpath};
use camcraft_post::geometry::Point3;
use camcraft_post::machine::{AxisLimits, Machine, MachineCheck, REPORT_CAP, SourceLocation};

fn machine() -> Machine {
    Machine {
        model: "TestMill".into(),
        axes: vec![AxisLimits {
            name: "X".into(),
            feed: Some(100.0),
            travel: Some((-10.0, 10.0)),
            ..Default::default()
        }],
        spindle_min: Some(50.0),
        spindle_max: Some(1000.0),
        max_tool_length: Some(100.0),
        max_tool_diameter: Some(20.0),
        tool_count: Some(1),
        ..Default::default()
    }
}

fn path(to_x: f64, feed: Option<f64>) -> Toolpath {
    Toolpath::new(vec![Move::new(
        MoveKind::Feed,
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(to_x, 0.0, 0.0),
        feed,
    )])
}

#[test]
fn pure_machine_checks_keep_operation_messages_and_nc_line_locations() {
    let machine = machine();
    let mut out = MachineCheck::default();
    out.check_path(&machine, Some(SourceLocation::Operation("Face1")), &path(11.0, Some(200.0)), [0.0; 3], [false; 3], true);
    assert_eq!(out.problems, [
        "Face1: move 0 feeds 200 mm/min, X tops out at 100",
        "Face1: move 0 reaches 11.000 in X, the stroke is -10.000..10.000",
    ]);
    let mut nc = MachineCheck::default();
    nc.check_path(&machine, Some(SourceLocation::NcLine(17)), &path(11.0, None), [0.0; 3], [false; 3], true);
    assert_eq!(nc.problems, ["line 17: move 0 reaches 11.000 in X, the stroke is -10.000..10.000"]);
    assert!(!nc.is_ok());
}

#[test]
fn pure_tool_checks_keep_rpm_sizes_and_distinct_tool_count() {
    let machine = machine();
    let mut out = MachineCheck::default();
    out.check_tool_count(&machine, &["T1", "T1", "T2"]);
    out.check_tool(&machine, Some(SourceLocation::Operation("Face1")), "T1", 101.0, 21.0, 1001.0);
    out.check_tool(&machine, Some(SourceLocation::Operation("Face2")), "T2", 100.0, 20.0, 49.0);
    assert_eq!(out.problems, [
        "program calls 2 tools, the changer holds 1",
        "Face1: tool T1 runs 1001 rpm, above the machine's 1000",
        "Face1: tool T1 is 101.0 mm long, the machine takes 100.0",
        "Face1: tool T1 is 21.0 mm across, the machine takes 20.0",
        "Face2: tool T2 runs 49 rpm, below the machine's 50",
    ]);
}

#[test]
fn pure_checks_keep_unstated_limits_inclusive_edges_and_probe_rpm() {
    let mut out = MachineCheck::default();
    out.check_tool(&machine(), None, "probe", 100.0, 20.0, 0.0);
    out.check_path(&machine(), None, &path(10.0, Some(100.0)), [0.0; 3], [false; 3], true);
    let sparse = Machine::default();
    out.check_tool(&sparse, None, "T1", 1000.0, 1000.0, 10000.0);
    out.check_tool_count(&sparse, &["T1", "T2"]);
    out.check_path(&sparse, None, &path(1000.0, Some(10000.0)), [0.0; 3], [false; 3], true);
    assert!(out.is_ok(), "{:?}", out.problems);
}

#[test]
fn pure_travel_uses_origin_flip_and_keeps_posted_lathe_override() {
    let machine = machine();
    let mut out = MachineCheck::default();
    // Both endpoints shift inside the travel window.
    let shifted = Toolpath::new(vec![Move::new(MoveKind::Rapid, Point3::new(20.0, 0.0, 0.0), Point3::new(21.0, 0.0, 0.0), None)]);
    out.check_path(&machine, None, &shifted, [20.0, 0.0, 0.0], [false; 3], true);
    assert!(out.is_ok());
    let mut asymmetric = machine.clone();
    asymmetric.axes[0].travel = Some((-20.0, 0.0));
    out.check_path(&asymmetric, Some(SourceLocation::Operation("Face1")), &path(11.0, None), [0.0; 3], [true, false, false], true);
    assert!(out.is_ok());
    out.check_path(&machine, None, &path(100.0, Some(200.0)), [0.0; 3], [false; 3], false);
    assert_eq!(out.problems.len(), 1);
    assert!(out.problems[0].contains("feeds 200"));
}

#[test]
fn pure_machine_report_caps_findings_and_counts_the_rest() {
    let machine = machine();
    let mut out = MachineCheck::default();
    for _ in 0..REPORT_CAP + 3 {
        out.check_tool(&machine, None, "T1", 100.0, 20.0, 1001.0);
    }
    assert_eq!(out.problems.len(), REPORT_CAP);
    assert_eq!(out.more, 3);
}
