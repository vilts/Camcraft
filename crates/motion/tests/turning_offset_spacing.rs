//! Pins issue #2882: the ride of a ball outside or inside a turned profile.
//! It has no move shorter than half the chord share, keeps both window
//! ends, keeps every sample on the nose (distance d from the profile), and
//! keeps each chord within 2 * tol of the true curve.
//!
//! Gate map (M4.4): covers camcraft_motion::turning::offset (ride
//! sample spacing) with literal (station, radius) profiles.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::turning::offset::{profile_offset, profile_offset_in};

const NOSE: f64 = 0.8;
const TOLS: [f64; 3] = [1e-2, 2.5e-3, 1e-4];

const OD_STEP: &[(f64, f64)] = &[(-16.0, 6.0), (2.0, 6.0), (2.0, 5.0), (16.0, 5.0)];
const OD_COLLAR: &[(f64, f64)] = &[
    (-16.0, 5.0),
    (-2.0, 5.0),
    (-2.0, 7.0),
    (2.0, 7.0),
    (2.0, 5.0),
    (16.0, 5.0),
];
const ID_WALL: &[(f64, f64)] = &[(-16.0, 3.0), (0.0, 3.0), (0.0, 4.0), (16.0, 4.0)];

/// Least Euclidean distance from (s, r) to the segments of a polyline.
fn dist_to(s: f64, r: f64, pts: &[(f64, f64)]) -> f64 {
    pts.windows(2)
        .map(|w| {
            let (ax, ay) = w[0];
            let (bx, by) = w[1];
            let (dx, dy) = (bx - ax, by - ay);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 {
                (((s - ax) * dx + (r - ay) * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (ax + t * dx - s).hypot(ay + t * dy - r)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Least consecutive hypot distance of a ride, and the index of its first sample.
fn min_gap(cl: &[(f64, f64)]) -> (f64, usize) {
    cl.windows(2)
        .enumerate()
        .map(|(i, w)| ((w[1].0 - w[0].0).hypot(w[1].1 - w[0].1), i))
        .fold((f64::INFINITY, 0), |best, cur| {
            if cur.0 < best.0 {
                cur
            } else {
                best
            }
        })
}

fn check_ends_and_nose(
    name: &str,
    cl: &[(f64, f64)],
    pts: &[(f64, f64)],
    s_lo: f64,
    s_hi: f64,
    tol: f64,
) {
    let first = cl.first().expect("a ride has samples");
    let last = cl.last().expect("a ride has samples");
    assert!(
        (first.0 - s_lo).abs() < 1e-12,
        "{name}: tol {tol}, first station {}, want {s_lo}",
        first.0
    );
    assert!(
        (last.0 - s_hi).abs() < 1e-12,
        "{name}: tol {tol}, last station {}, want {s_hi}",
        last.0
    );
    for &(s, r) in cl {
        let d = dist_to(s, r, pts);
        assert!(
            (d - NOSE).abs() < 1e-6,
            "{name}: tol {tol}, sample at station {s}, distance {d}"
        );
    }
}

fn check_chords(name: &str, cl: &[(f64, f64)], pts: &[(f64, f64)], tol: f64) {
    let bound = NOSE - 2.0 * tol - 1e-9;
    for w in cl.windows(2) {
        for k in 1..=9 {
            let t = f64::from(k) / 10.0;
            let s = w[0].0 + t * (w[1].0 - w[0].0);
            let r = w[0].1 + t * (w[1].1 - w[0].1);
            let d = dist_to(s, r, pts);
            assert!(
                d >= bound,
                "{name}: tol {tol}, chord point at station {s}, distance {d}, bound {bound}"
            );
        }
    }
}

#[test]
fn od_ride_has_no_move_shorter_than_half_the_share() {
    for (name, pts) in [("OD stepped bar", OD_STEP), ("OD collar", OD_COLLAR)] {
        for tol in TOLS {
            let cl = profile_offset(pts, NOSE, tol, -16.0, 16.0).expect("a ride");
            let (gap, i) = min_gap(&cl);
            assert!(
                gap >= 0.5 * tol * (1.0 - 1e-9),
                "{name}: tol {tol}, gap {gap}, index {i}"
            );
        }
    }
}

#[test]
fn a_window_end_next_to_a_candidate_keeps_the_gap() {
    // The window stops 1e-7 past the candidate station 2 - nose (the
    // step corner's s - d): the sample there and the window's end
    // stand a hair apart. The end always survives, so the give-way
    // at the end (not the forward pass) must keep the gap.
    let s_hi = 2.0 - NOSE + 1e-7;
    for tol in TOLS {
        let cl = profile_offset(OD_STEP, NOSE, tol, -16.0, s_hi).expect("a ride");
        let (gap, i) = min_gap(&cl);
        assert!(
            gap >= 0.5 * tol * (1.0 - 1e-9),
            "window end at {s_hi}: tol {tol}, gap {gap}, index {i} of {}",
            cl.len()
        );
        check_ends_and_nose("OD stepped bar to 1.2", &cl, OD_STEP, -16.0, s_hi, tol);
        check_chords("OD stepped bar to 1.2", &cl, OD_STEP, tol);
    }
}

#[test]
fn id_ride_has_no_move_shorter_than_half_the_share() {
    for tol in TOLS {
        let cl = profile_offset_in(ID_WALL, NOSE, tol, -15.0, 15.0).expect("a ride");
        let (gap, i) = min_gap(&cl);
        assert!(
            gap >= 0.5 * tol * (1.0 - 1e-9),
            "ID stepped hole wall: tol {tol}, gap {gap}, index {i}"
        );
    }
}

#[test]
fn thinning_keeps_the_window_ends_and_the_nose() {
    for tol in TOLS {
        let od = profile_offset(OD_STEP, NOSE, tol, -16.0, 16.0).expect("a ride");
        check_ends_and_nose("OD stepped bar", &od, OD_STEP, -16.0, 16.0, tol);

        let id = profile_offset_in(ID_WALL, NOSE, tol, -15.0, 15.0).expect("a ride");
        check_ends_and_nose("ID stepped hole wall", &id, ID_WALL, -15.0, 15.0, tol);
    }
}

#[test]
fn thinned_chords_stay_within_the_bound() {
    for tol in TOLS {
        for (name, pts) in [("OD stepped bar", OD_STEP), ("OD collar", OD_COLLAR)] {
            let cl = profile_offset(pts, NOSE, tol, -16.0, 16.0).expect("a ride");
            check_chords(name, &cl, pts, tol);
        }
        let id = profile_offset_in(ID_WALL, NOSE, tol, -15.0, 15.0).expect("a ride");
        check_chords("ID stepped hole wall", &id, ID_WALL, tol);
    }
}
