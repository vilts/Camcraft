//! Pins of the slot fit law (#1504): two opposite edges give the slot
//! between them, a face gives the slot that fits it, and a slot too
//! wide, too long or placed past the picks is refused by name with its
//! numbers.

use super::*;
use crate::geo2d::Loop;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// The two long floor edges of a 30 x 10 pocket centred on the origin.
fn pocket_edges() -> SlotBound {
    SlotBound::from_edges(&[p(-15.0, -5.0), p(0.0, -5.0), p(15.0, -5.0)], &[p(15.0, 5.0), p(-15.0, 5.0)]).expect("bound")
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)]).expect("loop")]).expect("region")
}

fn msg(e: GenError) -> String {
    e.to_string()
}

#[test]
fn two_opposite_edges_give_the_slot_between_them() {
    let b = pocket_edges();
    assert_eq!(b.room(), (30.0, 10.0));
    let f = b.fit().expect("fit");
    assert!((f.width - 10.0).abs() < 1e-12, "{f:?}");
    let (lo, hi) = if f.a.x < f.b.x { (f.a, f.b) } else { (f.b, f.a) };
    assert!((lo.x + 10.0).abs() < 1e-12 && (hi.x - 10.0).abs() < 1e-12, "round ends tangent to the edges' ends: {f:?}");
    assert!(lo.y.abs() < 1e-12 && hi.y.abs() < 1e-12, "on the centre line: {f:?}");
    b.confine(f.a, f.b, f.width).expect("the fit fits");
    // A narrower, shorter slot inside the strip fits too.
    b.confine(p(-5.0, 1.0), p(5.0, 1.0), 6.0).expect("inside");
}

#[test]
fn a_slot_wider_than_the_edges_is_refused_by_name() {
    let e = msg(pocket_edges().confine(p(-8.0, 0.0), p(8.0, 0.0), 12.0).expect_err("too wide"));
    assert!(e.contains("12.000 mm wide") && e.contains("only 10.000 mm apart"), "{e}");
}

#[test]
fn a_slot_longer_than_the_edges_is_refused_by_name() {
    // The maintainer's typed default against a 30 mm pocket: 44 mm end to end.
    let e = msg(pocket_edges().confine(p(-20.0, 0.0), p(20.0, 0.0), 4.0).expect_err("too long"));
    assert!(e.contains("44.000 mm long end to end") && e.contains("share only 30.000 mm"), "{e}");
}

#[test]
fn a_slot_placed_past_the_edges_is_refused_by_name() {
    let e = msg(pocket_edges().confine(p(-3.0, 3.0), p(3.0, 3.0), 6.0).expect_err("across the wall"));
    assert!(e.contains("runs 1.000 mm outside the picked edges"), "{e}");
}

#[test]
fn edges_that_bound_no_slot_are_refused_not_guessed() {
    let e = msg(SlotBound::from_edges(&[p(0.0, 0.0), p(10.0, 0.0)], &[p(0.0, 5.0), p(10.0, 8.0)]).expect_err("skew"));
    assert!(e.contains("not straight and parallel"), "{e}");
    let e = msg(SlotBound::from_edges(&[p(0.0, 0.0), p(10.0, 0.0)], &[p(20.0, 0.0), p(30.0, 0.0)]).expect_err("one line"));
    assert!(e.contains("lie on one line"), "{e}");
    let e = msg(SlotBound::from_edges(&[p(0.0, 0.0), p(10.0, 0.0)], &[p(20.0, 5.0), p(30.0, 5.0)]).expect_err("apart"));
    assert!(e.contains("do not face each other"), "{e}");
    let e = msg(SlotBound::from_edges(&[p(0.0, 0.0), p(5.0, 3.0), p(10.0, 0.0)], &[p(0.0, 5.0), p(10.0, 5.0)]).expect_err("bowed"));
    assert!(e.contains("not straight"), "{e}");
    let e = msg(SlotBound::from_edges(&[p(0.0, 0.0), p(4.0, 0.0)], &[p(0.0, 5.0), p(4.0, 5.0)]).expect("bound").fit().expect_err("short"));
    assert!(e.contains("4.000 mm long for a 5.000 mm wide slot"), "{e}");
}

#[test]
fn a_face_gives_the_slot_that_fits_it() {
    // A 10 x 30 face standing along Y, off the origin.
    let b = SlotBound::from_face(rect(10.0, -5.0, 20.0, 25.0)).expect("bound");
    let f = b.fit().expect("fit");
    assert!((f.width - 10.0).abs() < 1e-9, "{f:?}");
    assert!((f.a.x - 15.0).abs() < 1e-9 && (f.b.x - 15.0).abs() < 1e-9, "{f:?}");
    let (lo, hi) = if f.a.y < f.b.y { (f.a.y, f.b.y) } else { (f.b.y, f.a.y) };
    assert!((lo - 0.0).abs() < 1e-9 && (hi - 20.0).abs() < 1e-9, "{f:?}");
    let e = msg(b.confine(p(15.0, -10.0), p(15.0, 30.0), 6.0).expect_err("too long"));
    assert!(e.contains("46.000 mm long end to end") && e.contains("is only 30.000 mm long"), "{e}");
    let e = msg(b.confine(p(15.0, 0.0), p(15.0, 20.0), 12.0).expect_err("too wide"));
    assert!(e.contains("12.000 mm wide") && e.contains("is only 10.000 mm wide"), "{e}");
}

#[test]
fn a_face_no_straight_slot_fills_is_refused() {
    // An L: its min rectangle's slot crosses the notch.
    let l = Region::new(vec![
        Loop::new(vec![p(0.0, 0.0), p(60.0, 0.0), p(60.0, 10.0), p(10.0, 10.0), p(10.0, 40.0), p(0.0, 40.0)]).expect("loop"),
    ])
    .expect("region");
    // A square holds a disk, not a slot.
    let e = msg(SlotBound::from_face(rect(0.0, 0.0, 20.0, 20.0)).expect("bound").fit().expect_err("square"));
    assert!(e.contains("leaves room 20.000 mm long for a 20.000 mm wide slot"), "{e}");
    let e = msg(SlotBound::from_face(l).expect("bound").fit().expect_err("an L holds no slot"));
    assert!(e.contains("holds no straight slot"), "{e}");
    // An island standing in the slot's way is material in it.
    let island = Region::new(vec![
        Loop::new(vec![p(0.0, 0.0), p(40.0, 0.0), p(40.0, 10.0), p(0.0, 10.0)]).expect("outer"),
        Loop::new(vec![p(19.0, 4.0), p(21.0, 4.0), p(21.0, 6.0), p(19.0, 6.0)]).expect("island"),
    ])
    .expect("region");
    let b = SlotBound::from_face(island).expect("bound");
    let e = msg(b.confine(p(5.0, 5.0), p(35.0, 5.0), 8.0).expect_err("island in the slot"));
    assert!(e.contains("outside the picked face"), "{e}");
}
