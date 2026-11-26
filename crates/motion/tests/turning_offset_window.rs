//! Pins of #2881: the line claim of a profile segment holds only at
//! the stations where the ball's tangent point lies on the segment.
//! Fixtures are hand-made (station, radius) polylines, so each
//! expected ride follows from plane geometry alone.
//!
//! Gate map (M4.4): covers camcraft_motion::turning::offset.

use camcraft_motion::turning::offset::{profile_offset, profile_offset_in};

const TOL: f64 = 1e-3;

/// The distance from a (station, radius) point to a polyline.
fn dist_to(s: f64, r: f64, pts: &[(f64, f64)]) -> f64 {
    let mut d = f64::INFINITY;
    for w in pts.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (((s - x0) * dx + (r - y0) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (px, py) = (x0 + dx * t, y0 + dy * t);
        d = d.min(((s - px) * (s - px) + (r - py) * (r - py)).sqrt());
    }
    d
}

/// Every sample of a ride stands at exactly `d` from the profile:
/// nearer is a gouge, farther is a ride through air.
fn assert_rides_at(ride: &[(f64, f64)], pts: &[(f64, f64)], d: f64, what: &str) {
    for &(s, r) in ride {
        let got = dist_to(s, r, pts);
        assert!(
            (got - d).abs() < 1e-6,
            "{what}: ride at station {s} r {r} stands {got} from the profile, not {d}"
        );
    }
}

/// A shoulder with the first chord of a fillet: slope 16.6, 0.1 long
/// in station. The ball of radius 1 rolls over it; the ride never
/// rises above the top flat plus the radius.
fn steep_chord() -> Vec<(f64, f64)> {
    vec![(0.0, 10.0), (5.0, 10.0), (5.1, 11.66), (10.0, 11.66)]
}

#[test]
fn a_steep_fillet_chord_keeps_the_ride_on_the_nose() {
    let pts = steep_chord();
    let ride = profile_offset(&pts, 1.0, TOL, 1.0, 9.0).expect("a ride");
    let top = ride.iter().fold(0.0f64, |m, q| m.max(q.1));
    assert!(top <= 11.66 + 1.0 + 1e-9, "the ride leaves the bar: max r {top}");
    assert_rides_at(&ride, &pts, 1.0, "steep chord");
}

#[test]
fn a_sloped_inside_corner_is_not_gouged() {
    // A flat that meets a 45 degree face: the face's offset line
    // claims stations BEFORE the corner (its tangent points are on
    // the face there), and the flat's claim alone is too low.
    let pts = vec![(-10.0, 10.0), (0.0, 10.0), (10.0, 20.0), (20.0, 20.0)];
    let ride = profile_offset(&pts, 1.0, TOL, -5.0, 15.0).expect("a ride");
    assert_rides_at(&ride, &pts, 1.0, "inside corner");
    // And the mirror: a 45 degree face down onto a flat.
    let pts = vec![(-10.0, 20.0), (0.0, 20.0), (10.0, 10.0), (20.0, 10.0)];
    let ride = profile_offset(&pts, 1.0, TOL, -5.0, 15.0).expect("a ride");
    assert_rides_at(&ride, &pts, 1.0, "falling face");
}

#[test]
fn the_inside_ride_obeys_the_same_window() {
    // A bore wall with a steep chord down to a smaller bore: the
    // inside ride must not dive toward the axis at the chord.
    let pts = vec![(0.0, 20.0), (5.0, 20.0), (5.1, 18.34), (10.0, 18.34)];
    let ride = profile_offset_in(&pts, 1.0, TOL, 1.0, 9.0).expect("an inside ride");
    let low = ride.iter().fold(f64::INFINITY, |m, q| m.min(q.1));
    assert!(low >= 18.34 - 1.0 - 1e-9, "the inside ride dives: min r {low}");
    assert_rides_at(&ride, &pts, 1.0, "inside steep chord");
    // A sloped inside corner of a bore.
    let pts = vec![(-10.0, 20.0), (0.0, 20.0), (10.0, 10.0), (20.0, 10.0)];
    let ride = profile_offset_in(&pts, 1.0, TOL, -5.0, 15.0).expect("an inside ride");
    assert_rides_at(&ride, &pts, 1.0, "inside sloped corner");
}
