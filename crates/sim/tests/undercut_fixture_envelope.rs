//! #1444: the fixture laws (c2)/(d2) sweep a lollipop's whole ball. The
//! neck leaves the ball below its pole, so the plain tool stack reads
//! the head only up to the neck start and the neck above it; the upper
//! hemisphere outside the neck was never swept. The fixture stack
//! (`camcraft_tools::undercut::fixture_geom`) carries the head band up
//! to the pole, on feeds and on rapids.
//! Provenance: plane and solid geometry of the handbook ball cutter;
//! our own fixture numbers, per the Clean-Room Protocol.
//!
//! Gate map: covers camcraft_sim::verify::laws (bands_vs_fixtures,
//! rapids_vs_fixtures) with camcraft_tools::undercut::fixture_geom.

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{bands_vs_fixtures, rapids_vs_fixtures};
use camcraft_tools::tools::ToolKind;
use camcraft_tools::undercut::{cutter, fixture_geom};

/// A 10 mm ball, its 4 mm neck leaving it 5 mm above the tip.
fn lollipop() -> camcraft_tools::tools::Tool {
    cutter(ToolKind::Lollipop, 10.0, 5.0, 4.0, 30.0, 0.0)
}

/// The tip from (-10, 0, 0) to (10, 0, 0).
fn pass(kind: MoveKind) -> Toolpath {
    Toolpath::new(vec![Move::new(kind, Point3::new(-10.0, 0.0, 0.0), Point3::new(10.0, 0.0, 0.0), None)])
}

/// At mid-move, (0, 3.5, 6.5) is in this box and in the ball: its
/// squared distance from the centre (0, 0, 5) is 14.5, under 25. It is
/// outside the neck (radius 2) and above the neck start (Z 5).
fn cap_box() -> FixtureBox {
    FixtureBox { min: Point3::new(-0.5, 3.0, 6.0), max: Point3::new(0.5, 4.0, 7.0) }
}

#[test]
fn a_feed_strikes_steel_that_only_the_balls_upper_cap_reaches() {
    let g = fixture_geom(&lollipop());
    let hits = bands_vs_fixtures(&pass(MoveKind::Feed), &g, &[cap_box()], None, 0.01);
    assert_eq!(hits.len(), 1, "a real lollipop/fixture collision passed: {hits:?}");
}

#[test]
fn a_rapid_strikes_steel_that_only_the_balls_upper_cap_reaches() {
    let g = fixture_geom(&lollipop());
    let hits = rapids_vs_fixtures(&pass(MoveKind::Rapid), &g, &[cap_box()]);
    assert_eq!(hits.len(), 1, "a real lollipop/fixture collision passed: {hits:?}");
}

#[test]
fn steel_over_the_pole_beside_the_neck_stays_clear() {
    // Z 10.5..11: over the pole, 3 mm off the 2 mm neck.
    let high = FixtureBox { min: Point3::new(-0.5, 3.0, 10.5), max: Point3::new(0.5, 4.0, 11.0) };
    let g = fixture_geom(&lollipop());
    assert!(bands_vs_fixtures(&pass(MoveKind::Feed), &g, &[high], None, 0.01).is_empty());
    assert!(rapids_vs_fixtures(&pass(MoveKind::Rapid), &g, &[high]).is_empty());
}
