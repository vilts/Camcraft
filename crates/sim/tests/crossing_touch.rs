//! #1129: the crossing law (a) reads a shared vertex a rounding apart
//! (within `CROSSING_TOUCH`, 1e-9 mm) as a touch, as it always read an
//! exactly shared one - so a vertex reached twice no longer holds the
//! post. A real crossing a micron through still reports, at every chord
//! length; a walk passing through a chord at a vertex a picometre off
//! its interior still reports (only END points are excused); collinear
//! overlaps keep the exact reading.
//!
//! Gate map (#1129): covers camcraft_sim::verify (crossing).
//!
//! Provenance: the orientation predicate (O'Rourke, "Computational
//! Geometry in C") the law already cites; our own fixtures.

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{crossed_pairs, crossed_pairs_all, CROSSING_TOUCH};

const Z: f64 = -1.0;

/// An XY point and an XY chord.
type P = (f64, f64);
type Seg = (P, P);

fn feed(a: (f64, f64), b: (f64, f64)) -> Move {
    Move::new(MoveKind::Feed, Point3::new(a.0, a.1, Z), Point3::new(b.0, b.1, Z), Some(400.0))
}

fn path(chords: &[Seg]) -> Toolpath {
    Toolpath { moves: chords.iter().map(|&(a, b)| feed(a, b)).collect(), ..Toolpath::default() }
}

/// Both readings of the law (all-pairs reference and bucketed) agree
/// and give these pairs.
fn pairs(t: &Toolpath) -> Vec<[usize; 2]> {
    let all = crossed_pairs_all(t, 0.01);
    assert_eq!(crossed_pairs(t, 0.01), all, "bucketed == all-pairs");
    all
}

/// The exact-sign predicate the law used before #1129, for checking
/// that a fixture really sits on the knife edge it is about.
fn exact(a0: (f64, f64), a1: (f64, f64), b0: (f64, f64), b1: (f64, f64)) -> bool {
    let o = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
    let (oa, ob, oc, od) = (o(a0, a1, b0), o(a0, a1, b1), o(b0, b1, a0), o(b0, b1, a1));
    oa != 0.0 && ob != 0.0 && oc != 0.0 && od != 0.0 && (oa > 0.0) != (ob > 0.0) && (oc > 0.0) != (od > 0.0)
}

/// Origins (a part at the table origin and a metre out) and chord
/// lengths (a micron to a metre) every pin is judged at.
const ORIGINS: [f64; 3] = [0.0, 37.25, 1000.0];
const LENGTHS: [f64; 4] = [1e-3, 0.5, 20.0, 1000.0];

/// Chord A runs along +X into its end `v` (at `ox, oy`); chord B leaves
/// from `v` jittered by `j`, up and to the right, length `len` each.
fn joint(ox: f64, oy: f64, len: f64, j: (f64, f64)) -> [Seg; 2] {
    let v = (ox, oy);
    let b0 = (ox + j.0, oy + j.1);
    [((ox - len, oy), v), (b0, (b0.0 + 0.5 * len, b0.1 + len))]
}

#[test]
fn a_shared_vertex_a_rounding_apart_does_not_warn() {
    let m = 1e-12;
    for ox in ORIGINS {
        for len in LENGTHS {
            let mut tripped = false;
            for k in 0..16 {
                let a = k as f64 * std::f64::consts::TAU / 16.0 + 0.1;
                let [ca, cb] = joint(ox, -ox * 0.5, len, (m * a.cos(), m * a.sin()));
                tripped |= exact(ca.0, ca.1, cb.0, cb.1);
                assert_eq!(pairs(&path(&[ca, cb])), Vec::<[usize; 2]>::new(), "origin {ox}, length {len}, jitter {k}");
            }
            // The knife edge is really exercised: the exact-sign law
            // read at least one of these joints as a crossing.
            assert!(tripped, "origin {ox}, length {len}: no jitter tripped the exact test");
        }
    }
}

#[test]
fn a_crossing_a_micron_through_still_warns() {
    // B starts a micron short of A's end and a micron below its line
    // (a quarter of the chord for the 1 um chord, which a micron would
    // overshoot), so it passes through A's interior: at least 0.35 um
    // from the shared vertex, far past the 1 pm touch.
    for ox in ORIGINS {
        for len in LENGTHS {
            let d = (0.25 * len).min(1e-3);
            let [ca, cb] = joint(ox, -ox * 0.5, len, (-d, -d));
            assert_eq!(pairs(&path(&[ca, cb])), vec![[0, 1]], "origin {ox}, length {len}");
        }
    }
}

#[test]
fn a_walk_through_a_chord_at_a_vertex_a_picometre_off_its_line_still_warns() {
    // The #1134 review's probe: a lap (0,0)-(4,0) and a walk down X = 2
    // whose middle vertex stands 0.5 pm off the lap's interior. Each of
    // the walk's chords ends "on the lap's line" within the tolerance,
    // but no END of the lap is near: the exact sign keeps the law, and
    // the pass-through reports.
    for oy in [0.0, 1000.0] {
        for dy in [-0.5e-9, 0.5e-9] {
            let t = path(&[((0.0, oy), (4.0, oy)), ((2.0, oy + 1.0), (2.0, oy + dy)), ((2.0, oy + dy), (2.0, oy - 1.0))]);
            let got = pairs(&t);
            assert!(!got.is_empty(), "lap y {oy}, vertex {dy}: the pass-through was hidden");
            assert!(got.iter().all(|p| p[0] == 0), "lap y {oy}, vertex {dy}: {got:?}");
        }
    }
}

#[test]
fn collinear_overlaps_keep_their_reading() {
    // Partly overlapping collinear chords share no end point, so the
    // law reads them exactly as the exact-sign test did, at any angle.
    for ang in [0.0, 0.3, 1.0, std::f64::consts::FRAC_PI_4] {
        let (c, s) = (f64::cos(ang), f64::sin(ang));
        for (o, len) in [(0.0, 2.0), (1000.0, 2.0), (5.0, 1e-3)] {
            let p = |u: f64| (o + u * c, o * 0.25 + u * s);
            let (a0, a1, b0, b1) = (p(0.0), p(len), p(0.5 * len), p(1.5 * len));
            let want = if exact(a0, a1, b0, b1) { vec![[0, 1]] } else { Vec::new() };
            assert_eq!(pairs(&path(&[(a0, a1), (b0, b1)])), want, "angle {ang}, origin {o}, length {len}");
        }
    }
    // An exact retrace (the same chord back) is a touch, as it was.
    assert_eq!(pairs(&path(&[((0.0, 0.0), (2.0, 0.0)), ((2.0, 0.0), (0.0, 0.0))])), Vec::<[usize; 2]>::new());
}

#[test]
fn a_joint_just_past_the_touch_keeps_the_exact_reading() {
    // Twice the tolerance apart is two vertices, not one: wherever the
    // exact-sign test reads that joint as a crossing, the law still does.
    let m = 2.0 * CROSSING_TOUCH;
    for len in LENGTHS {
        let mut seen = 0;
        for k in 0..16 {
            let a = k as f64 * std::f64::consts::TAU / 16.0 + 0.1;
            let [ca, cb] = joint(0.0, 0.0, len, (m * a.cos(), m * a.sin()));
            let want = if exact(ca.0, ca.1, cb.0, cb.1) { vec![[0, 1]] } else { Vec::new() };
            seen += want.len();
            assert_eq!(pairs(&path(&[ca, cb])), want, "length {len}, jitter {k}");
        }
        assert!(seen > 0, "length {len}: no jitter crossed");
    }
}
