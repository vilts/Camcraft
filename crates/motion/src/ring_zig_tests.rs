//! #1558 review pins for the ring ramp ([`crate::ring_zig`]): it rides
//! arcs only where the planned prefix is on the ring in the ring's own
//! sense and every arc spans a postable chord; anything else posts the
//! chords exactly as [`crate::links::push_zig`] does.
//!
//! Provenance: our own tests over RS-274 G2/G3 helical interpolation;
//! no vendor material consulted.

use std::f64::consts::PI;

use crate::cl::{Move, PathBuilder};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::{plan_zig_capped, push_zig};
use crate::ring_zig::{MIN_ARC_CHORD, arc_chord, push_zig_ring};

const C: Point2 = Point2 { x: 4.0, y: -2.0 };

/// A closed ring polygon of `n` chords on radius `rho` about [`C`],
/// CCW when `ccw` - the circular kernel's own shape.
fn ring(rho: f64, n: usize, ccw: bool) -> Vec<Point2> {
    let s = if ccw { 1.0 } else { -1.0 };
    (0..=n)
        .map(|k| {
            let t = s * 2.0 * PI * k as f64 / n as f64;
            Point2::new(C.x + rho * t.cos(), C.y + rho * t.sin())
        })
        .collect()
}

fn start(p: Point2, z: f64) -> PathBuilder {
    PathBuilder::new(Point3::new(p.x, p.y, z))
}

fn ring_ramp(pre: &[Point2], ccw: bool, n: usize, dz: f64) -> Vec<Move> {
    let mut b = start(pre[0], 0.0);
    push_zig_ring(&mut b, pre, C, ccw, n, 0.0, -dz, 100.0);
    b.finish().moves
}

fn chord_ramp(pre: &[Point2], n: usize, dz: f64) -> Vec<Move> {
    let mut b = start(pre[0], 0.0);
    push_zig(&mut b, pre, n, 0.0, -dz, 100.0);
    b.finish().moves
}

/// An arc move's sweep in its own sense, radians in (0, 2 pi].
fn sweep(m: &Move) -> f64 {
    let a = m.arc.expect("an arc");
    let ang = |p: Point3| (p.y - a.center.y).atan2(p.x - a.center.x);
    let d = ang(m.to) - ang(m.from);
    let d = if a.ccw { d } else { -d };
    let d = d.rem_euclid(2.0 * PI);
    if d <= 0.0 { 2.0 * PI } else { d }
}

/// On the ring, in its sense: arcs about the centre, each leg's sweep
/// is the planned leg (lap / rho) and every piece descends at the ramp
/// angle at most.
#[test]
fn an_on_ring_prefix_rides_arcs_at_the_ramp_angle() {
    for ccw in [true, false] {
        let rho = 1.0;
        let path = ring(rho, 32, ccw);
        let (pre, n) = plan_zig_capped(&path, 1.0, 3.0, 0.0).expect("a plan");
        let lap: f64 = pre.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
        let moves = ring_ramp(&pre, ccw, n, 1.0);
        assert!(moves.iter().all(|m| m.arc.is_some()), "ccw {ccw}: all arcs");
        let tan = 3f64.to_radians().tan();
        let mut leg = 0.0;
        let mut legs = 0;
        let mut out = None;
        for m in &moves {
            let a = m.arc.expect("arc");
            let len = rho * sweep(m);
            let dz = m.from.z - m.to.z;
            assert!(dz > 0.0 && dz / len <= tan * (1.0 + 1e-6), "ccw {ccw}: slope {} deg", (dz / len).atan().to_degrees());
            if out.is_some_and(|o| o != a.ccw) {
                assert!((leg - lap / rho).abs() < 1e-5, "ccw {ccw}: leg sweep {leg} vs plan {}", lap / rho);
                leg = 0.0;
                legs += 1;
            }
            out = Some(a.ccw);
            leg += sweep(m);
        }
        assert!((leg - lap / rho).abs() < 1e-5, "ccw {ccw}: last leg {leg}");
        assert_eq!(legs + 1, 2 * n, "ccw {ccw}: legs");
    }
}

/// A ladder that is not this ring - a square corner, or an arc that
/// runs onto its tangent line before the prefix ends - stays chords,
/// exactly push_zig's, whatever flag asked for arcs.
#[test]
fn a_non_circular_prefix_stays_chords() {
    let square = vec![
        Point2::new(C.x + 1.0, C.y),
        Point2::new(C.x + 1.0, C.y + 1.0),
        Point2::new(C.x, C.y + 1.0),
    ];
    // A quarter arc of R1 (8 chords) then its tangent line onward.
    let mut bore: Vec<Point2> = ring(1.0, 32, true).into_iter().take(9).collect();
    bore.push(Point2::new(C.x - 2.0, C.y + 1.0));
    for path in [square, bore] {
        let (pre, n) = plan_zig_capped(&path, 1.0, 3.0, 0.0).expect("a plan");
        let moves = ring_ramp(&pre, true, n, 1.0);
        assert!(moves.iter().all(|m| m.arc.is_none()), "chords only");
        assert_eq!(moves, chord_ramp(&pre, n, 1.0), "the chords push_zig posts");
    }
}

/// A ring prefix walked in the sense opposite to the arcs asked for is
/// caught (each leg would fly the long way round): chords.
#[test]
fn a_flipped_sense_is_caught() {
    for ccw in [true, false] {
        let path = ring(1.0, 32, ccw);
        let (pre, n) = plan_zig_capped(&path, 1.0, 3.0, 0.0).expect("a plan");
        let moves = ring_ramp(&pre, !ccw, n, 1.0);
        assert!(moves.iter().all(|m| m.arc.is_none()), "ccw {ccw}: chords");
        assert_eq!(moves, chord_ramp(&pre, n, 1.0));
    }
}

/// A micro-ramp (a few-micron last level) would fly arcs whose ends a
/// four-decimal inch post rounds together - a full circle on the
/// control. Its pieces are under MIN_ARC_CHORD, so it posts chords;
/// a ramp just over the bound still rides arcs.
#[test]
fn a_sub_resolution_ramp_posts_chords() {
    let path = ring(1.0, 32, true);
    for dz in [0.0003, 0.0001, 1e-6] {
        let (pre, n) = plan_zig_capped(&path, dz, 3.0, 0.0).expect("a plan");
        let lap: f64 = pre.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
        assert!(arc_chord(1.0, lap) < MIN_ARC_CHORD, "dz {dz}: the fixture is sub-resolution");
        let moves = ring_ramp(&pre, true, n, dz);
        assert!(moves.iter().all(|m| m.arc.is_none()), "dz {dz}: chords");
    }
    let (pre, n) = plan_zig_capped(&path, 0.002, 3.0, 0.0).expect("a plan");
    let moves = ring_ramp(&pre, true, n, 0.002);
    assert!(moves.iter().all(|m| m.arc.is_some()), "a 19 um leg rides arcs");
}

#[test]
fn min_arc_chord_outlasts_four_decimal_inch() {
    // One posted unit on each axis: sqrt(2) * 0.0001 in.
    assert!(MIN_ARC_CHORD >= 2f64.sqrt() * 0.00254);
    assert!((arc_chord(2.0, PI) - 4.0).abs() < 1e-12);
}

/// Ring vertices alone do not prove a long flat follows the ring.
#[test]
fn a_d_shaped_prefix_posts_its_chords() {
    for ccw in [true, false] {
        let sense = if ccw { 1.0 } else { -1.0 };
        let angle: f64 = sense * 0.98;
        let flat = vec![Point2::new(C.x + 1.0, C.y), Point2::new(C.x + angle.cos(), C.y + angle.sin())];
        let mut followed_flat = flat.clone();
        let end = angle + sense * 0.05;
        followed_flat.push(Point2::new(C.x + end.cos(), C.y + end.sin()));
        for pre in [flat, followed_flat] {
            let moves = ring_ramp(&pre, ccw, 1, 0.1);
            assert!(moves.iter().all(|m| m.arc.is_none()), "ccw {ccw}: the 0.98 rad flat must stay chords");
            assert_eq!(moves, chord_ramp(&pre, 1, 0.1), "ccw {ccw}: preserve the planned flat exactly");
        }
    }
}
