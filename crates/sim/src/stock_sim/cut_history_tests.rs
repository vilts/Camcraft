//! #3819: `simulate` grows the column stock's cut history in place. It
//! copied the whole history on each call before, so a program cut one
//! move for each call was quadratic in its moves.
//! Provenance: our own block-and-pass fixtures.

use super::*;
use crate::geo2d::Point2;
use crate::geometry::Point3;
use std::sync::Arc;

fn pass(y: f64) -> Move {
    Move::new(MoveKind::Feed, Point3::new(-1.0, y, 0.5), Point3::new(5.0, y, 0.5), Some(500.0))
}

fn op(m: &[Move]) -> [SimOp<'_>; 1] {
    [SimOp { moves: m, radius: 0.5, ball: false, cone: None, corner: None }]
}

fn top(dex: &Dexel) -> f64 {
    dex.col(Point2::new(1.0, 1.0)).map_or(f64::NAN, |k| dex.top_at(k))
}

fn cuts(dex: &Dexel) -> usize {
    dex.history.as_ref().map_or(0, |h| h.cuts.len())
}

#[test]
fn a_call_grows_the_history_in_place() {
    let mut dex = Dexel::block(0.0, 0.0, 4.0, 4.0, 1.0, 0.1).expect("block");
    let before = dex.history.as_ref().map(Arc::as_ptr).expect("a block keeps a history");
    let n = cuts(&dex);
    simulate(&mut dex, &op(&[pass(1.0)])).expect("first pass");
    simulate(&mut dex, &op(&[pass(2.0)])).expect("second pass");
    let after = dex.history.as_ref().map(Arc::as_ptr).expect("the history stays");
    assert_eq!(before, after, "simulate copied the history instead of growing it in place");
    assert!(cuts(&dex) > n, "the passes were not added to the history");
}

#[test]
fn a_shared_history_is_copied_before_it_grows() {
    let mut cut = Dexel::block(0.0, 0.0, 4.0, 4.0, 1.0, 0.1).expect("block");
    let kept = cut.clone();
    let n = cuts(&kept);
    simulate(&mut cut, &op(&[pass(1.0)])).expect("pass");
    assert_eq!(cuts(&kept), n, "a cut on one stock changed the history of its clone");
    assert!(cuts(&cut) > n, "the pass was not added to the cut stock's history");
    let (a, b) = (cut.history.as_ref().map(Arc::as_ptr), kept.history.as_ref().map(Arc::as_ptr));
    assert_ne!(a, b, "the two stocks still share one history");
    assert!(top(&kept) > top(&cut), "the pass did not cut, or it cut the clone too");
}
