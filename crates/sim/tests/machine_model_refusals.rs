//! The generated machine model refuses degenerate input by name (#2584):
//! non-finite numbers, zero or reversed travel, a zero axis, a broken
//! parent chain, and a pose that does not state each joint one time.
//! Each refusal is a clean error with the part id and the key, never a
//! panic and never a body of a guessed size.
//!
//! Gate map (M4.4): covers camcraft_sim::machine_model.

use camcraft_sim::machine_model::{
    self, JointKind, MachineModel, MachineSpec, PartSpec, SpindleSpec, ToolStationSpec, MAX_DIM, MAX_PARTS,
};

fn spec() -> MachineSpec {
    MachineSpec {
        parts: vec![
            PartSpec::new("base", None, JointKind::Fixed),
            PartSpec {
                axis: Some([-1.0, 0.0, 0.0]),
                travel: Some((-400.0, 400.0)),
                ..PartSpec::new("x", Some(0), JointKind::Linear)
            },
            PartSpec {
                axis: Some([0.0, 0.0, 1.0]),
                travel: Some((-300.0, 300.0)),
                ..PartSpec::new("z", Some(0), JointKind::Linear)
            },
            PartSpec {
                axis: Some([1.0, 0.0, 0.0]),
                travel: Some((-120.0, 120.0)),
                ..PartSpec::new("a", Some(2), JointKind::Rotary)
            },
            PartSpec {
                spindle: Some(SpindleSpec { diameter: Some(100.0), length: Some(150.0) }),
                tool_station: Some(ToolStationSpec { max_tool_length: Some(250.0), max_tool_diameter: Some(80.0) }),
                ..PartSpec::new("head", Some(3), JointKind::Fixed)
            },
        ],
    }
}

/// The refusal of `spec()` after `edit`, which must name each of `words`.
#[track_caller]
fn refused(edit: impl FnOnce(&mut MachineSpec), words: &[&str]) {
    let mut s = spec();
    edit(&mut s);
    let why = machine_model::generate(&s).expect_err("degenerate input must refuse");
    assert!(why.starts_with("machine model: "), "{why}");
    for w in words {
        assert!(why.contains(w), "the refusal does not name {w:?}: {why}");
    }
}

fn model() -> MachineModel {
    machine_model::generate(&spec()).expect("the plain input generates")
}

#[track_caller]
fn pose_refused(values: &[(&str, f64)], words: &[&str]) {
    let why = model().pose(values).expect_err("a bad pose must refuse");
    assert!(why.starts_with("machine model: pose: "), "{why}");
    for w in words {
        assert!(why.contains(w), "the refusal does not name {w:?}: {why}");
    }
}

#[test]
fn the_plain_input_generates_and_poses() {
    let m = model();
    assert_eq!(m.bodies.len(), 4, "x, z, spindle, tool envelope (no Y travel: no table block)");
    m.pose(&[("x", 1.0), ("z", 2.0), ("a", 3.0)]).expect("a full pose");
}

#[test]
fn travel_must_be_finite_with_max_over_min() {
    refused(|s| s.parts[1].travel = Some((0.0, 0.0)), &["part x", "travel", "max more than min"]);
    refused(|s| s.parts[1].travel = Some((10.0, -10.0)), &["part x", "travel", "max more than min"]);
    refused(|s| s.parts[2].travel = Some((f64::NAN, 300.0)), &["part z", "travel", "finite"]);
    refused(|s| s.parts[2].travel = Some((-300.0, f64::INFINITY)), &["part z", "travel", "finite"]);
    refused(|s| s.parts[3].travel = Some((90.0, 90.0)), &["part a", "travel", "max more than min"]);
    refused(|s| s.parts[1].travel = Some((-MAX_DIM, MAX_DIM)), &["part x", "travel", "over"]);
    refused(|s| s.parts[0].travel = Some((0.0, 10.0)), &["part base", "travel", "fixed"]);
}

#[test]
fn a_joint_axis_must_be_a_finite_direction() {
    refused(|s| s.parts[1].axis = None, &["part x", "axis", "states none"]);
    refused(|s| s.parts[3].axis = None, &["part a", "axis", "states none"]);
    refused(|s| s.parts[1].axis = Some([0.0, 0.0, 0.0]), &["part x", "axis", "zero direction"]);
    refused(|s| s.parts[3].axis = Some([f64::NAN, 0.0, 0.0]), &["part a", "axis", "finite"]);
    refused(|s| s.parts[2].axis = Some([0.0, f64::INFINITY, 1.0]), &["part z", "axis", "finite"]);
}

#[test]
fn positions_must_be_finite() {
    refused(|s| s.parts[3].pivot = Some([0.0, f64::NAN, 0.0]), &["part a", "pivot", "finite"]);
    refused(|s| s.parts[2].offset = [f64::INFINITY, 0.0, 0.0], &["part z", "offset", "finite"]);
    refused(|s| s.parts[2].offset = [0.0, 0.0, 2.0 * MAX_DIM], &["part z", "offset", "over"]);
}

#[test]
fn spindle_and_tool_station_sizes_must_be_real_lengths() {
    let spindle = |d, l| Some(SpindleSpec { diameter: d, length: l });
    refused(|s| s.parts[4].spindle = spindle(Some(0.0), Some(150.0)), &["part head", "spindle diameter", "more than zero"]);
    refused(|s| s.parts[4].spindle = spindle(Some(-5.0), Some(150.0)), &["part head", "spindle diameter", "more than zero"]);
    refused(|s| s.parts[4].spindle = spindle(Some(100.0), Some(f64::NAN)), &["part head", "spindle length", "finite"]);
    refused(|s| s.parts[4].spindle = spindle(Some(2.0 * MAX_DIM), Some(150.0)), &["part head", "spindle diameter", "over"]);
    let station = |l, d| Some(ToolStationSpec { max_tool_length: l, max_tool_diameter: d });
    refused(|s| s.parts[4].tool_station = station(Some(0.0), Some(80.0)), &["part head", "max tool length", "more than zero"]);
    refused(|s| s.parts[4].tool_station = station(Some(250.0), Some(f64::INFINITY)), &["part head", "max tool diameter", "finite"]);
}

#[test]
fn the_part_list_must_be_a_tree_with_unique_ids() {
    refused(|s| s.parts.clear(), &["no part"]);
    refused(|s| s.parts[2].id = "x".to_owned(), &["part x", "two parts"]);
    refused(|s| s.parts[2].id = "  ".to_owned(), &["index 2", "id"]);
    refused(|s| s.parts[1].parent = Some(1), &["part x", "parent"]);
    refused(|s| s.parts[1].parent = Some(4), &["part x", "parent"]);
    refused(|s| s.parts[1].parent = Some(99), &["part x", "parent"]);
    let crowd = |s: &mut MachineSpec| {
        s.parts.extend((0..MAX_PARTS).map(|i| PartSpec::new(&format!("p{i}"), Some(0), JointKind::Fixed)));
    };
    refused(crowd, &["parts", "over"]);
}

#[test]
fn a_pose_states_each_joint_one_time_with_a_finite_value() {
    pose_refused(&[("x", 0.0), ("z", 0.0)], &["part a", "no axis value"]);
    pose_refused(&[("x", 0.0), ("z", 0.0), ("a", 0.0), ("b", 0.0)], &["no part b"]);
    pose_refused(&[("x", 0.0), ("z", 0.0), ("a", 0.0), ("x", 1.0)], &["part x", "two values"]);
    pose_refused(&[("x", 0.0), ("z", 0.0), ("a", 0.0), ("head", 1.0)], &["part head", "fixed"]);
    pose_refused(&[("x", f64::NAN), ("z", 0.0), ("a", 0.0)], &["part x", "finite"]);
    pose_refused(&[("x", 0.0), ("z", 0.0), ("a", f64::INFINITY)], &["part a", "finite"]);
    pose_refused(&[("x", 0.0), ("z", 1e300), ("a", 0.0)], &["part z", "over"]);
}
