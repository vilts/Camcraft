//! M11.2 A: the arc plane's laws - frame, direction per plane (the
//! G18 handedness included), tessellation endpoints and midpoints,
//! the builder's Z-monotone split. Provenance: RS-274/NGC and ISO
//! 6983 plane selection and G2/G3 direction (public standards).

use std::f64::consts::PI;

use super::*;
use crate::geo2d::Point2;
use crate::geometry::Point3;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn near(a: Point3, b: Point3, tol: f64) -> bool {
    (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol && (a.z - b.z).abs() <= tol
}

fn arc(from: Point3, to: Point3, c: (f64, f64), ccw: bool, plane: ArcPlane) -> Move {
    Move::new(MoveKind::Feed, from, to, Some(100.0)).with_arc_in(Point2::new(c.0, c.1), ccw, plane)
}

#[test]
fn the_frames_are_right_handed_permutations() {
    let q = p(1.0, 2.0, 3.0);
    for pl in [ArcPlane::Xy, ArcPlane::Xz, ArcPlane::Yz] {
        let (u, v, w) = pl.uvw(q);
        assert_eq!(pl.point(u, v, w), q, "{pl:?} round-trips bit for bit");
    }
    // (u, v, w) basis vectors: u x v must be the +normal (w).
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    };
    for (pl, normal) in [
        (ArcPlane::Xy, [0.0, 0.0, 1.0]),
        (ArcPlane::Xz, [0.0, 1.0, 0.0]),
        (ArcPlane::Yz, [1.0, 0.0, 0.0]),
    ] {
        let e = |u: f64, v: f64, w: f64| {
            let q = pl.point(u, v, w);
            [q.x, q.y, q.z]
        };
        assert_eq!(cross(e(1.0, 0.0, 0.0), e(0.0, 1.0, 0.0)), normal, "{pl:?}");
        assert_eq!(e(0.0, 0.0, 1.0), normal, "{pl:?} w is the normal axis");
    }
    assert_eq!(ArcPlane::Xz.center_letters(), ('I', 'K'));
    assert_eq!(ArcPlane::Yz.center_letters(), ('J', 'K'));
    assert_eq!(ArcPlane::from_letters('I', 'K'), Some(ArcPlane::Xz));
    assert_eq!(ArcPlane::from_letters('J', 'I'), None);
    assert_eq!(ArcPlane::default(), ArcPlane::Xy);
}

#[test]
fn g18_reads_from_plus_y_so_the_front_view_looks_reversed() {
    // Center (X0, Z0); (X10, Z0) -> (X0, Z10). Drawn X-right/Z-up the
    // short quarter LOOKS counterclockwise, but RS-274 views G18 from
    // +Y: it is G2.
    let (a, b) = (p(10.0, 0.0, 0.0), p(0.0, 0.0, 10.0));
    let g2 = arc(a, b, (0.0, 0.0), false, ArcPlane::Xz);
    let am = g2.arc.unwrap_or(ArcMove::xy(Point2::new(0.0, 0.0), false));
    let (r, d) = am.radius_sweep(a, b);
    assert!((r - 10.0).abs() < 1e-12 && (d - PI / 2.0).abs() < 1e-12, "G2 is the quarter");
    let h = 10.0 / 2f64.sqrt();
    let mid = am.point_at(a, b, 0.5).unwrap_or(a);
    assert!(near(mid, p(h, 0.0, h), 1e-12), "midpoint {mid:?}");
    // G3 between the same endpoints goes the long way, +X -> -Z -> -X -> +Z.
    let g3 = arc(a, b, (0.0, 0.0), true, ArcPlane::Xz);
    let am3 = g3.arc.unwrap_or(am);
    assert!((am3.radius_sweep(a, b).1 - 1.5 * PI).abs() < 1e-12);
    let third = am3.point_at(a, b, 1.0 / 3.0).unwrap_or(a);
    let two = am3.point_at(a, b, 2.0 / 3.0).unwrap_or(a);
    assert!(near(third, p(0.0, 0.0, -10.0), 1e-9), "first through -Z: {third:?}");
    assert!(near(two, p(-10.0, 0.0, 0.0), 1e-9), "then through -X: {two:?}");
}

#[test]
fn g19_reads_naturally_from_plus_x() {
    // Center (Y0, Z0); (Y10, Z0) -> (Y0, Z10) is the G3 quarter.
    let (a, b) = (p(0.0, 10.0, 0.0), p(0.0, 0.0, 10.0));
    let m = arc(a, b, (0.0, 0.0), true, ArcPlane::Yz);
    let am = m.arc.unwrap_or(ArcMove::xy(Point2::new(0.0, 0.0), false));
    assert!((am.radius_sweep(a, b).1 - PI / 2.0).abs() < 1e-12);
    let h = 10.0 / 2f64.sqrt();
    assert!(near(am.point_at(a, b, 0.5).unwrap_or(a), p(0.0, h, h), 1e-12));
    // G2 is the long way: first through -Z.
    let g2 = arc(a, b, (0.0, 0.0), false, ArcPlane::Yz);
    let am2 = g2.arc.unwrap_or(am);
    assert!(near(am2.point_at(a, b, 1.0 / 3.0).unwrap_or(a), p(0.0, 0.0, -10.0), 1e-9));
}

#[test]
fn vertical_tessellation_keeps_the_circle_and_exact_endpoints() {
    for (pl, a, b, c, ccw) in [
        (ArcPlane::Xz, p(10.0, 3.0, 0.0), p(0.0, 3.0, 10.0), (0.0, 0.0), false),
        (ArcPlane::Yz, p(-2.0, 10.0, 0.0), p(-2.0, 0.0, 10.0), (0.0, 0.0), true),
    ] {
        let m = arc(a, b, c, ccw, pl);
        let segs = m.segments3(0.01);
        assert!(segs.len() > 4, "{pl:?}: a fine fan");
        assert_eq!(segs.first().map(|s| s.0), Some(a), "{pl:?} starts exactly");
        assert_eq!(segs.last().map(|s| s.1), Some(b), "{pl:?} ends exactly");
        for w in segs.windows(2) {
            assert_eq!(w[0].1, w[1].0, "{pl:?}: the chords chain");
        }
        for (q, _) in &segs {
            let (u, v, wn) = pl.uvw(*q);
            assert!(((u * u + v * v).sqrt() - 10.0).abs() < 1e-9, "{pl:?} on the circle");
            assert!((wn - pl.uvw(a).2).abs() < 1e-12, "{pl:?}: planar, normal held");
        }
        // The XY projection is the footprint: one line, normal fixed.
        for (q, r) in m.chords(0.01) {
            match pl {
                ArcPlane::Xz => assert!(q.y == 3.0 && r.y == 3.0),
                _ => assert!(q.x == -2.0 && r.x == -2.0),
            }
        }
    }
}

#[test]
fn a_vertical_helix_rises_along_the_normal() {
    // XZ quarter with a Y rise of 5: length over the rise, Y linear.
    let (a, b) = (p(10.0, 0.0, 0.0), p(0.0, 5.0, 10.0));
    let m = arc(a, b, (0.0, 0.0), false, ArcPlane::Xz);
    let am = m.arc.unwrap_or(ArcMove::xy(Point2::new(0.0, 0.0), false));
    let want = ((10.0 * PI / 2.0).powi(2) + 25.0).sqrt();
    assert!((am.length(a, b).unwrap_or(0.0) - want).abs() < 1e-12);
    assert!((am.point_at(a, b, 0.5).unwrap_or(a).y - 2.5).abs() < 1e-12);
}

#[test]
fn the_builder_splits_a_vertical_arc_at_its_z_extremum() {
    // G2 in XZ from X-10 to X10 about (X0, Z0): seen from +Y the
    // viewer's right is -X, so clockwise from there runs DOWN, under
    // the bottom (Z-10): two Z-monotone arcs, joined there.
    let mut b = PathBuilder::new(p(-10.0, 0.0, 0.0));
    b.push_arc_in(MoveKind::Feed, p(10.0, 0.0, 0.0), Some(100.0), Point2::new(0.0, 0.0), false, ArcPlane::Xz);
    let t = b.finish();
    assert_eq!(t.moves.len(), 2, "split at the bottom");
    assert_eq!(t.moves[0].to, p(0.0, 0.0, -10.0));
    assert!(t.is_chained());
    for m in &t.moves {
        let a = m.arc.unwrap_or(ArcMove::xy(Point2::new(9.0, 9.0), true));
        assert_eq!((a.plane, a.ccw, a.center), (ArcPlane::Xz, false, Point2::new(0.0, 0.0)));
    }
    // A quarter that ENDS at its bottom (the entry-arc shape) is
    // already monotone: one move.
    let mut b = PathBuilder::new(p(20.0, 0.0, 5.0));
    b.push_arc_in(MoveKind::Feed, p(15.0, 0.0, 0.0), Some(100.0), Point2::new(15.0, 5.0), true, ArcPlane::Xz);
    assert_eq!(b.finish().moves.len(), 1);
    // An XY call through the plane door is push_arc exactly.
    let mut b1 = PathBuilder::new(p(10.0, 0.0, 0.0));
    b1.push_arc_in(MoveKind::Feed, p(0.0, 10.0, 0.0), Some(1.0), Point2::new(0.0, 0.0), true, ArcPlane::Xy);
    let mut b2 = PathBuilder::new(p(10.0, 0.0, 0.0));
    b2.push_arc(MoveKind::Feed, p(0.0, 10.0, 0.0), Some(1.0), Point2::new(0.0, 0.0), true);
    assert_eq!(b1.finish(), b2.finish());
    // No in-plane motion is a straight move, never a zero arc.
    let mut b = PathBuilder::new(p(0.0, 0.0, 0.0));
    b.push_arc_in(MoveKind::Feed, p(0.0, 4.0, 0.0), Some(1.0), Point2::new(5.0, 5.0), true, ArcPlane::Xz);
    assert!(b.finish().moves[0].arc.is_none());
}

#[test]
fn thread_dip_chords_interpolate_radius_with_axial_travel() {
    // A half turn of the golden dip: the controller moves linearly
    // from r5.6 to r5.9 while advancing one half lead.
    for ccw in [false, true] {
        let m = arc(
            p(5.6, 0.0, 20.0),
            p(-5.9, 0.0, 19.0),
            (0.0, 0.0),
            ccw,
            ArcPlane::Xy,
        );
        let chords = m.segments3(0.001);
        assert!(chords.len() > 20);
        assert_eq!(chords[0].0, m.from);
        assert_eq!(chords.last().unwrap().1, m.to);
        for (_, q) in &chords {
            let f = 20.0 - q.z;
            assert!(
                (q.x.hypot(q.y) - (5.6 + 0.3 * f)).abs() < 1e-10,
                "radius must follow the synchronized axial ramp: {q:?}"
            );
        }
        // Choose the chord nearest the interior half-turn midpoint;
        // its radius must be near 5.75, never the outer radius 5.9.
        let mid = chords
            .iter()
            .map(|(_, q)| q)
            .min_by(|a, b| (a.z - 19.5).abs().total_cmp(&(b.z - 19.5).abs()))
            .unwrap();
        assert!((mid.x.hypot(mid.y) - 5.75).abs() < 0.01, "{mid:?}");
        assert_eq!(mid.y.is_sign_positive(), ccw);
    }
}

#[test]
fn thread_dip_chords_bound_linear_radius_ramp_error() {
    // A steep radius ramp stresses the tangential contribution to
    // chord error; a circular sagitta bound alone is insufficient.
    let sweep: f64 = 0.1;
    let m = arc(
        p(1.0, 0.0, 0.0),
        p(10.0 * sweep.cos(), 10.0 * sweep.sin(), 1.0),
        (0.0, 0.0),
        true,
        ArcPlane::Xy,
    );
    let tol = 0.001;
    for (a, b) in m.segments3(tol) {
        for u in [0.25, 0.5, 0.75] {
            let f = a.z + (b.z - a.z) * u;
            let radius = 1.0 + 9.0 * f;
            let exact = p(radius * (sweep * f).cos(), radius * (sweep * f).sin(), f);
            let chord = p(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u, f);
            assert!(
                (exact.x - chord.x).hypot(exact.y - chord.y) <= tol,
                "radius ramp chords must honor tolerance at {f}: {exact:?} vs {chord:?}"
            );
        }
    }
}
