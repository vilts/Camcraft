//! The fixture laws (c2)/(d2) on a spindle-C turning move (issue
//! #486): the part turns under a fixed radial live tool, so the
//! machine's tool stands on the +X station of the spindle line at
//! every instant while the part-frame path swings round it. Steel
//! declared FIXED (a tailstock, a steady, a fixed clamp) stays in the
//! machine frame and meets the station tool wherever it stands; steel
//! RIDING the part (a chuck jaw) meets the radial tool at every pose
//! the turn passes through, not only the pose the move states at its
//! end (`cl::stamp_c` cuts one move into many pieces).
//!
//! Every hit asserted here is first shown by hand: the stack placed
//! where the machine places it at mid-turn, inside the box. Clear
//! cases are the same moves with the steel moved off that place.
//!
//! Gate map (M4.4): covers camcraft_sim::verify::workholding,
//! camcraft_sim::verify::c_steel, camcraft_sim::cl (stamp_c).
//! Lane shape (M4.4): all tests here run in well under a second.
//! Provenance: our own fixtures; the law is the Prime Directive's, the
//! C convention `cl::rotary`'s.

use camcraft_sim::cl::{stamp_c, Move, MoveKind, Toolpath};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{
    bands_vs_workholding, rapids_vs_workholding, FixturePart, RotaryAxis, ToolGeom, Warning, Workholding,
};

fn tool() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 15.0,
        shank_radius: 3.0,
        shank_len: 20.0,
        holder_radius: 10.0,
        holder_len: 30.0,
    }
}

/// The spindle line: world Z through the origin.
fn spindle() -> RotaryAxis {
    RotaryAxis { origin: Point3::new(0.0, 0.0, 0.0), dir: [0.0, 0.0, 1.0] }
}

/// The tip at azimuth `deg` on radius `rho`, height `z`, and its
/// radial pose.
fn at(rho: f64, deg: f64, z: f64) -> (Point3, [f64; 3]) {
    let (s, c) = deg.to_radians().sin_cos();
    (Point3::new(rho * c, rho * s, z), [c, s, 0.0])
}

/// One live-tool turn from radius 10 at azimuth 0 to radius 10 at
/// azimuth 90 (C 0 to -90), z from `z0` to `z1`, stamped to 0.01 mm.
/// The verified path is the part-frame chord - its midpoint at (5, 5),
/// radius 7.07, C -45 - and the stamp splits it until the machine's
/// C-linear motion follows that chord.
fn quarter_turn(kind: MoveKind, z0: f64, z1: f64) -> Toolpath {
    let (a, _) = at(10.0, 0.0, z0);
    let (b, pb) = at(10.0, 90.0, z1);
    let raw = Toolpath::new(vec![Move::new(kind, a, b, Some(400.0)).with_axis(pb)]);
    stamp_c(&raw, [0.0, 0.0], 0.01).unwrap_or_else(|e| panic!("the turn stamps: {e}"))
}

/// Is `p` inside the box?
fn inside(p: Point3, b: &FixtureBox) -> bool {
    (b.min.x..=b.max.x).contains(&p.x) && (b.min.y..=b.max.y).contains(&p.y) && (b.min.z..=b.max.z).contains(&p.z)
}

/// Fixed steel on the station side of the line at z -6..-4, x 24..28:
/// where the machine's shank (x 22..42 along +X) passes when the tip,
/// descending from z 0 to -10, is near z -5 - mid-turn, C near -45.
/// Short of the holder (x 42 on, radius 10), and below the stack at
/// the turn's start (tip z 0, shank down to z -3).
fn station_block() -> FixtureBox {
    FixtureBox { min: Point3::new(24.0, -3.0, -6.0), max: Point3::new(28.0, 3.0, -4.0) }
}

/// The stamped pieces of the turn, judged against `fixed` steel.
fn fixed_hits(path: &Toolpath, block: FixtureBox, rapids: bool, axis: Option<RotaryAxis>) -> Vec<Warning> {
    let b = [block];
    let wh = Workholding { turning: &[], fixed: &b, axis };
    if rapids {
        rapids_vs_workholding(path, &tool(), &wh, None)
    } else {
        bands_vs_workholding(path, &tool(), &wh, None, 0.01)
    }
}

#[test]
fn the_machine_meets_fixed_steel_at_the_station_mid_turn() {
    // By hand: at mid-turn the tip (radius 7.07) stands at (7.07, 0, -5)
    // on the machine and the shank's axis runs x 22.07..42.07 along +X
    // at y 0 - its point 20 mm up the tool is inside the block.
    let shank_mid = Point3::new(50f64.sqrt() + 20.0, 0.0, -5.0);
    assert!(inside(shank_mid, &station_block()));
    let path = quarter_turn(MoveKind::Feed, 0.0, -10.0);
    assert!(path.moves.len() > 4, "the turn is cut into pieces: {}", path.moves.len());
    for axis in [Some(spindle()), None] {
        let w = fixed_hits(&path, station_block(), false, axis);
        assert!(!w.is_empty(), "fixed steel at the station must convict the turn (axis {axis:?})");
        // The piece the machine strikes the block on - the one passing
        // z -5, mid-turn - is convicted. (Pieces near the turn's start
        // may report a graze too: the stack there stands 0.06 mm over
        // the block's top, inside the enclosures' growth.)
        let mid = path.moves.iter().position(|m| m.from.z >= -5.0 && m.to.z <= -5.0);
        assert!(
            mid.is_some_and(|k| w.iter().any(|x| matches!(x, Warning::FixtureHit { at, .. } if *at == k))),
            "the mid-turn piece {mid:?} must convict (axis {axis:?}): {w:?}"
        );
    }
}

#[test]
fn a_rapid_turn_past_fixed_steel_convicts_as_a_rapid() {
    let path = quarter_turn(MoveKind::Rapid, 0.0, -10.0);
    let w = fixed_hits(&path, station_block(), true, Some(spindle()));
    let mid = path.moves.iter().position(|m| m.from.z >= -5.0 && m.to.z <= -5.0);
    assert!(
        mid.is_some_and(|k| w.iter().any(|x| matches!(x, Warning::FixtureHit { at, fixture_part: FixturePart::Rapid } if *at == k))),
        "the mid-turn rapid piece {mid:?} must convict as a rapid: {w:?}"
    );
}

#[test]
fn fixed_steel_off_the_station_stays_clear() {
    // The same block on the far side of the line (-X), and the block
    // at the station but below the turn's deepest stack (z -30..-20):
    // the machine never meets either.
    let path = quarter_turn(MoveKind::Feed, 0.0, -10.0);
    let far = FixtureBox { min: Point3::new(-28.0, -3.0, -6.0), max: Point3::new(-24.0, 3.0, -4.0) };
    let low = FixtureBox { min: Point3::new(24.0, -3.0, -30.0), max: Point3::new(28.0, 3.0, -20.0) };
    for b in [far, low] {
        let w = fixed_hits(&path, b, false, Some(spindle()));
        assert!(w.is_empty(), "{b:?} is nowhere the machine's tool goes: {w:?}");
        let r = fixed_hits(&quarter_turn(MoveKind::Rapid, 0.0, -10.0), b, true, Some(spindle()));
        assert!(r.is_empty(), "{b:?}: {r:?}");
    }
}

/// A chuck jaw riding the part on the 45 deg diagonal, radius 60, at
/// the turn's height: where the radial holder (radius 42..72 from the
/// line) stands at mid-turn, and nowhere the stack stands at the pose
/// the move states at its end (+Y).
fn diagonal_jaw() -> FixtureBox {
    let c = 60.0 * std::f64::consts::FRAC_1_SQRT_2;
    FixtureBox { min: Point3::new(c - 1.5, c - 1.5, -1.5), max: Point3::new(c + 1.5, c + 1.5, 1.5) }
}

#[test]
fn steel_riding_the_part_meets_the_pose_the_turn_passes_through() {
    // By hand: at mid-turn (C -45) the tip is at (5, 5), radius 7.07,
    // and the holder runs radius 42.07..72.07 along the diagonal - the
    // jaw's centre (radius 60) is on its axis.
    let (tip, pose) = at(50f64.sqrt(), 45.0, 0.0);
    let on_holder = Point3::new(tip.x + (60.0 - tip.x.hypot(tip.y)) * pose[0], tip.y + (60.0 - tip.x.hypot(tip.y)) * pose[1], 0.0);
    assert!(inside(on_holder, &diagonal_jaw()));
    let path = quarter_turn(MoveKind::Feed, 0.0, 0.0);
    let j = [diagonal_jaw()];
    for axis in [Some(spindle()), None] {
        let wh = Workholding { turning: &j, fixed: &[], axis };
        let w = bands_vs_workholding(&path, &tool(), &wh, None, 0.01);
        assert!(!w.is_empty(), "the jaw on the diagonal must convict the turn (axis {axis:?})");
    }
}

#[test]
fn steel_riding_the_part_off_the_turns_reach_stays_clear() {
    // The jaw moved out to radius 120 on the diagonal: past the whole
    // stack (tip r10 + 65), at every pose of the turn.
    let c = 120.0 * std::f64::consts::FRAC_1_SQRT_2;
    let far = [FixtureBox { min: Point3::new(c - 1.5, c - 1.5, -1.5), max: Point3::new(c + 1.5, c + 1.5, 1.5) }];
    let path = quarter_turn(MoveKind::Feed, 0.0, 0.0);
    let wh = Workholding { turning: &far, fixed: &[], axis: Some(spindle()) };
    let w = bands_vs_workholding(&path, &tool(), &wh, None, 0.01);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn every_stamped_piece_states_the_station_pose_at_its_own_c() {
    let path = quarter_turn(MoveKind::Feed, 0.0, -10.0);
    for (i, m) in path.moves.iter().enumerate() {
        let (Some(c), Some(a)) = (m.rotary, m.axis) else { panic!("piece {i} states C and a pose") };
        // Rz(C) of the pose is the station's +X.
        let (s, co) = c.to_radians().sin_cos();
        let (x, y) = (a[0] * co - a[1] * s, a[0] * s + a[1] * co);
        assert!((x - 1.0).abs() < 1e-9 && y.abs() < 1e-9 && a[2].abs() < 1e-12, "piece {i} at C {c}: {a:?}");
    }
}

#[test]
fn a_path_stamped_before_486_is_read_at_the_pose_its_c_states() {
    // Before #486 every piece stated the whole move's axis (+Y here):
    // the laws read the C, not that axis, so a cached path convicts too.
    let mut path = quarter_turn(MoveKind::Feed, 0.0, 0.0);
    let end = path.moves.last().and_then(|m| m.axis);
    for m in &mut path.moves {
        m.axis = end;
    }
    let j = [diagonal_jaw()];
    let wh = Workholding { turning: &j, fixed: &[], axis: Some(spindle()) };
    assert!(!bands_vs_workholding(&path, &tool(), &wh, None, 0.01).is_empty());
}

#[test]
fn fixed_steel_under_a_c_no_line_places_fails_closed() {
    // Every C word off by 30 deg from its tip: no line stands every tip
    // on its ray, so the machine frame is unknown - each C move against
    // fixed steel reports, even steel far from anything.
    let mut path = quarter_turn(MoveKind::Feed, 0.0, 0.0);
    for (k, m) in path.moves.iter_mut().enumerate() {
        m.rotary = m.rotary.map(|c| c + if k % 2 == 0 { 30.0 } else { -30.0 });
    }
    let far = FixtureBox { min: Point3::new(500.0, 500.0, 500.0), max: Point3::new(510.0, 510.0, 510.0) };
    let w = fixed_hits(&path, far, false, Some(spindle()));
    assert_eq!(w.len(), path.moves.len(), "{w:?}");
    // The same steel riding the part needs no line: clear.
    let f = [far];
    let wh = Workholding { turning: &f, fixed: &[], axis: Some(spindle()) };
    assert!(bands_vs_workholding(&path, &tool(), &wh, None, 0.01).is_empty());
}

#[test]
fn an_arc_stamped_turn_convicts_fixed_station_steel_mid_turn() {
    use camcraft_sim::geo2d::Point2;
    let (a, _) = at(10.0, 0.0, 0.0);
    let (b, pb) = at(10.0, 90.0, -10.0);
    let arc_mv = Move::new(MoveKind::Feed, a, b, Some(400.0))
        .with_arc(Point2::new(0.0, 0.0), true)
        .with_axis(pb);
    let path = stamp_c(&Toolpath::new(vec![arc_mv]), [0.0, 0.0], 0.01).expect("arc stamps");
    assert!(path.moves.iter().all(|m| m.arc.is_none()));
    // Mid-arc the tip stands on r 10, so the shank at +20 reaches x 30 (inside 29..32).
    let block = FixtureBox { min: Point3::new(29.0, -2.0, -6.0), max: Point3::new(32.0, 2.0, -4.0) };
    assert!(!fixed_hits(&path, block, false, Some(spindle())).is_empty());
    let clear = FixtureBox { min: Point3::new(120.0, -2.0, -6.0), max: Point3::new(124.0, 2.0, -4.0) };
    assert!(fixed_hits(&path, clear, false, Some(spindle())).is_empty());
}
