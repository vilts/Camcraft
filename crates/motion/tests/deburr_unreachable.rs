//! #1889 deburr unreachable-edge policy: an edge that climbs steeper
//! than the cone's flank is out of reach of a vertical cone. The policy
//! says what the ride does then - `SkipWithNote` (the default) leaves
//! the edge and names the count in a note, `Refuse` stops the ride by
//! name. An edge the programmed window leaves out is not a candidate:
//! the window is the machinist's own choice, not a reach limit.
//!
//! Provenance: our own tests of machine-tool handbook cone geometry (the
//! flank-slope law `deburr.rs` states, #303); no vendor material.
//!
//! Gate map (#1889): covers camcraft_motion::finishing3d (the deburr
//! kernel's unreachable-edge policy).
//! Lane shape: every law here runs in seconds; nothing is ignored.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::finishing3d::{deburr, DeburrParams, UnreachableEdges};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A 90-deg cone, a 2-wide break (the tip stands 2 under the edge),
/// the tip window -4..-2: an edge at z = 0 is the one level in reach.
fn params(unreachable: UnreachableEdges) -> DeburrParams {
    DeburrParams {
        width: 2.0 * std::f64::consts::SQRT_2, // #1890: the chord of a 2 mm setback
        tip_angle_deg: 90.0,
        clearance_z: 20.0,
        top_z: -2.0,
        bottom_z: -4.0,
        feed: 500.0,
        plunge_feed: 100.0,
        unreachable,
        min_edge_angle_deg: 0.0,
        exclude_holes: false,
        hole_min_diameter: 0.0,
        hole_max_diameter: 0.0,
        sizing: camcraft_motion::finishing3d::DeburrBreak::default(),
    }
}

/// A vertical curtain x 0..10, z -10..0 at y = 0: its top rim is level
/// and in the window, its two side rims are vertical (out of reach).
fn curtain() -> Vec<Triangle> {
    vec![
        Triangle::new(p3(0.0, 0.0, -10.0), p3(10.0, 0.0, -10.0), p3(10.0, 0.0, 0.0)),
        Triangle::new(p3(0.0, 0.0, -10.0), p3(10.0, 0.0, 0.0), p3(0.0, 0.0, 0.0)),
    ]
}

/// A level 10 x 4 patch at height `z`: every rim edge is level.
fn patch(z: f64) -> Vec<Triangle> {
    vec![
        Triangle::new(p3(0.0, 0.0, z), p3(10.0, 0.0, z), p3(10.0, 4.0, z)),
        Triangle::new(p3(0.0, 0.0, z), p3(10.0, 4.0, z), p3(0.0, 4.0, z)),
    ]
}

fn feeds(t: &camcraft_motion::cl::Toolpath) -> Vec<Point3> {
    t.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to).collect()
}

#[test]
fn skip_with_note_is_the_default_and_names_the_skipped_count() {
    assert_eq!(UnreachableEdges::default(), UnreachableEdges::SkipWithNote);
    let t = deburr(&curtain(), &params(UnreachableEdges::SkipWithNote)).expect("the level rim is in reach");
    assert!(t.notes.iter().any(|n| n.contains("2 edges climb steeper") && n.contains("skipped")), "{:?}", t.notes);
    let cut = feeds(&t);
    assert!(!cut.is_empty(), "the level rim rides");
    for c in &cut {
        assert!((c.z + 2.0).abs() < 1e-9, "only the level rim rides, the tip 2 under it: {c:?}");
    }
}

#[test]
fn refuse_stops_the_ride_by_name_when_an_edge_is_out_of_reach() {
    let e = deburr(&curtain(), &params(UnreachableEdges::Refuse)).expect_err("two rims are out of reach");
    let GenError::Invalid(msg) = &e else {
        panic!("a refusal by name is Invalid, got {e:?}");
    };
    assert!(msg.contains("deburr"), "{msg}");
    assert!(msg.contains("2 edges climb steeper"), "the refusal names the count: {msg}");
    assert!(msg.contains("Refuse") && msg.contains("Skip with note"), "it names the setting and the way out: {msg}");
}

#[test]
fn refuse_passes_when_every_candidate_edge_is_in_reach() {
    let skip = deburr(&patch(0.0), &params(UnreachableEdges::SkipWithNote)).expect("level rim");
    let refuse = deburr(&patch(0.0), &params(UnreachableEdges::Refuse)).expect("nothing is out of reach");
    assert!(refuse.notes.is_empty(), "{:?}", refuse.notes);
    assert_eq!(skip.moves, refuse.moves, "the policy changes no move when every edge is in reach");
    assert_eq!(feeds(&refuse).len(), 4, "the closed rim rides whole");
}

#[test]
fn refuse_does_not_count_an_edge_the_window_leaves_out() {
    // A second level patch far under the window: the window clips its
    // rim whole. That is the programmed window, not a reach failure.
    let mut mesh = patch(0.0);
    mesh.extend(patch(-50.0));
    let t = deburr(&mesh, &params(UnreachableEdges::Refuse)).expect("the window is not a reach limit");
    for c in feeds(&t) {
        assert!((c.z + 2.0).abs() < 1e-9, "only the patch in the window rides: {c:?}");
    }
}

#[test]
fn refuse_names_reach_before_empty_when_every_candidate_is_out_of_reach() {
    // Only the vertical side rims stand in this window (tip -9..-5).
    let p = DeburrParams { top_z: -5.0, bottom_z: -9.0, ..params(UnreachableEdges::Refuse) };
    let e = deburr(&curtain(), &p).expect_err("no edge is in reach");
    assert!(matches!(&e, GenError::Invalid(m) if m.contains("2 edges climb steeper")), "{e:?}");
    // The default still answers Empty by name, as before.
    let p = DeburrParams { unreachable: UnreachableEdges::SkipWithNote, ..p };
    assert!(matches!(deburr(&curtain(), &p), Err(GenError::Empty(_))));
}

#[test]
fn active_edge_filters_preserve_the_unreachable_refusal() {
    let p = DeburrParams {
        min_edge_angle_deg: 40.0,
        exclude_holes: true,
        hole_min_diameter: 2.0,
        hole_max_diameter: 9.0,
        ..params(UnreachableEdges::Refuse)
    };
    let e = deburr(&curtain(), &p).expect_err("the unfiltered vertical rims are out of reach");
    assert!(matches!(&e, GenError::Invalid(m) if m.contains("2 edges climb steeper") && m.contains("Refuse")), "{e:?}");
    let skip = DeburrParams { unreachable: UnreachableEdges::SkipWithNote, ..p };
    let t = deburr(&curtain(), &skip).expect("the level rim remains in reach with active filters");
    assert!(!feeds(&t).is_empty());
    assert!(t.notes.iter().any(|n| n.contains("2 edges climb steeper") && n.contains("skipped")), "{:?}", t.notes);
}
