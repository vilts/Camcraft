//! #3627: the plain hop of `march::emit_runs`, driven with hand-made
//! runs so that the order of the runs is a choice of the test (a high
//! run first, then a low one: the inner-first order of a convex part).
//! Fixture: a mesa - a top at z 10 over (-4..4)^2 on a base at z 5 over
//! (-12..12)^2.

use crate::cl::{MoveKind, PathBuilder};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

use super::gaplink::straight_ok;
use super::march::emit_runs;
use super::params::{Finish3DParams, FinishPattern, GapLink};
use super::support::build_sups;

const R: f64 = 3.0;
const TOP: f64 = 10.0;
const BASE: f64 = 5.0;
const CLEAR: f64 = 25.0;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn mesa() -> Vec<Triangle> {
    let quad = |q: [Point3; 4]| vec![Triangle::new(q[0], q[1], q[2]), Triangle::new(q[0], q[2], q[3])];
    let (t, b) = (TOP, BASE);
    let mut f = Vec::new();
    f.extend(quad([p(-4.0, -4.0, t), p(4.0, -4.0, t), p(4.0, 4.0, t), p(-4.0, 4.0, t)]));
    f.extend(quad([p(-4.0, -4.0, b), p(4.0, -4.0, b), p(4.0, -4.0, t), p(-4.0, -4.0, t)]));
    f.extend(quad([p(4.0, -4.0, b), p(4.0, 4.0, b), p(4.0, 4.0, t), p(4.0, -4.0, t)]));
    f.extend(quad([p(4.0, 4.0, b), p(-4.0, 4.0, b), p(-4.0, 4.0, t), p(4.0, 4.0, t)]));
    f.extend(quad([p(-4.0, 4.0, b), p(-4.0, -4.0, b), p(-4.0, -4.0, t), p(-4.0, 4.0, t)]));
    f.extend(quad([p(-12.0, -12.0, b), p(12.0, -12.0, b), p(12.0, -4.0, b), p(-12.0, -4.0, b)]));
    f.extend(quad([p(-12.0, 4.0, b), p(12.0, 4.0, b), p(12.0, 12.0, b), p(-12.0, 12.0, b)]));
    f.extend(quad([p(-12.0, -4.0, b), p(-4.0, -4.0, b), p(-4.0, 4.0, b), p(-12.0, 4.0, b)]));
    f.extend(quad([p(4.0, -4.0, b), p(12.0, -4.0, b), p(12.0, 4.0, b), p(4.0, 4.0, b)]));
    f
}

fn params(retract_z: Option<f64>) -> Finish3DParams {
    Finish3DParams {
        gap_link: GapLink::Jump,
        pattern: FinishPattern::Contour,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: 0.1,
        clearance_z: CLEAR,
        retract_z,
        top_z: TOP,
        bottom_z: BASE,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

/// A run of center samples on a flat at tip height `z`, along x at y.
fn run(x0: f64, x1: f64, y: f64, z: f64) -> Vec<(Point2, f64)> {
    (0..=4).map(|k| (Point2::new(x0 + (x1 - x0) * f64::from(k) / 4.0, y), z + R)).collect()
}

/// The moves of the link from the end of the first run to the plunge
/// of the second: everything after the first run's last sample, up to
/// and with the plunge.
fn link(runs: &[Vec<(Point2, f64)>], retract_z: Option<f64>) -> Vec<(MoveKind, Point3)> {
    let f = mesa();
    let sups = build_sups(&f, R);
    let mut b = PathBuilder::new(p(0.0, 0.0, CLEAR));
    emit_runs(&mut b, &params(retract_z), R, &sups, runs, false);
    let moves = b.moves();
    // The first plunge is the entry of run 0; the second is the entry
    // of run 1. The link is the moves after run 0's last feed, up to
    // and with that second plunge.
    let plunges: Vec<usize> =
        moves.iter().enumerate().filter(|(_, m)| m.kind == MoveKind::Plunge).map(|(i, _)| i).collect();
    let second = plunges.get(1).copied().unwrap_or(moves.len() - 1);
    let last_feed = moves[..second]
        .iter()
        .rposition(|m| m.kind == MoveKind::Feed && (m.to.z - (runs[0][0].1 - R)).abs() < 1e-9)
        .unwrap_or(0);
    moves[last_feed + 1..=second].iter().map(|m| (m.kind, m.to)).collect()
}

/// The law of the hop in one assert: never down at the end of the run,
/// and the move across is clear of the facets or at clearance.
fn assert_safe(l: &[(MoveKind, Point3)], from: Point3, label: &str) {
    let sups = build_sups(&mesa(), R);
    let mut at = from;
    for &(kind, to) in l {
        let same_xy = (to.x - at.x).hypot(to.y - at.y) < 1e-9;
        if same_xy && kind != MoveKind::Plunge {
            assert!(to.z >= at.z - 1e-9, "{label}: the hop goes down at the run end: {at:?} -> {to:?}");
        }
        if !same_xy {
            assert!(
                (to.z - CLEAR).abs() < 1e-9 || straight_ok(at, to, R, &sups),
                "{label}: the move across {at:?} -> {to:?} is not clear of the part"
            );
        }
        at = to;
    }
}

/// Inner-first on a convex part: the high run on the mesa top, then a
/// low run on the base. A retract at 7 is below the high run end.
#[test]
fn high_run_first_with_a_low_retract_never_digs_into_the_mesa() {
    let runs = vec![run(-2.0, 2.0, 0.0, TOP), run(9.0, 9.0, -2.0, BASE)];
    let l = link(&runs, Some(7.0));
    assert_safe(&l, p(2.0, 0.0, TOP), "mesa, retract 7");
    assert_eq!(l.last().map(|m| m.0), Some(MoveKind::Plunge), "{l:?}");
}

/// A ridge between two low runs: the retract (7) is above both run
/// ends but below the mesa top (10) between them. The move across at 7
/// goes through the mesa; it must go over it.
#[test]
fn a_retract_below_a_ridge_between_the_runs_goes_over_it() {
    let runs = vec![run(-9.0, -9.0, 0.0, BASE), run(9.0, 9.0, 0.0, BASE)];
    let l = link(&runs, Some(7.0));
    assert_safe(&l, p(-9.0, 0.0, BASE), "ridge, retract 7");
}

/// The usual setup: a retract above the part. The hop is the plain hop
/// of every older file, move for move.
#[test]
fn a_retract_above_the_part_is_the_plain_hop() {
    let runs = vec![run(-2.0, 2.0, 0.0, TOP), run(9.0, 9.0, -2.0, BASE)];
    let l = link(&runs, Some(15.0));
    assert_eq!(
        l,
        vec![
            (MoveKind::Feed, p(2.0, 0.0, 15.0)),
            (MoveKind::Rapid, p(9.0, -2.0, 15.0)),
            (MoveKind::Plunge, p(9.0, -2.0, BASE)),
        ]
    );
}
