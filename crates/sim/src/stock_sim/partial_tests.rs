//! Pins of the travelled-part cut (#1516): mid-move stock for a line
//! and an arc is the sweep of the travelled part, the whole fraction is
//! the whole move, and degenerate input gives no cut and no panic.

use super::*;
use crate::cl::{MoveKind, ArcMove};
use crate::geo2d::Point2;
use crate::geometry::{cube, Point3};
use crate::sim::Seg;
use crate::stock_sim::backend::{Backend, FIELD_BAND};

fn feed(from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
}

fn block() -> Stock {
    let c = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 12.0, 6.0));
    Stock::from_facets(Backend::Field { band: FIELD_BAND }, &c.facets, 0.5).expect("field stock")
}

fn tool<'a>(moves: &'a [Move]) -> SimOp<'a> {
    SimOp { moves, radius: 1.0, ball: false, cone: None, corner: None }
}

fn cut(base: &Stock, m: Move) -> Stock {
    let mut s = base.clone();
    let mv = [m];
    s.simulate_display(&[tool(&mv)]).expect("cut");
    s
}

/// Every sample of the block, bit for bit.
fn samples(s: &Stock) -> Vec<u64> {
    let mut out = Vec::new();
    for i in 0..44 {
        for j in 0..28 {
            for k in 0..16 {
                let p = Point3::new(i as f64 * 0.47 - 0.2, j as f64 * 0.47 - 0.2, k as f64 * 0.47 - 0.3);
                out.push(s.distance(p).to_bits());
            }
        }
    }
    out
}

fn air(s: &Stock, x: f64, y: f64) -> bool {
    s.distance(Point3::new(x, y, 4.5)) > 0.0
}

#[test]
fn mid_line_stock_is_the_sweep_of_the_travelled_part() {
    let base = block();
    let m = feed(Point3::new(2.0, 6.0, 3.0), Point3::new(18.0, 6.0, 3.0));
    let (shown, bricks) = base.with_partial(&tool(&[]), &m, 0.5).expect("cut").expect("travelled");
    assert!(!bricks.is_empty(), "the partial cut names the bricks it changed");
    let travelled = cut(&base, feed(m.from, Point3::new(10.0, 6.0, 3.0)));
    assert_eq!(samples(&shown), samples(&travelled), "mid-move stock is the sweep of the travelled part");
    assert!(air(&shown, 6.0, 6.0), "behind the tool the material is gone");
    assert!(!air(&shown, 14.0, 6.0), "ahead of the tool it still stands");
    assert!(!air(&base, 6.0, 6.0), "the completed-move stock is untouched");
    let (end, _) = base.with_partial(&tool(&[]), &m, 1.0).expect("cut").expect("travelled");
    assert_eq!(samples(&end), samples(&cut(&base, m)), "the whole fraction is the whole move");
}

#[test]
fn mid_arc_stock_is_the_sweep_of_the_travelled_part() {
    let base = block();
    // The upper half circle about (10, 4), counter-clockwise from +X.
    let m = feed(Point3::new(16.0, 4.0, 3.0), Point3::new(4.0, 4.0, 3.0)).with_arc(Point2::new(10.0, 4.0), true);
    let sub = partial_move(&m, 0.5).expect("travelled");
    assert_eq!(sub.to, point_along(m.from, m.to, m.arc, 0.5), "the cut ends where the tool is drawn");
    assert!((sub.to.x - 10.0).abs() < 1e-9 && (sub.to.y - 10.0).abs() < 1e-9, "half the arc is its top: {:?}", sub.to);
    assert_eq!(sub.arc.map(|a: ArcMove| (a.center, a.ccw)), m.arc.map(|a| (a.center, a.ccw)), "the sub-arc keeps the circle");
    let (shown, _) = base.with_partial(&tool(&[]), &m, 0.5).expect("cut").expect("travelled");
    assert_eq!(samples(&shown), samples(&cut(&base, sub)), "mid-arc stock is the sweep of the travelled arc");
    let (s45, c45) = std::f64::consts::FRAC_PI_4.sin_cos();
    assert!(air(&shown, 10.0 + 6.0 * c45, 4.0 + 6.0 * s45), "behind the tool (45 deg) the material is gone");
    assert!(!air(&shown, 10.0 - 6.0 * c45, 4.0 + 6.0 * s45), "ahead of it (135 deg) it stands");
    let whole = cut(&base, m);
    assert!(air(&whole, 10.0 - 6.0 * c45, 4.0 + 6.0 * s45), "the whole arc cuts it");
    let (end, _) = base.with_partial(&tool(&[]), &m, 1.0).expect("cut").expect("travelled");
    assert_eq!(samples(&end), samples(&whole), "the whole fraction is the whole arc");
}

#[test]
fn degenerate_moves_and_fractions_cut_nothing() {
    let base = block();
    let p = Point3::new(5.0, 5.0, 3.0);
    let line = feed(p, Point3::new(15.0, 5.0, 3.0));
    for f in [0.0, -1.0, f64::NAN, f64::NEG_INFINITY, 1e-12] {
        assert!(partial_move(&line, f).is_none(), "fraction {f} travelled nothing");
        assert!(base.with_partial(&tool(&[]), &line, f).expect("no cut").is_none());
    }
    assert_eq!(partial_move(&line, f64::INFINITY), None, "an infinite fraction is no position");
    assert_eq!(partial_move(&line, 7.0), Some(line), "past the end clamps to the whole move");
    assert!(partial_move(&feed(p, p), 0.5).is_none(), "a zero-length move has no travelled part");
    let nan = feed(p, Point3::new(f64::NAN, 5.0, 3.0));
    assert!(partial_move(&nan, 0.5).is_none(), "a non-finite move is never cut");
    let flat = feed(p, Point3::new(15.0, 5.0, 3.0)).with_arc(Point2::new(5.0, 5.0), true);
    assert!(partial_move(&flat, 0.5).is_some_and(|m| m.to.is_finite()), "a zero-radius arc falls back straight");
}

fn seg(from: Point3, to: Point3, secs: f64, dwell: f64) -> Seg {
    Seg { from, to, arc: None, secs, dwell, feed: Some(500.0), op: 0, abc: ([0.0; 3], [0.0; 3]), pl: secs, rl: 0.0 }
}

#[test]
fn the_playhead_fraction_follows_the_drawn_tool() {
    let a = Point3::new(0.0, 0.0, 0.0);
    let b = Point3::new(10.0, 0.0, 0.0);
    let sc = Script { ops: vec!["Op".into()], segs: vec![seg(a, b, 4.0, 1.0), seg(b, a, 2.0, 0.0)], cum: vec![5.0, 7.0], total: 7.0, truncated: false };
    assert_eq!(sc.travelled(1.0), 0.25);
    assert_eq!(sc.travelled(4.5), 1.0, "a dwell stands at the move's end");
    assert_eq!(sc.travelled(6.0), 0.5);
    for t in [0.0, -3.0, 7.0, 99.0, f64::NAN, f64::INFINITY] {
        assert_eq!(sc.travelled(t), 0.0, "t = {t} has nothing in flight");
    }
    assert_eq!(Script::default().travelled(1.0), 0.0, "an empty timeline has nothing in flight");
}
