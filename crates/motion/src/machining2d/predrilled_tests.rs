//! Unit pins for `predrilled::entry` (#1449 review): the hole-in-area
//! check and the link proof each refuse on their own, on hand-built
//! levels the public pocket entry cannot reach (its re-seating puts the
//! pass start where both always hold).
//!
//! Provenance: our own fixtures.

use super::super::descent::Lvl;
use super::super::levels::{Cert, Motion};
use super::{PreHole, entry};
use crate::geo2d::{Loop, Point2, Region};
use crate::links::{Approach, Links};

/// The cutter-centre area: 0..40 x 0..30.
fn area() -> Region {
    let pts = vec![Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), Point2::new(40.0, 30.0), Point2::new(0.0, 30.0)];
    match Loop::new(pts).and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("region: {e:?}"),
    }
}

fn motion(holes: &[PreHole]) -> Motion {
    let links = Links { approach: Approach { predrilled: true, center_cutting: true, ..Approach::DEFAULT }, ..Links::PLANAR };
    Motion::new(25.0, Some(15.0), 500.0, 100.0).with_links(links, 1.5).with_holes(holes)
}

fn served(passes: &[Vec<Point2>], hole: Point2) -> bool {
    let r = area();
    let cen = Cert::Region(&r);
    let l = Lvl { cen: &cen, passes, pass: &passes[0], walk: None, mat_top: 0.0 };
    entry(&l, -4.0, &motion(&[PreHole { at: hole, floor: -10.0 }])).is_some()
}

#[test]
fn a_hole_outside_the_cutter_area_is_refused_even_at_the_pass_start() {
    // The pass start itself lies outside the area, and the hole sits on
    // it - the link proof is skipped (no link), so only the area check
    // stands between this hole and a plunge.
    let e = Point2::new(50.0, 15.0);
    let passes = vec![vec![e, Point2::new(45.0, 15.0)]];
    assert!(!served(&passes, e), "a hole outside the cutter-centre area is never plunged");
    // Control: the same shape inside the area is served.
    let e = Point2::new(10.0, 15.0);
    assert!(served(&[vec![e, Point2::new(30.0, 15.0)]], e));
}

#[test]
fn a_hole_whose_link_crosses_a_planned_pass_is_refused() {
    // Hole and start both inside the area; the chord between them
    // crosses another planned pass - only the link proof refuses it.
    let e = Point2::new(5.0, 5.0);
    let hole = Point2::new(5.0, 25.0);
    let first = vec![e, Point2::new(35.0, 5.0)];
    let blocker = vec![Point2::new(2.0, 15.0), Point2::new(38.0, 15.0)];
    assert!(!served(&[first.clone(), blocker], hole), "a link across a planned pass is never cut");
    // Control: with nothing in the way the same hole is served.
    assert!(served(&[first], hole));
}
