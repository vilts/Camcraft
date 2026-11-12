//! Unit pins of `rotary_law` (#904): the radial field law's band and rapid allowances on a model imported coarser than promised - each shrinks by the excess while a positive allowance is left, and reads untightened past that. Our own fixtures; no vendor material.

use super::*;
use crate::cl::{Move, MoveKind, Toolpath};
use crate::geometry::Point3;

/// A bar of radius `r` laid along world X, from X -16 to 16.
fn bar(r: f64) -> Vec<Triangle> {
    let v = |j: usize, x: f64| {
        let th = std::f64::consts::TAU * j as f64 / 32.0;
        Point3::new(x, r * th.cos(), r * th.sin())
    };
    let mut f = Vec::new();
    for j in 0..32 {
        let (a, b, c, d) = (v(j, -16.0), v(j + 1, -16.0), v(j, 16.0), v(j + 1, 16.0));
        f.push(Triangle::new(a, b, d));
        f.push(Triangle::new(a, d, c));
        f.push(Triangle::new(Point3::new(-16.0, 0.0, 0.0), b, a));
        f.push(Triangle::new(Point3::new(16.0, 0.0, 0.0), c, d));
    }
    f
}

/// A 6 mm ball on a 3 mm flute under a 10 mm shank: the shank's bottom
/// stands 3 mm above the tip, wider than the cut.
fn cutter() -> verify::Cutter {
    verify::Cutter { radius: 3.0, corner_r: 3.0, tip_angle: 0.0, flute_len: 3.0, shank_radius: 5.0, shank_len: 20.0, holder_radius: 5.0, holder_len: 5.0, body: None, neck: None }
}

/// The law's warnings for one upright move along the bar's crown with
/// the tip at height `z`, the part an r 6 bar in an r 10 bar of stock.
fn verdict(kind: MoveKind, z: f64, tolerance: f64, excess: f64) -> Vec<Warning> {
    let m = Move { rotary: None, kind, from: Point3::new(-1.0, 0.0, z), to: Point3::new(1.0, 0.0, z), feed: (kind != MoveKind::Rapid).then_some(300.0), arc: None, axis: Some([0.0, 0.0, 1.0]) };
    let path = Toolpath { moves: vec![m], cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None };
    let mut warns = Vec::new();
    rotary_into(&mut warns, &path, &bar(6.0), Some(&bar(10.0)), &cutter(), (tolerance, excess));
    warns
}

/// The tip height at which `hit` first stops holding, coming down from
/// `clear` (no hit) toward `struck` (hit): the law's own threshold.
fn threshold(clear: f64, struck: f64, hit: impl Fn(f64) -> bool) -> f64 {
    assert!(!hit(clear) && hit(struck), "the bracket holds the threshold");
    let (mut lo, mut hi) = (struck, clear);
    // 14 halvings of a bracket a few millimetres wide: 0.2 microns.
    for _ in 0..14 {
        let mid = 0.5 * (lo + hi);
        if hit(mid) { lo = mid } else { hi = mid }
    }
    hi
}

const EXCESS: f64 = 0.007;

#[test]
fn the_band_allowance_gives_a_coarse_models_excess_back() {
    // A cut along the crown: the ball carves its own kerf, and the
    // wider shank meets the stock standing beside it once the tip is
    // deep enough. The depth it may stand in is the allowance.
    let hit = |excess: f64| move |z: f64| !verdict(MoveKind::Feed, z, 0.1, excess).is_empty();
    let full = threshold(9.0, 6.0, hit(0.0));
    let coarse = threshold(9.0, 6.0, hit(EXCESS));
    // The shank is convicted sooner on the coarse model - by the
    // excess, read along the field's ray (a few percent off vertical).
    assert!(coarse > full, "stricter, never looser: {coarse} vs {full}");
    assert!((coarse - full - EXCESS).abs() < 0.0015, "by the excess: {}", coarse - full);
    // Between the two: silent today, a shank hit on the coarse model.
    let between = 0.5 * (full + coarse);
    assert!(verdict(MoveKind::Feed, between, 0.1, 0.0).is_empty());
    assert!(matches!(verdict(MoveKind::Feed, between, 0.1, EXCESS).as_slice(), [Warning::ShankHit { at: 0, .. }]));
    // Past the bound (nothing of the 0.01 mm left) the law reads
    // untightened: the program gate refuses the operation instead.
    assert!((threshold(9.0, 6.0, hit(GOUGE_TOL)) - full).abs() < 1e-9);
}

#[test]
fn the_rapid_allowance_gives_a_coarse_models_excess_back() {
    // A rapid along the crown: the ball may graze the stock's skin by
    // the operation's finish budget (0.1 mm), no more.
    let hit = |excess: f64| move |z: f64| !verdict(MoveKind::Rapid, z, 0.1, excess).is_empty();
    let full = threshold(10.5, 9.0, hit(0.0));
    let coarse = threshold(10.5, 9.0, hit(EXCESS));
    assert!(coarse > full, "stricter, never looser: {coarse} vs {full}");
    assert!((coarse - full - EXCESS).abs() < 0.0015, "by the excess: {}", coarse - full);
    let between = 0.5 * (full + coarse);
    assert!(verdict(MoveKind::Rapid, between, 0.1, 0.0).is_empty());
    assert_eq!(verdict(MoveKind::Rapid, between, 0.1, EXCESS), vec![Warning::RapidInStock { at: 0 }]);
    // A rapid a millimetre in the stock is convicted either way; one a
    // millimetre clear is silent either way.
    assert!(!verdict(MoveKind::Rapid, 9.0, 0.1, 0.0).is_empty() && !verdict(MoveKind::Rapid, 9.0, 0.1, EXCESS).is_empty());
    assert!(verdict(MoveKind::Rapid, 11.0, 0.1, 0.0).is_empty() && verdict(MoveKind::Rapid, 11.0, 0.1, EXCESS).is_empty());
    // The rapid allowance is the finish budget: it keeps tightening
    // while the excess leaves it positive, and reads plain past that.
    assert!((threshold(10.5, 9.0, hit(0.1)) - full).abs() < 1e-9);
}
