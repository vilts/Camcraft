//! Pins of the plan's crossing check (`guard.rs`; issue #1048: a
//! shared end point a rounding apart - within `TOUCH` of the other
//! chord's END point - is no crossing; a real crossing is, a walk
//! passing through a chord a picometre off its line included). Ours.

use super::*;

/// Offsets of one vertex reached twice: rounding-sized, in eight
/// directions, at the 1e-15 of #980 and the 1e-12 of #1048's "Done".
fn jitters() -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for m in [1e-15, 1e-12] {
        for k in 0..8 {
            let a = k as f64 * std::f64::consts::FRAC_PI_4 + 0.3;
            out.push((m * a.cos(), m * a.sin()));
        }
    }
    out
}

fn off(p: Point2, j: (f64, f64)) -> Point2 {
    Point2::new(p.x + j.0, p.y + j.1)
}

/// The exact-sign test read at least one jitter of the pair as a proper
/// crossing - the knife edge the case is about is really exercised.
fn old_test_tripped(pairs: &[(Point2, Point2, Point2, Point2)]) -> bool {
    pairs.iter().any(|&(a, b, c, d)| proper(a, b, c, d))
}

#[test]
fn a_figure_eight_crosses_and_a_spiral_does_not() {
    let eight = vec![Point2::new(0.0, 0.0), Point2::new(2.0, 2.0), Point2::new(2.0, 0.0), Point2::new(0.0, 2.0)];
    assert!(first_crossing(&[eight]).is_some());
    let spiral: Vec<Point2> = (0..2000)
        .map(|k| {
            let a = k as f64 * 0.02;
            let r = 1.0 + 0.1 * a;
            Point2::new(r * a.cos(), r * a.sin())
        })
        .collect();
    assert!(first_crossing(&[spiral]).is_none());
}

/// A spiral that orbits its seed and leaves from a closing point a
/// rounding off its start (#980's walk, kept un-snapped here).
#[test]
fn a_spiral_returning_to_its_seed_within_rounding_does_not_cross() {
    let (seed, r) = (Point2::new(3.7, -1.2), 2.0);
    let ring: Vec<Point2> = (0..=48)
        .map(|k| {
            let a = k as f64 * std::f64::consts::TAU / 48.0 + std::f64::consts::PI;
            Point2::new(seed.x + r * a.cos(), seed.y + r * a.sin())
        })
        .collect();
    let start = ring[0];
    let mut tripped = Vec::new();
    for j in jitters() {
        let mut walk: Vec<Point2> = ring[..48].to_vec();
        walk.push(off(start, j));
        walk.extend(ring[1..20].iter().enumerate().map(|(k, p)| {
            let f = 1.0 + 0.01 * (k + 1) as f64;
            Point2::new(seed.x + (p.x - seed.x) * f, seed.y + (p.y - seed.y) * f)
        }));
        tripped.push((walk[0], walk[1], walk[48], walk[49]));
        assert!(first_crossing(&[walk]).is_none(), "jitter {j:?}");
    }
    assert!(old_test_tripped(&tripped), "no jitter reached the exact-sign knife edge");
}

/// A microlift's return lands on a chord end of the laps already cut,
/// a rounding off it, and leaves at an angle.
#[test]
fn a_return_landing_on_a_chord_end_does_not_cross() {
    let lap = vec![Point2::new(0.0, 0.0), Point2::new(5.0, 0.3), Point2::new(9.0, 2.1), Point2::new(12.0, 5.0)];
    let mut tripped = Vec::new();
    for j in jitters() {
        for back in [Point2::new(4.0, 6.0), Point2::new(6.0, -4.0), Point2::new(1.0, 0.1)] {
            let ret = vec![back, off(lap[1], j), Point2::new(7.0, 3.0)];
            tripped.push((lap[0], lap[1], ret[0], ret[1]));
            tripped.push((lap[1], lap[2], ret[0], ret[1]));
            tripped.push((lap[1], lap[2], ret[1], ret[2]));
            let walks = [lap.clone(), ret];
            assert!(first_crossing(&walks).is_none(), "jitter {j:?}, from {back:?}");
            assert!(crossing_pairs(&walks).is_empty());
        }
    }
    assert!(old_test_tripped(&tripped), "no jitter reached the exact-sign knife edge");
}

/// The next ring starts a rounding off a vertex of the ring before.
#[test]
fn a_ring_starting_on_the_previous_rings_vertex_does_not_cross() {
    let poly = |r: f64, n: usize| -> Vec<Point2> {
        (0..=n)
            .map(|k| {
                let a = k as f64 * std::f64::consts::TAU / n as f64;
                Point2::new(r * a.cos(), r * a.sin())
            })
            .collect()
    };
    let inner = poly(1.0, 24);
    let outer = poly(1.5, 24);
    let mut tripped = Vec::new();
    for j in jitters() {
        let mut next = vec![off(inner[0], j)];
        next.extend(outer.iter().skip(1).copied());
        tripped.push((inner[0], inner[1], next[0], next[1]));
        tripped.push((inner[23], inner[24], next[0], next[1]));
        assert!(first_crossing(&[inner.clone(), next]).is_none(), "jitter {j:?}");
    }
    assert!(old_test_tripped(&tripped), "no jitter reached the exact-sign knife edge");
}

/// A shared end does not excuse a real crossing beside it: a chord
/// leaving the shared vertex, turning back and crossing the lap 0.15 mm
/// on; and a turn a micron past the vertex's line.
#[test]
fn a_real_crossing_near_a_shared_point_still_crosses() {
    let (v, a, b) = (Point2::new(0.0, 0.0), Point2::new(-3.0, 0.0), Point2::new(3.0, 0.5));
    for j in jitters() {
        let w = off(v, j);
        let cut = vec![w, Point2::new(0.3, 1.0), Point2::new(0.000_001, -1.0)];
        assert!(first_crossing(&[vec![a, v, b], cut]).is_some(), "crossing near the vertex, {j:?}");
        // Through the lap a micron below its line, then back.
        let dip = vec![w, Point2::new(1.0, 2.0), Point2::new(2.0, 1.0 / 3.0 - 1e-6), Point2::new(2.5, 2.0)];
        assert!(first_crossing(&[vec![a, v, b], dip]).is_some(), "a micron through, {j:?}");
    }
}

/// Decided (#1048, after the #1134 review): an EXACT contact is no
/// crossing, as under the exact-sign law - a vertex exactly on another
/// chord's interior, a collinear overlap, a chord retraced (the planner
/// retraces a lifted piece's last chords:
/// `a_channel_is_grown_as_one_spiral_that_never_crosses`), a chord
/// leaving a shared end a rounding off it. A vertex past another
/// chord's interior keeps the exact sign: a rounding or a micron past
/// it crosses; a micron short does not.
#[test]
fn a_contact_is_no_crossing_and_a_micron_past_is() {
    let lap = vec![Point2::new(0.0, 0.0), Point2::new(4.0, 0.0)];
    let t = |dy: f64| vec![Point2::new(1.0, 2.0), Point2::new(2.0, dy), Point2::new(3.0, 2.0)];
    for dy in [0.0, 1e-12, 5e-10] {
        assert!(first_crossing(&[lap.clone(), t(dy)]).is_none(), "T-touch at {dy}");
    }
    for dy in [-1e-12, -5e-10, -1e-6] {
        assert!(first_crossing(&[lap.clone(), t(dy)]).is_some(), "past the interior by {dy}");
    }
    assert!(first_crossing(&[lap.clone(), t(1e-6)]).is_none(), "a micron short");
    let (p, q) = (Point2::new(0.0, 0.0), Point2::new(4.0, 0.0));
    assert!(!crosses(p, q, Point2::new(1.0, 0.0), Point2::new(6.0, 0.0)), "collinear overlap");
    for j in jitters() {
        assert!(!crosses(p, q, off(q, j), off(p, j)), "retraced, {j:?}");
        assert!(!crosses(p, q, off(p, j), Point2::new(2.0, 1e-13)), "along from the shared end, {j:?}");
    }
}

/// The #1134 review's probe: a walk passing straight through a chord at
/// a vertex 0.5 pm off the chord's interior (either side), or exactly
/// on it, must be reported - at small and at metre coordinates. Reading
/// "within TOUCH of the other chord's LINE" as a touch hid it (both of
/// the walk's chords there read as touching); only end points are
/// excused now.
#[test]
fn a_walk_through_a_chord_at_a_vertex_a_picometre_off_its_line_still_crosses() {
    for (ox, oy) in [(0.0, 0.0), (37.25, -12.5), (1000.0, 1000.0), (-998.5, 1001.0)] {
        let lap = vec![Point2::new(ox, oy), Point2::new(ox + 4.0, oy)];
        for dy in [-0.5e-9, 0.5e-9] {
            let walk = vec![Point2::new(ox + 2.0, oy + 1.0), Point2::new(ox + 2.0, oy + dy), Point2::new(ox + 2.0, oy - 1.0)];
            assert!(first_crossing(&[lap.clone(), walk.clone()]).is_some(), "origin ({ox}, {oy}), vertex {dy} off");
            assert_eq!(crossing_pairs(&[lap.clone(), walk]), vec![(0, 1)], "origin ({ox}, {oy}), vertex {dy} off");
        }
    }
    // The review's exact reproduction.
    let probe = [vec![Point2::new(0.0, 0.0), Point2::new(4.0, 0.0)], vec![Point2::new(2.0, 1.0), Point2::new(2.0, -0.5e-9), Point2::new(2.0, -1.0)]];
    assert!(first_crossing(&probe).is_some());
}

/// A NaN or infinite point is reported, never skipped.
#[test]
fn a_non_finite_point_reads_as_a_crossing() {
    for bad in [Point2::new(f64::NAN, 0.0), Point2::new(0.0, f64::NAN), Point2::new(f64::INFINITY, 1.0), Point2::new(1.0, f64::NEG_INFINITY)] {
        let walk = vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0), bad, Point2::new(2.0, 2.0)];
        assert!(first_crossing(std::slice::from_ref(&walk)).is_some(), "{bad:?}");
        let clean = vec![Point2::new(5.0, 5.0), Point2::new(6.0, 5.0)];
        assert_eq!(crossing_pairs(&[clean, walk]), vec![(1, 1)], "{bad:?}");
        assert!(crosses(Point2::new(0.0, 0.0), bad, Point2::new(3.0, 3.0), Point2::new(4.0, 4.0)));
    }
}

/// The bucketed scan finds every pair the all-pairs predicate does
/// (walks that reuse each other's vertices a rounding off, the case
/// #1048 is about).
#[test]
fn the_bucketed_scan_agrees_with_all_pairs() {
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    for round in 0..40 {
        let mut walks: Vec<Vec<Point2>> = Vec::new();
        for w in 0..6 {
            let mut pts = Vec::new();
            for _ in 0..8 {
                let p = if w > 0 && rnd() < 0.3 {
                    let prev = &walks[(rnd() * w as f64) as usize];
                    let q = prev[(rnd() * prev.len() as f64) as usize];
                    Point2::new(q.x + (rnd() - 0.5) * 1e-12, q.y + (rnd() - 0.5) * 1e-12)
                } else {
                    Point2::new(rnd() * 10.0, rnd() * 10.0)
                };
                pts.push(p);
            }
            walks.push(pts);
        }
        let mut want = Vec::new();
        let segs: Vec<(usize, usize, Point2, Point2)> =
            walks.iter().enumerate().flat_map(|(w, p)| p.windows(2).enumerate().map(move |(i, s)| (w, i, s[0], s[1]))).collect();
        for (u, s) in segs.iter().enumerate() {
            for t in &segs[u + 1..] {
                if s.0 == t.0 && s.1.abs_diff(t.1) <= 1 {
                    continue;
                }
                if crosses(s.2, s.3, t.2, t.3) {
                    want.push((s.0.min(t.0), s.0.max(t.0)));
                }
            }
        }
        want.sort_unstable();
        want.dedup();
        assert_eq!(crossing_pairs(&walks), want, "round {round}");
    }
}
