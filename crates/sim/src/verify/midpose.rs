//! The pose along a move whose pose CHANGES (M13.7's last owed item):
//! a coordinated 5-axis block leans from the pose held coming in to
//! the one it states while its tip travels, and the material the tool
//! sweeps at the poses between is as real as at either end. The solid
//! law (b2, [`super::solid`]) and the posed-contact law (f2,
//! [`super::contact`]) read a move at its end pose; this module hands
//! them the intermediate poses, by the controller's own rule, which is
//! [`super::posed_steel`]'s: the pose is modal (a move stating nothing
//! keeps the one held), the letters lerp between the two poses
//! ([`crate::cl::pose`]), the tip rides its straight line (an arc its
//! chords), and each piece is cut into intervals no longer than
//! [`STEP`] of travel at the stack's far end - tip travel plus reach
//! times the pose's turn - capped at [`MAX_STEPS`], never unbounded.
//!
//! Only a move whose two poses differ is this module's for the part
//! laws: a constant-pose or upright move yields no interval through
//! [`change`], so its law keeps its verdict and its cost
//! byte-identical. The fixture sweep ([`super::posed_steel`]) walks
//! EVERY posed move through [`intervals`] (a constant pose is the
//! same walk with no turn), and reads each interval's [`Interval::swing`]:
//! the farthest any pose INSIDE the interval strays from `am`, proven
//! from the letter rate rather than sampled at the ends - each letter
//! turns the axis at most one radian per radian, so the axis travels
//! no more than the letters' summed sweep (M13.8's continuous pose).
//!
//! Provenance: the modal pose law is RS-274's; bounded sampling of a
//! swept pose is textbook (Ericson, "Real-Time Collision Detection",
//! public material). Our own design; no vendor source consulted.

use crate::cl::{angles_to_axis, axis_to_angles, normalize_axis, Move, Toolpath};
use crate::geometry::Point3;

/// Interval travel cap at the stack's far end, mm (posed_steel's).
const STEP: f64 = 0.5;
/// Intervals per piece cap (posed_steel's).
const MAX_STEPS: usize = 4096;
/// Two poses closer than this (radians) are one pose: no change.
const SAME_POSE: f64 = 1e-9;

/// One interval of a posed move: the tip from `p0` to `p1` (`pm`
/// their midpoint) under the pose `am` held at the midpoint, the
/// poses `a0`/`a1` held at its ends, and `swing` the proven bound
/// (radians) on the angle between `am` and ANY pose the interval
/// holds - never under the end poses' own angles.
#[derive(Clone, Copy, Debug)]
pub(super) struct Interval {
    pub p0: Point3,
    pub p1: Point3,
    pub pm: Point3,
    pub am: [f64; 3],
    pub a0: [f64; 3],
    pub a1: [f64; 3],
    pub swing: f64,
}

/// A move's pose as stated: an axis, or `None` for the upright.
pub(super) type Pose = Option<[f64; 3]>;

/// The (from, to) pose of every move by the modal rule: the first
/// move starts on its own stated pose; every move starts on the pose
/// held coming in and ends on the one it states, or keeps the held
/// one when it states nothing. `None` is the upright.
pub(super) fn poses(path: &Toolpath) -> Vec<(Pose, Pose)> {
    let mut held: Option<[f64; 3]> = path.moves.first().and_then(|m| m.axis);
    path.moves
        .iter()
        .map(|m| {
            let from = held;
            let to = m.axis.or(held);
            held = to;
            (from, to)
        })
        .collect()
}

/// The unit poses of a move whose pose changes, `None` when it does
/// not (both the same axis, or both upright): that move is not this
/// module's.
pub(super) fn change(from: Pose, to: Pose) -> Option<([f64; 3], [f64; 3])> {
    let up = [0.0, 0.0, 1.0];
    let a = from.map_or(Some(up), normalize_axis).unwrap_or(up);
    let b = to.map_or(Some(up), normalize_axis).unwrap_or(up);
    (angle(a, b) > SAME_POSE).then_some((a, b))
}

/// The intervals of a posed move `m` from pose `a` to pose `b` (the
/// same pose twice: a constant-pose walk, no turn), `reach` the stack's
/// far end above the tip (mm) and `chord_tol` the arc chord budget.
/// Empty when the move is nowhere (non-finite) - a caller that must
/// fail closed reads the move's endpoints itself.
pub(super) fn intervals(m: &Move, a: [f64; 3], b: [f64; 3], reach: f64, chord_tol: f64) -> Vec<Interval> {
    walk(m, a, b, reach, chord_tol, true)
}

/// [`intervals`] cut by the pose's turn alone - [`STEP`] of travel at
/// the stack's far end from the TURN, none from the tip's own run: for
/// a law that sweeps the tip segment exactly (the fixture mesh law,
/// #339 slice 3) and needs the intervals only to bound the swing. A
/// constant pose is one interval per chord piece.
pub(super) fn turn_intervals(m: &Move, a: [f64; 3], b: [f64; 3], reach: f64, chord_tol: f64) -> Vec<Interval> {
    walk(m, a, b, reach, chord_tol, false)
}

fn walk(m: &Move, a: [f64; 3], b: [f64; 3], reach: f64, chord_tol: f64, by_travel: bool) -> Vec<Interval> {
    if !(m.from.is_finite() && m.to.is_finite()) {
        return Vec::new();
    }
    let la = axis_to_angles(a);
    let lb = axis_to_angles(b);
    // The letters' whole sweep in radians: the axis turns at most this
    // much over the move, whatever path it traces on the sphere.
    let rate: f64 = (0..3).map(|k| (lb[k] - la[k]).abs()).sum::<f64>().to_radians();
    let tol = if chord_tol.is_finite() && chord_tol > 0.0 { chord_tol } else { 0.01 };
    let reach = if reach.is_finite() { reach.max(0.0) } else { 0.0 };
    let pieces = m.segments3(tol);
    let total: f64 = pieces.iter().map(|(p, q)| dist(*p, *q)).sum();
    let mut done = 0.0f64;
    let mut out = Vec::new();
    for (pa, pb) in &pieces {
        let len = dist(*pa, *pb);
        let frac = |s: f64| if total > 0.0 { (done + len * s) / total } else { s };
        let turn = angle(pose(la, lb, frac(0.0)), pose(la, lb, frac(1.0)));
        let span = if by_travel { len + reach * turn } else { reach * turn };
        let n = if span.is_finite() { ((span / STEP).ceil() as usize).clamp(1, MAX_STEPS) } else { 1 };
        for j in 0..n {
            let (t0, t1) = (j as f64 / n as f64, (j + 1) as f64 / n as f64);
            let tm = 0.5 * (t0 + t1);
            let (a0, a1, am) = (pose(la, lb, frac(t0)), pose(la, lb, frac(t1)), pose(la, lb, frac(tm)));
            // From the midpoint to either end the letters sweep half the
            // interval's share of the move; a non-finite share fails
            // closed (the capsule law reads NaN as a hit).
            let bound = 0.5 * rate * (frac(t1) - frac(t0)).abs();
            let swing = angle(am, a0).max(angle(am, a1)).max(bound);
            let swing = if bound.is_finite() { swing } else { f64::NAN };
            out.push(Interval {
                p0: lerp(*pa, *pb, t0),
                p1: lerp(*pa, *pb, t1),
                pm: lerp(*pa, *pb, tm),
                am,
                a0,
                a1,
                swing,
            });
        }
        done += len;
    }
    out
}

/// The axis the letters hold `s` of the way from `a` to `b` (each
/// letter on its own straight line); upright when no vector holds them.
pub(super) fn pose(a: [f64; 3], b: [f64; 3], s: f64) -> [f64; 3] {
    let l = [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s, a[2] + (b[2] - a[2]) * s];
    angles_to_axis(l).unwrap_or([0.0, 0.0, 1.0])
}

/// The angle between two unit axes, radians.
fn angle(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
    let t = d.acos();
    if t.is_finite() { t } else { std::f64::consts::PI }
}

fn lerp(a: Point3, b: Point3, s: f64) -> Point3 {
    Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s)
}

fn dist(a: Point3, b: Point3) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::MoveKind;

    /// The letters' lerp between the two poses, at `s` of the move.
    fn along(a: [f64; 3], b: [f64; 3], s: f64) -> [f64; 3] {
        pose(axis_to_angles(a), axis_to_angles(b), s)
    }

    /// Every pose INSIDE an interval - not only its ends - stays within
    /// `swing` of the midpoint pose, on a lean that turns A and B at
    /// once (the sphere path is no geodesic there, so the end poses
    /// alone would prove nothing).
    #[test]
    fn the_swing_bound_holds_between_the_samples() {
        let a = angles_to_axis([-70.0, 20.0, 0.0]).unwrap();
        let b = angles_to_axis([65.0, -80.0, 0.0]).unwrap();
        let m = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 0.0, 0.0), Some(100.0));
        let ivs = intervals(&m, a, b, 60.0, 0.01);
        assert!(ivs.len() > 100, "{} intervals", ivs.len());
        let n = ivs.len() as f64;
        for (j, iv) in ivs.iter().enumerate() {
            assert!(iv.swing.is_finite() && iv.swing >= angle(iv.am, iv.a0).max(angle(iv.am, iv.a1)));
            for k in 0..=16 {
                let s = (j as f64 + k as f64 / 16.0) / n;
                let inside = along(a, b, s);
                assert!(angle(iv.am, inside) <= iv.swing + 1e-12, "interval {j}, sample {k}");
            }
        }
    }

    /// A constant pose walks the move with no turn: one interval per
    /// STEP of tip travel, every pose the stated one, swing zero.
    #[test]
    fn a_constant_pose_walks_without_turning() {
        let a = angles_to_axis([30.0, 0.0, 0.0]).unwrap();
        let m = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 0.0, 0.0), Some(100.0));
        let ivs = intervals(&m, a, a, 100.0, 0.01);
        assert_eq!(ivs.len(), 20);
        assert!(ivs.iter().all(|iv| iv.swing == 0.0 && iv.am == a && iv.a0 == a && iv.a1 == a));
    }

    /// A move that is nowhere yields nothing: the caller fails closed.
    #[test]
    fn a_non_finite_move_yields_no_interval() {
        let a = [0.0, 0.0, 1.0];
        let m = Move::new(MoveKind::Feed, Point3::new(f64::NAN, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), None);
        assert!(intervals(&m, a, a, 10.0, 0.01).is_empty());
    }
}
