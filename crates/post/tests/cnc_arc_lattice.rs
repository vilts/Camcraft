//! #3442: the stream writer's arc law on the micrometre lattice. An arc
//! whose in-plane ends round to the same micrometre and whose true sweep
//! is half a turn or less is written as a `Feed` to its rounded end; the
//! reader would take the rounded record as a full turn. A near-full turn,
//! a stated full turn and an arc that the lattice can show keep their arc
//! record.
//!
//! #3474: the same law for rounded ends that differ on one ray from the
//! rounded centre (the same angle about it). The reader sees no sweep in
//! such a record, so the writer gives the short arc as a `Feed`.
//!
//! Provenance: our own invariants (`cnc_arc_lattice.rs`).
//!
//! Gate map (M4.4): covers camcraft_post::post::cnc,
//! camcraft_post::post::cnc_arc_lattice.

use camcraft_post::cl::{ArcPlane, MoveKind};
use camcraft_post::post::cnc::{self, MoveRec, Record};

fn mv(x: f64, y: f64, z: f64) -> Record {
    Record::Move(MoveRec { kind: MoveKind::Feed, x, y, z, feed: Some(500.0), center: None, ccw: false, plane: ArcPlane::Xy, abc: None })
}

fn arc(to: [f64; 3], center: (f64, f64), ccw: bool, plane: ArcPlane) -> Record {
    Record::Move(MoveRec { kind: MoveKind::Feed, x: to[0], y: to[1], z: to[2], feed: Some(500.0), center: Some(center), ccw, plane, abc: None })
}

/// The last line of the stream: the arc record as written.
fn written(recs: &[Record]) -> String {
    cnc::serialize(recs).lines().last().unwrap_or("").to_string()
}

/// A fillet-sized arc: R50, a 0.09 um chord (#3463), from (0, 0) about (0, 50).
fn fillet_end(r: f64, chord: f64) -> [f64; 3] {
    let t = chord / r;
    [r * t.sin(), r - r * t.cos(), 0.0]
}

#[test]
fn a_short_arc_under_one_micrometre_is_written_as_a_feed() {
    let end = fillet_end(50.0, 0.00009);
    let line = written(&[mv(0.0, 0.0, 0.0), arc(end, (0.0, 50.0), true, ArcPlane::Xy)]);
    assert_eq!(line, "Feed X 0 Y 0 Z 0 F 500000");
    // A short helix keeps its Z.
    let helix = written(&[mv(0.0, 0.0, 0.0), arc([end[0], end[1], -1.0], (0.0, 50.0), true, ArcPlane::Xy)]);
    assert_eq!(helix, "Feed X 0 Y 0 Z -1000 F 500000");
    // G18: the same arc in XZ, CW about (X 0, Z 50) is the short one there.
    let xz = written(&[mv(0.0, 0.0, 0.0), arc([0.00009, 0.0, 0.0], (0.0, 50.0), false, ArcPlane::Xz)]);
    assert_eq!(xz, "Feed X 0 Y 0 Z 0 F 500000");
}

#[test]
fn a_near_full_turn_a_stated_full_turn_and_a_visible_arc_keep_their_arc_record() {
    let end = fillet_end(50.0, 0.00009);
    // The same end points the other way round: a turn 0.09 um short of full.
    let near = written(&[mv(0.0, 0.0, 0.0), arc(end, (0.0, 50.0), false, ArcPlane::Xy)]);
    assert_eq!(near, "Arc CW X 0 Y 0 Z 0 F 500000 I 0 J 50000");
    let full = written(&[mv(0.0, 0.0, 0.0), arc([0.0, 0.0, 0.0], (0.0, 50.0), true, ArcPlane::Xy)]);
    assert_eq!(full, "Arc CCW X 0 Y 0 Z 0 F 500000 I 0 J 50000");
    let visible = written(&[mv(0.0, 0.0, 0.0), arc(fillet_end(50.0, 0.002), (0.0, 50.0), true, ArcPlane::Xy)]);
    assert_eq!(visible, "Arc CCW X 2 Y 0 Z 0 F 500000 I 0 J 50000");
    let xz = written(&[mv(0.0, 0.0, 0.0), arc([0.00009, 0.0, 0.0], (0.0, 50.0), true, ArcPlane::Xz)]);
    assert_eq!(xz, "Arc CCW X 0 Y 0 Z 0 F 500000 I 0 K 50000");
    // With no known start (the first record) the writer does not guess.
    let first = written(&[arc(end, (0.0, 50.0), true, ArcPlane::Xy)]);
    assert_eq!(first, "Arc CCW X 0 Y 0 Z 0 F 500000 I 0 J 50000");
}

/// #3474: centre (10, 5), radius 10.51 um, a sweep of 0.047 rad. The ends
/// round to X 10011 and X 10010 at Y 5000: one ray from the centre.
#[test]
fn a_short_arc_whose_rounded_ends_share_one_angle_is_written_as_a_feed() {
    let e = [10.010498, 5.000494, 0.0];
    let line = written(&[mv(10.01051, 5.0, 0.0), arc(e, (10.0, 5.0), true, ArcPlane::Xy)]);
    assert_eq!(line, "Feed X 10010 Y 5000 Z 0 F 500000");
    // A short helix keeps its Z.
    let helix = written(&[mv(10.01051, 5.0, 0.0), arc([e[0], e[1], -1.0], (10.0, 5.0), true, ArcPlane::Xy)]);
    assert_eq!(helix, "Feed X 10010 Y 5000 Z -1000 F 500000");
    // An end that rounds onto the centre: the radius is under the lattice.
    let on_centre = written(&[mv(10.0006, 5.0, 0.0), arc([10.00044, 5.00041, 0.0], (10.0, 5.0), true, ArcPlane::Xy)]);
    assert_eq!(on_centre, "Feed X 10000 Y 5000 Z 0 F 500000");
    // G18 (a lathe meridian): the same arc in XZ about (X 12, Z -1), CW.
    let xz = written(&[mv(12.01051, 0.0, -1.0), arc([12.010498, 0.0, -0.999506], (12.0, -1.0), false, ArcPlane::Xz)]);
    assert_eq!(xz, "Feed X 12010 Y 0 Z -1000 F 500000");
    // G19: in YZ about (Y 10, Z 5), CCW.
    let yz = written(&[mv(0.0, 10.01051, 5.0), arc([0.0, 10.010498, 5.000494], (10.0, 5.0), true, ArcPlane::Yz)]);
    assert_eq!(yz, "Feed X 0 Y 10010 Z 5000 F 500000");
}

/// #3474: the arcs that the same-angle law must not touch.
#[test]
fn a_long_turn_an_opposite_end_and_a_visible_arc_keep_their_arc_record_on_one_line() {
    // The same end points the other way round: 0.047 rad short of a circle.
    let long = written(&[mv(10.01051, 5.0, 0.0), arc([10.010498, 5.000494, 0.0], (10.0, 5.0), false, ArcPlane::Xy)]);
    assert_eq!(long, "Arc CW X 10010 Y 5000 Z 0 F 500000 I 10000 J 5000");
    // A half turn: the ends are on one line through the centre, not one ray.
    let half = written(&[mv(10.01051, 5.0, 0.0), arc([9.98949, 5.0, 0.0], (10.0, 5.0), true, ArcPlane::Xy)]);
    assert_eq!(half, "Arc CCW X 9989 Y 5000 Z 0 F 500000 I 10000 J 5000");
    // An arc that the lattice shows: the rounded ends are at two angles.
    let seen = written(&[mv(11.0, 5.0, 0.0), arc([10.0, 6.0, 0.0], (10.0, 5.0), true, ArcPlane::Xy)]);
    assert_eq!(seen, "Arc CCW X 10000 Y 6000 Z 0 F 500000 I 10000 J 5000");
    // True ends on one ray with no sweep at all: no short arc to read.
    let ray = written(&[mv(11.235, 20.0, 0.0), arc([11.234, 20.0, 0.0], (10.0, 20.0), true, ArcPlane::Xy)]);
    assert_eq!(ray, "Arc CCW X 11234 Y 20000 Z 0 F 500000 I 10000 J 20000");
}
