//! Issue #1591: the posted level's law ([`TrochLevel::check`]) - a cut
//! outside the level area and a cut across an earlier move are each
//! refused by name; a level inside its area that crosses nothing
//! passes. Our own regression; no vendor material consulted.

use super::{TrochLevel, WHO_3D};
use crate::cl::{Move, MoveKind};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;

fn area() -> Region {
    let pt = |x: f64, y: f64| Point2::new(x, y);
    let Ok(l) = Loop::new(vec![pt(0.0, 0.0), pt(20.0, 0.0), pt(20.0, 10.0), pt(0.0, 10.0)]) else { panic!("fixture loop") };
    let Ok(r) = Region::new(vec![l]) else { panic!("fixture region") };
    r
}

fn feed(a: (f64, f64), b: (f64, f64), z: f64) -> Move {
    Move::new(MoveKind::Feed, Point3::new(a.0, a.1, z), Point3::new(b.0, b.1, z), Some(800.0))
}

fn refusal(moves: &[Move], n0: usize) -> Option<String> {
    TrochLevel::check(moves, n0, &area()).err().map(|e| e.to_string())
}

#[test]
fn a_level_inside_its_area_that_crosses_nothing_passes() {
    // An earlier cut on another plane lies under the level's own.
    let moves = [feed((2.0, 5.0), (18.0, 5.0), -3.0), feed((10.0, 2.0), (10.0, 8.0), -6.0)];
    assert_eq!(refusal(&moves, 1), None);
}

#[test]
fn a_cut_outside_the_level_area_is_refused_by_name() {
    let moves = [feed((2.0, 5.0), (25.0, 5.0), -3.0)];
    let why = refusal(&moves, 0).unwrap_or_default();
    assert!(why.contains(WHO_3D) && why.contains("would leave the level area"), "{why}");
    // A move posted before the level is not the level's to answer for.
    let moves = [feed((2.0, 5.0), (25.0, 5.0), -3.0), feed((10.0, 2.0), (10.0, 8.0), -6.0)];
    assert_eq!(refusal(&moves, 1), None);
}

#[test]
fn a_cut_across_an_earlier_move_is_refused_by_name() {
    // The level's cut crosses a cut posted before it, on one plane.
    let moves = [feed((2.0, 5.0), (18.0, 5.0), -3.0), feed((10.0, 2.0), (10.0, 8.0), -3.0)];
    let why = refusal(&moves, 1).unwrap_or_default();
    assert!(why.contains(WHO_3D) && why.contains("would cross a cut posted before them"), "{why}");
    // Both moves the level's own: it crosses itself.
    let why = refusal(&moves, 0).unwrap_or_default();
    assert!(why.contains("would cross themselves"), "{why}");
}
