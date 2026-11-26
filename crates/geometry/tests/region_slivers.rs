//! #2766: `Region::without_slivers` removes the loops of near-zero area
//! that a boolean leaves, and keeps each real loop as it is.
//!
//! Gate map: covers camcraft_geometry::geo2d (slivers).
//! Provenance: our own regression of our own clean-up rule; no vendor
//! material consulted.

use camcraft_geometry::geo2d::{Loop, Point2, Region};

fn poly(pts: &[(f64, f64)]) -> Region {
    let l = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).expect("a loop");
    Region::new(vec![l]).expect("a region")
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

/// A 10 x 10 plate with a real 1 x 1 hole, a sliver hole of 3e-7 mm2 and
/// a sliver of material of 3e-7 mm2 that stands apart.
fn plate_with_slivers() -> Region {
    let hole = rect(2.0, 2.0, 3.0, 3.0);
    let sliver_hole = poly(&[(6.0, 6.0), (6.3, 6.0), (6.3, 6.000_002)]);
    let sliver_island = poly(&[(20.0, 0.0), (20.3, 0.0), (20.3, 0.000_002)]);
    rect(0.0, 0.0, 10.0, 10.0)
        .difference(&hole)
        .and_then(|r| r.difference(&sliver_hole))
        .and_then(|r| r.union(&sliver_island))
        .expect("booleans")
}

#[test]
fn without_slivers_drops_near_zero_loops_and_keeps_the_real_ones() {
    let r = plate_with_slivers();
    assert_eq!(r.loops().len(), 4, "the fixture has the plate, the hole and two slivers");
    let clean = r.without_slivers(1e-4);
    assert_eq!(clean.loops().len(), 2, "the plate and its real hole stay");
    for l in clean.loops() {
        assert!(r.loops().contains(l), "a kept loop is not changed");
    }
    assert!((clean.area() - 99.0).abs() < 1e-9, "area {}", clean.area());
    let tris = clean.triangles().expect("the clean region triangulates");
    assert!(!tris.is_empty());
}

#[test]
fn without_slivers_changes_nothing_for_a_clean_region_or_a_bad_measure() {
    let clean = rect(0.0, 0.0, 10.0, 10.0).difference(&rect(2.0, 2.0, 3.0, 3.0)).expect("difference");
    assert_eq!(clean.without_slivers(1e-4), clean);
    let r = plate_with_slivers();
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(r.without_slivers(bad), r, "measure {bad} removes nothing");
    }
}
