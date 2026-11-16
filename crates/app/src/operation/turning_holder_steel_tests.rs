//! Unit pins of `turning_holder_steel` (#1037): the holder-versus-steel law's exact band/box test, the holder's side (#1102), the margin, the -y side and the finding's move index.

use super::*;
use crate::cl::{Move, MoveKind};
use crate::operation::{Height, OpParams};

fn reach(x_lo: f64, x_hi: f64, y_hi: f64) -> Reach {
    Reach { x_lo, x_hi, y_lo: 0.0, y_hi }
}

#[test]
fn a_band_meets_a_box_exactly_along_a_straight_tip_motion() {
    // A band x 2..40, |y| <= 6, stations -8..10 about the tip.
    let r = reach(2.0, 40.0, 6.0);
    let jaw = Steel::Standing { s: (-40.0, -20.0), u: (14.0, 30.0), v: (-5.0, 5.0) };
    // The tip rides r 6.8 from station 20 to 2: the band's foot
    // reaches station -6, short of the jaw's -20.
    assert!(!meets((20.0, 6.8), (2.0, 6.8), -8.0, 10.0, &r, &jaw));
    // Down to station -10: the band spans -18..0, the jaw -40..-20:
    // still clear.
    assert!(!meets((20.0, 6.8), (-10.0, 6.8), -8.0, 10.0, &r, &jaw));
    // Down to -13: the foot reaches -21, inside the jaw.
    assert!(meets((20.0, 6.8), (-13.0, 6.8), -8.0, 10.0, &r, &jaw));
    // A jaw beside the plane, past the band's |y|, is clear.
    let aside = Steel::Standing { s: (-40.0, -20.0), u: (14.0, 30.0), v: (7.0, 9.0) };
    assert!(!meets((20.0, 6.8), (-13.0, 6.8), -8.0, 10.0, &r, &aside));
    // Revolved: the same jaw spinning reaches every side.
    let spun = Steel::Revolved { s: (-40.0, -20.0), r: (14.0, 34.0) };
    assert!(meets((20.0, 6.8), (-13.0, 6.8), -8.0, 10.0, &r, &spun));
    // A band wholly inside the envelope's bore never meets it.
    let inside = reach(-1.0, 1.0, 1.0);
    assert!(!meets((0.0, 6.8), (-30.0, 6.8), -1.0, 1.0, &inside, &spun));
    // ...and one wholly past its skin neither.
    let past = Steel::Revolved { s: (-40.0, -20.0), r: (0.0, 5.0) };
    let out = reach(2.0, 40.0, 0.0);
    assert!(!meets((0.0, 6.8), (-30.0, 6.8), -1.0, 1.0, &out, &past));
    // A tip crossing the axis sweeps the band through a revolved
    // bore from the other side.
    let ring = Steel::Revolved { s: (-1.0, 1.0), r: (20.0, 22.0) };
    assert!(meets((0.0, 30.0), (0.0, -30.0), -0.5, 0.5, &reach(-1.0, 1.0, 0.0), &ring));
}

/// A box over `[lo, hi]` in the turning tool frame, 12 facets.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<[[f64; 3]; 3]> {
    let c = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [[c(q[0]), c(q[1]), c(q[2])], [c(q[0]), c(q[2]), c(q[3])]]).collect()
}

/// A closed r `r` cylinder about Z over `z0..z1`.
fn cylinder(r: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    let v = |j: usize, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 32.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut f = Vec::new();
    for j in 0..32 {
        let (a, b, c, d) = (v(j, z0), v(j + 1, z0), v(j, z1), v(j + 1, z1));
        f.push(Triangle::new(a, b, d));
        f.push(Triangle::new(a, d, c));
        f.push(Triangle::new(Point3::new(0.0, 0.0, z0), b, a));
        f.push(Triangle::new(Point3::new(0.0, 0.0, z1), c, d));
    }
    f
}

fn mv(kind: MoveKind, from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind, from, to, feed: (kind != MoveKind::Rapid).then_some(100.0), arc: None, axis: None }
}

/// A rapid from world X `start` to `x` at Z 30, then feeds down world X
/// `x` through each Z of `zs` - beside the r 6 part.
fn ride(start: f64, x: f64, zs: &[f64]) -> Toolpath {
    let p = |x: f64, z: f64| Point3::new(x, 0.0, z);
    let mut moves = vec![mv(MoveKind::Rapid, p(start, 30.0), p(x, 30.0))];
    let mut at = 30.0;
    for &z in zs {
        moves.push(mv(MoveKind::Feed, p(x, at), p(x, z)));
        at = z;
    }
    Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None }
}

/// A box over x `x`, y `y`, z -4..4 (world).
fn jaw(x: [f64; 2], y: [f64; 2]) -> FixtureBox {
    FixtureBox { min: Point3::new(x[0], y[0], -4.0), max: Point3::new(x[1], y[1], 4.0) }
}

/// The holder's struck moves on `path` beside the r 6 part, for a body
/// x 3..12, |y| <= 3, z -4..4 and the steel `jaw` - spinning with the
/// part when `turns`, else standing - grown by `margin`.
fn struck(path: &Toolpath, jaw: FixtureBox, turns: bool, margin: f64) -> Result<Vec<usize>, String> {
    let body = LatheClearance::of(&cuboid([3.0, -3.0, -4.0], [12.0, 3.0, 4.0])).map_err(|e| e.to_string())?;
    let params = OpParams::ProfileFinish { tolerance: 0.05, clearance_z: Height::fixed(30.0), top_z: Height::fixed(16.0), bottom_z: Height::fixed(0.0), feed: 150.0, plunge_feed: 80.0 };
    let part = cylinder(6.0, -16.0, 16.0);
    let boxes = [jaw];
    let steel: (&[FixtureBox], &[FixtureBox]) = if turns { (&boxes, &[]) } else { (&[], &boxes) };
    let hits = steel_hits(&params, path, &part, &body, steel, margin)?;
    Ok(hits.iter().filter_map(|w| if let Warning::FixtureHit { at, fixture_part: FixturePart::Holder } = *w { Some(at) } else { None }).collect())
}

#[test]
fn a_rear_holder_past_centre_meets_standing_steel_like_its_front_mirror() {
    // Review of PR #1061: the front tool rides X 8, its body x 3..12
    // stands at X 11..20, into a standing jaw at x 12..20.
    let front_jaw = jaw([12.0, 20.0], [-5.0, 5.0]);
    let rear_jaw = jaw([-20.0, -12.0], [-5.0, 5.0]);
    assert_eq!(struck(&ride(8.0, 8.0, &[0.0]), front_jaw, false, 0.0), Ok(vec![1]));
    assert_eq!(struck(&ride(8.0, 8.0, &[0.0]), rear_jaw, false, 0.0), Ok(vec![]));
    // The mirror: a rear tool whose first point sits 0.5 past centre,
    // so every later tip is at X -8 of the path's frame. Read along
    // that frame's +x its body stood at X -5..4 and the jaw at -20..-12
    // passed; on the holder's own side it is struck, as the front one.
    assert_eq!(struck(&ride(0.5, -8.0, &[0.0]), rear_jaw, false, 0.0), Ok(vec![1]));
    assert_eq!(struck(&ride(0.5, -8.0, &[0.0]), front_jaw, false, 0.0), Ok(vec![]));
    // Spinning steel reads the same on either side.
    assert_eq!(struck(&ride(0.5, -8.0, &[0.0]), rear_jaw, true, 0.0), Ok(vec![1]));
    // A path on both sides of the axis cannot tell its holder's side:
    // refused by name, never read mirrored.
    let e = struck(&ride(-8.0, 8.0, &[0.0]), front_jaw, false, 0.0).err().unwrap_or_default();
    assert!(e.starts_with("the lathe holder body cannot be judged against the fixtures: the path stands on both sides"), "{e}");
}

#[test]
fn steel_grows_by_the_margin_standing_or_spinning() {
    // The body reaches X 20 (r 20.23 at its |y| 3 corner); a jaw from
    // x 20.5 is half a margin of 1 beyond it.
    let near = jaw([20.5, 28.0], [-5.0, 5.0]);
    for turns in [false, true] {
        assert_eq!(struck(&ride(8.0, 8.0, &[0.0]), near, turns, 0.0), Ok(vec![]), "turns {turns}");
        assert_eq!(struck(&ride(8.0, 8.0, &[0.0]), near, turns, 1.0), Ok(vec![1]), "turns {turns}");
    }
}

#[test]
fn standing_steel_on_the_minus_y_side_alone_is_struck() {
    // The band keeps |y| <= 3 only, so a jaw wholly at -y meets it...
    assert_eq!(struck(&ride(8.0, 8.0, &[0.0]), jaw([12.0, 20.0], [-5.0, -1.0]), false, 0.0), Ok(vec![1]));
    // ...and its +y mirror too, while one past |y| 3 is clear.
    assert_eq!(struck(&ride(8.0, 8.0, &[0.0]), jaw([12.0, 20.0], [1.0, 5.0]), false, 0.0), Ok(vec![1]));
    assert_eq!(struck(&ride(8.0, 8.0, &[0.0]), jaw([12.0, 20.0], [-5.0, -3.5]), false, 0.0), Ok(vec![]));
}

#[test]
fn the_finding_names_the_move_that_strikes() {
    // Rapid in (move 0), feed to Z 10 - the body's foot at 6, clear of
    // the jaw's top 4 (move 1) - then on to Z 0 (move 2): only move 2.
    let jaw = jaw([12.0, 20.0], [-5.0, 5.0]);
    assert_eq!(struck(&ride(8.0, 8.0, &[10.0, 0.0]), jaw, false, 0.0), Ok(vec![2]));
    assert_eq!(struck(&ride(0.5, -8.0, &[10.0, 0.0]), self::jaw([-20.0, -12.0], [-5.0, 5.0]), false, 0.0), Ok(vec![2]));
}

#[test]
fn the_gate_names_a_part_it_cannot_read_never_passing_it_clear() -> Result<(), String> {
    // Review of PR #1061: the post gate reads the op's part itself
    // (`lathe_steel_hits`); a part it cannot find leaves the holder
    // unjudged against the jaw - an `Err` by name, never an empty `Ok`.
    let world = crate::world::new_world();
    let body = LatheClearance::of(&cuboid([3.0, -3.0, -4.0], [12.0, 3.0, 4.0])).map_err(|e| e.to_string())?;
    let params = OpParams::ProfileFinish { tolerance: 0.05, clearance_z: Height::fixed(30.0), top_z: Height::fixed(16.0), bottom_z: Height::fixed(0.0), feed: 150.0, plunge_feed: 80.0 };
    let op = crate::schema::Operation {
        tool: "TN-1600-0R8".into(),
        src: crate::operation::SrcRef::Entity("ghost".into()),
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let boxes = [jaw([12.0, 20.0], [-5.0, 5.0])];
    let e = lathe_steel_hits(&world, &op, &ride(8.0, 8.0, &[0.0]), &body, (&boxes, &[])).err().unwrap_or_default();
    assert!(e.starts_with("the lathe holder body cannot be judged against the fixtures: ") && e.contains("'ghost'"), "{e}");
    // No steel, nothing to judge: the part is never read.
    assert_eq!(lathe_steel_hits(&world, &op, &ride(8.0, 8.0, &[0.0]), &body, (&[], &[])), Ok(vec![]));
    Ok(())
}
