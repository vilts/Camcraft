//! Unit pins of `turning_holder_side` (#1102): the holder law reads a lathe body on the holder's own side of the spindle axis, for a tip at either signed X of the path's meridian.

use super::*;
use crate::cl::{Move, MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::operation::drill_prior::PriorCell;
use crate::operation::turning_rapids::Meridian;
use crate::operation::{Height, OpParams};
use crate::verify::Warning;
use camcraft_tools::lathe_holder::LatheClearance;

#[test]
fn the_side_is_where_the_path_reaches_and_a_past_centre_overrun_keeps_it() {
    // Every turning generator emits its tips on the frame's own half
    // plane (rho >= 0), reaching the axis at most (a face to X 0): +1.
    assert_eq!(side_of([30.0, 12.0, 0.0]), Ok(1.0));
    assert_eq!(side_of([]), Ok(1.0));
    assert_eq!(side_of([0.0, -0.0]), Ok(1.0));
    // A face run 0.4 past centre keeps the holder on its side.
    assert_eq!(side_of([20.0, -0.4]), Ok(1.0));
    // A rear tool whose first point sits a hair past centre: every
    // later tip is at X < 0 of the path's frame - the holder is on -.
    assert_eq!(side_of([0.5, -8.0, -30.0]), Ok(-1.0));
    // Both sides beyond an overrun, or a path near the axis on both
    // sides: the side cannot be told - refused by name, never guessed.
    for xs in [[30.0, -30.0], [20.0, -3.0], [2.0, -2.0], [1.0, -0.5]] {
        let e = side_of(xs).err().unwrap_or_default();
        assert!(e.contains("both sides of the spindle axis"), "{xs:?}: {e}");
    }
    // A near side within the overrun while the far side reaches less
    // than twice it (a small part, review of PR #1167): the near side
    // may be the holder's own - refused, never read mirrored.
    let e = side_of([2.0, -3.0]).err().unwrap_or_default();
    assert!(e.contains("both sides of the spindle axis"), "{e}");
    assert_eq!(side_of([6.0, -0.4]), Ok(1.0));
    assert!(side_of([f64::NAN]).is_err());
}

fn cylinder(r: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 32.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut f = Vec::new();
    for j in 0..32 {
        let (a, b, c, d) = (v(j, r, z0), v(j + 1, r, z0), v(j, r, z1), v(j + 1, r, z1));
        f.push(Triangle::new(a, b, d));
        f.push(Triangle::new(a, d, c));
        f.push(Triangle::new(Point3::new(0.0, 0.0, z0), b, a));
        f.push(Triangle::new(Point3::new(0.0, 0.0, z1), c, d));
    }
    f
}

/// A box over `[lo, hi]` in the turning tool frame, 12 facets.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<[[f64; 3]; 3]> {
    let c = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [[c(q[0]), c(q[1]), c(q[2])], [c(q[0]), c(q[2]), c(q[3])]]).collect()
}

/// The holder hits (move, against the part) of `body` on a path whose
/// first point stands at world X `start`, then rides world X `x` down
/// from Z 30 to Z 0 beside a Ø12 part (Z -16..16, read as the stock).
fn hits(body_x: [f64; 2], start: f64, x: f64) -> Result<Vec<(usize, bool)>, String> {
    let p = Point3::new;
    let mv = |kind, from, to| Move { rotary: None, kind, from, to, feed: (kind != MoveKind::Rapid).then_some(100.0), arc: None, axis: None };
    hits_on(body_x, vec![mv(MoveKind::Rapid, p(start, 0.0, 30.0), p(x, 0.0, 30.0)), mv(MoveKind::Feed, p(x, 0.0, 30.0), p(x, 0.0, 0.0))])
}

fn path_of(moves: Vec<Move>) -> Toolpath {
    Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None }
}

/// [`hits`] on any path `moves` beside the Ø12 part.
fn hits_on(body_x: [f64; 2], moves: Vec<Move>) -> Result<Vec<(usize, bool)>, String> {
    let body = LatheClearance::of(&cuboid([body_x[0], -3.0, -4.0], [body_x[1], 3.0, 4.0])).map_err(|e| e.to_string())?;
    let tool = crate::tools::catalog().into_iter().find(|(n, _)| *n == "TN-1600-0R8").map(|(_, t)| t).ok_or("catalog TN-1600-0R8")?;
    let params = OpParams::ProfileFinish { tolerance: 0.05, clearance_z: Height::fixed(30.0), top_z: Height::fixed(16.0), bottom_z: Height::fixed(0.0), feed: 150.0, plunge_feed: 80.0 };
    let path = path_of(moves);
    let part = cylinder(6.0, -16.0, 16.0);
    let cx = Meridian { part: &part, stock: None, nose: 0.8, margin: 0.01, z_tol: 1e-6, drills: &[], band: 0.0, body: None, ahead: &[], outward: None };
    let mut warns = Vec::new();
    crate::operation::turning_holder::judge(&params, &path, &cx, Some(&body), &tool, &mut PriorCell::new(None), &mut warns)?;
    Ok(warns.iter().filter_map(|w| if let Warning::HolderHit { at, part, .. } = w { Some((*at, *part)) } else { None }).collect())
}

#[test]
fn a_rear_tool_past_centre_is_read_on_its_own_side_like_its_front_mirror() {
    // The tip rides r 8 beside the r 6 part (nose 0.8 clear). A body
    // lip x -12..-3 reaches from the tip toward the axis: on the
    // holder's side it stands at r -4..5 - through the part.
    let reaching = [-12.0, -3.0];
    // A body x 3..12 stands outward, r 11..20: clear of the part.
    let clear = [3.0, 12.0];
    // Front tool, every tip at X >= 0 (the law as before #1102).
    assert_eq!(hits(reaching, 8.0, 8.0), Ok(vec![(1, true)]));
    assert_eq!(hits(clear, 8.0, 8.0), Ok(vec![]));
    // Rear tool starting 0.5 past centre: the frame's + side is the
    // start's, so every later tip is at X -8. Read along the frame's +x
    // the reaching lip would sit at r 11..20 (a missed strike) and the
    // clear body would cross the axis (a false one); read on the
    // holder's side, both verdicts are the front tool's.
    assert_eq!(hits(reaching, 0.5, -8.0), Ok(vec![(1, true)]));
    assert_eq!(hits(clear, 0.5, -8.0), Ok(vec![]));
    // A face run 0.4 past centre from the front keeps the front side.
    assert_eq!(hits(clear, -0.4, 8.0), Ok(vec![]));
    // A path on both sides of the axis cannot tell its holder's side.
    let both = hits(clear, -8.0, 8.0).err().unwrap_or_default();
    assert!(both.starts_with("the lathe holder body cannot be judged: the path stands on both sides"), "{both}");
}

/// A thread pass the way the thread kernel winds it: rapid in at X
/// `out`, feed to `t`, six ccw half-turn XY arcs about the axis whose
/// ends alternate between `t` and its antipode, falling 0.75 each, then
/// a rapid out. `start` is the first point's X.
fn thread_pass(start: f64, out: f64, t: f64) -> Vec<Move> {
    use crate::cl::{ArcMove, ArcPlane};
    use crate::geo2d::Point2;
    let p = Point3::new;
    let mv = |kind, from, to| Move { rotary: None, kind, from, to, feed: (kind != MoveKind::Rapid).then_some(100.0), arc: None, axis: None };
    let mut moves = vec![mv(MoveKind::Rapid, p(start, 0.0, 30.0), p(out, 0.0, 30.0)), mv(MoveKind::Rapid, p(out, 0.0, 30.0), p(out, 0.0, 10.0)), mv(MoveKind::Feed, p(out, 0.0, 10.0), p(t, 0.0, 10.0))];
    let mut at = p(t, 0.0, 10.0);
    for i in 1..=6 {
        let to = p(if i % 2 == 0 { t } else { -t }, 0.0, 10.0 - 0.75 * i as f64);
        let mut m = mv(MoveKind::Feed, at, to);
        m.arc = Some(ArcMove { center: Point2::new(0.0, 0.0), ccw: true, plane: ArcPlane::Xy });
        moves.push(m);
        at = to;
    }
    moves.push(mv(MoveKind::Rapid, at, p(out, 0.0, at.z)));
    moves
}

#[test]
fn a_thread_pass_winding_through_the_antipode_is_judged_not_refused() -> Result<(), String> {
    // Review of PR #1167: the thread kernel's half-turn arcs end on the
    // antipode, so a raw from/to reading put the path at +5.5 and -5.5
    // and refused every threading op with a lathe body. A round arc about
    // the axis tells no side; the approach does.
    let (reaching, clear) = ([-12.0, -3.0], [3.0, 12.0]);
    assert_eq!(hits_on(clear, thread_pass(8.0, 8.0, 5.5)), Ok(vec![]));
    let front = hits_on(reaching, thread_pass(8.0, 8.0, 5.5))?;
    assert!(!front.is_empty() && front.iter().all(|&(_, part)| part), "{front:?}");
    // The rear mirror (first point 0.5 past centre, every other tip at
    // X < 0): the arcs are read on the holder's side, same verdicts.
    assert_eq!(hits_on(clear, thread_pass(0.5, -8.0, -5.5)), Ok(vec![]));
    let rear = hits_on(reaching, thread_pass(0.5, -8.0, -5.5))?;
    assert_eq!(rear, front);
    Ok(())
}

#[test]
fn the_drawn_body_flips_its_radial_with_the_holder_side() -> Result<(), String> {
    // body_axes and judge share one side: a rear path draws the body
    // along the negated radial, a front one along the frame's own.
    let p = Point3::new;
    let mv = |from, to| Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(100.0), arc: None, axis: None };
    let part = cylinder(6.0, -16.0, 16.0);
    let axes = |start: f64, x: f64| crate::operation::turning_holder::body_axes(&part, &path_of(vec![mv(p(start, 0.0, 30.0), p(x, 0.0, 30.0)), mv(p(x, 0.0, 30.0), p(x, 0.0, 0.0))]));
    let (front, along) = axes(8.0, 8.0)?;
    let (rear, rear_along) = axes(0.5, -8.0)?;
    assert!(front[0].abs() > 0.99, "{front:?}");
    for k in 0..3 {
        assert!((rear[k] + front[k]).abs() < 1e-9, "{rear:?} vs {front:?}");
        assert!((rear_along[k] - along[k]).abs() < 1e-9);
    }
    // A thread pass draws on its approach's side too, not refused.
    assert_eq!(crate::operation::turning_holder::body_axes(&part, &path_of(thread_pass(8.0, 8.0, 5.5))).map(|a| a.0), Ok(front));
    Ok(())
}
