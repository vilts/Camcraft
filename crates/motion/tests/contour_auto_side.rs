//! #1933: the Side Auto law of a picked contour ring reads the part's
//! section at the ring's own level. Material on one side only decides;
//! material on both sides, on neither, or probes that disagree give
//! `Undecided` with a sentence that asks for a named side - never a guess.
//!
//! Gate map (#1933): covers camcraft_motion::machining2d (contour_auto).
//!
//! Provenance: hand-built boxes; the 2.5D profiling rule of published
//! machining practice. No vendor material consulted.

use std::ops::Range;

use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{cube, Point3, Triangle};
use camcraft_motion::machining2d::{auto_side, AutoSide};

/// The square ring of half-width `h` about the origin.
fn square(h: f64) -> Region {
    let pts = vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)];
    Region::new(vec![Loop::new(pts).expect("loop")]).expect("region")
}

/// Boxes as separate bodies (they may touch face to face).
fn boxes(b: &[([f64; 3], [f64; 3])]) -> (Vec<Triangle>, Vec<Range<usize>>) {
    let (mut f, mut r) = (Vec::new(), Vec::new());
    for (lo, hi) in b {
        let s = f.len();
        f.extend(cube(Point3::new(lo[0], lo[1], lo[2]), Point3::new(hi[0], hi[1], hi[2])).facets.iter().copied());
        r.push(s..f.len());
    }
    (f, r)
}

/// A frame of four boxes around the square of half-width 10, out to 15,
/// standing from `z0` to `z1`.
fn frame(z0: f64, z1: f64) -> Vec<([f64; 3], [f64; 3])> {
    vec![
        ([-15.0, 10.0, z0], [15.0, 15.0, z1]),
        ([-15.0, -15.0, z0], [15.0, -10.0, z1]),
        ([10.0, -10.0, z0], [15.0, 10.0, z1]),
        ([-15.0, -10.0, z0], [-10.0, 10.0, z1]),
    ]
}

#[test]
fn the_rim_of_a_block_reads_outside_and_a_frame_opening_reads_inside() {
    let (f, b) = boxes(&[([-10.0, -10.0, 0.0], [10.0, 10.0, 10.0])]);
    assert_eq!(auto_side(&square(10.0), &f, &b, || None, 10.0, 10.0).expect("law"), AutoSide::Outside);
    let (f, b) = boxes(&frame(0.0, 10.0));
    assert_eq!(auto_side(&square(10.0), &f, &b, || None, 10.0, 10.0).expect("law"), AutoSide::Inside);
    // A wall patch spans the full height: the probe under its top decides.
    assert_eq!(auto_side(&square(10.0), &f, &b, || None, 0.0, 10.0).expect("law"), AutoSide::Inside);
}

#[test]
fn a_floor_rim_decides_from_above() {
    // A frame on a slab: the opening's floor at Z 4. Under the floor rim
    // both sides are metal; just above it the opening is air - Inside.
    let mut parts = frame(4.0, 10.0);
    parts.push(([-15.0, -15.0, 0.0], [15.0, 15.0, 4.0]));
    let (f, b) = boxes(&parts);
    assert_eq!(auto_side(&square(10.0), &f, &b, || None, 4.0, 4.0).expect("law"), AutoSide::Inside);
}

#[test]
fn a_ring_mid_solid_is_undecided_by_name() {
    let (f, b) = boxes(&[([-20.0, -20.0, 0.0], [20.0, 20.0, 10.0])]);
    let AutoSide::Undecided(why) = auto_side(&square(10.0), &f, &b, || None, 5.0, 5.0).expect("law") else {
        panic!("metal on both sides must not decide");
    };
    assert!(why.contains("set Side to Inside or Outside") && why.contains("Z 5.000"), "{why}");
    assert!(why.contains("on both sides of the ring there"), "{why}");
}

#[test]
fn a_ring_with_no_material_at_its_level_reads_the_part_outline() {
    // A ring over a block, in open air: the XY outline decides, as the
    // old law did (a sheet part has no section at any level).
    let (f, b) = boxes(&[([-20.0, -20.0, 0.0], [20.0, 20.0, 10.0])]);
    let plan = square(20.0);
    assert_eq!(auto_side(&square(10.0), &f, &b, || Some(plan.clone()), 15.0, 15.0).expect("law"), AutoSide::Outside);
    let ring = Region::new(vec![Loop::new(vec![
        Point2::new(30.0, 30.0), Point2::new(40.0, 30.0), Point2::new(40.0, 40.0), Point2::new(30.0, 40.0),
    ]).expect("loop")]).expect("region");
    assert_eq!(auto_side(&ring, &f, &b, || Some(plan.clone()), 15.0, 15.0).expect("law"), AutoSide::Inside);
    assert_eq!(auto_side(&ring, &f, &b, || None, 15.0, 15.0).expect("law"), AutoSide::Outside);
}

#[test]
fn probes_that_disagree_are_undecided_by_name() {
    // Under the top of the span the opening is air in a frame (Inside);
    // over its bottom a block fills the ring with air around it (Outside).
    let mut parts = frame(5.0, 10.0);
    parts.push(([-10.0, -10.0, 0.0], [10.0, 10.0, 5.0]));
    let (f, b) = boxes(&parts);
    let AutoSide::Undecided(why) = auto_side(&square(10.0), &f, &b, || None, 0.0, 10.0).expect("law") else {
        panic!("disagreeing probes must not decide");
    };
    assert!(why.contains("inside the ring on one side of that level and outside it on the other"), "{why}");
}

#[test]
fn no_part_to_read_keeps_outside() {
    assert_eq!(auto_side(&square(10.0), &[], &[], || None, 0.0, 10.0).expect("law"), AutoSide::Outside);
}
