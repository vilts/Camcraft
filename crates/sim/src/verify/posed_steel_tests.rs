//! #1305: the posed sweep reads a band as the flat-ended cylinder it
//! is. A capsule's caps reach a whole radius past each band end, so a
//! holder standing 0.5 mm over a clamp was convicted at any lean; the
//! rim of the true band dips only R sin θ. Pinned both ways, with the
//! property that keeps the law safe: steel standing at ANY point of
//! the true leaning band, at any instant of the move, is struck.
//! Provenance: our own fixtures; the law is the Prime Directive's.

use super::*;
use crate::cl::{angles_to_axis, axis_to_angles, Move};

/// A r 15 holder whose bottom stands 30 mm over the tip (the #938
/// fixture): flute 0..10 and shank 10..30 at r 3, holder 30..60.
fn wide_holder() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 10.0,
        shank_radius: 3.0,
        shank_len: 20.0,
        holder_radius: 15.0,
        holder_len: 30.0,
    }
}

/// A unit axis leaning `deg` off +Z toward +X.
fn lean(deg: f64) -> [f64; 3] {
    let r = deg.to_radians();
    [r.sin(), 0.0, r.cos()]
}

fn feed(from: Point3, to: Point3, axis: [f64; 3]) -> Move {
    Move::new(MoveKind::Feed, from, to, Some(500.0)).with_axis(axis)
}

/// The issue's evidence case: a 2 mm feed at the origin, the clamp's
/// top 0.5 mm under the upright holder's bottom, inside its rim. The
/// clamp's near side `x0` is 6 in the issue; from 8 the leaning SHANK
/// (r 3, its top 30 sin θ + 2 mm of feed out) stays off it to 5 deg,
/// so the holder's bottom face alone decides.
fn over_the_clamp(deg: f64, x0: f64) -> Option<FixturePart> {
    let clamp = [FixtureBox { min: Point3::new(x0, -8.0, -10.0), max: Point3::new(14.0, 8.0, 29.5) }];
    let path = Toolpath::new(vec![feed(Point3::new(0.0, 0.0, 0.0), Point3::new(2.0, 0.0, 0.0), lean(deg))]);
    let tool = wide_holder();
    posed_hits(&path, &tool, &HolderStack::cylinder(&tool), &clamp, None, 0.01, false)[0]
}

/// The lean at which the holder's bottom face first touches the clamp's
/// far top edge (x 14, z 29.5), the tip at the feed's start: the face
/// through (30 sin θ, 30 cos θ) with normal the pose, 30 - 14 sin θ =
/// 29.5 cos θ, solved by bisection.
fn touching_lean_deg() -> f64 {
    let gap = |deg: f64| 30.0 - 14.0 * deg.to_radians().sin() - 29.5 * deg.to_radians().cos();
    let (mut a, mut b) = (0.0f64, 10.0f64);
    for _ in 0..60 {
        let m = 0.5 * (a + b);
        if gap(m) > 0.0 { a = m } else { b = m }
    }
    0.5 * (a + b)
}

#[test]
fn a_holder_just_over_a_clamp_is_clear_at_a_small_real_lean() {
    for deg in [1e-7, 0.01, 0.1, 0.5, 1.0, 1.5] {
        assert_eq!(over_the_clamp(deg, 6.0), None, "the issue's clamp, lean {deg} deg");
    }
    for deg in [0.01, 1.0, 2.0, 2.1] {
        assert_eq!(over_the_clamp(deg, 8.0), None, "lean {deg} deg: the rim dips {} mm of 0.5", 15.0 * deg.to_radians().sin());
    }
}

#[test]
fn the_holder_is_struck_once_its_rim_really_reaches_the_clamp() {
    for deg in [2.2, 3.0, 5.0] {
        assert_eq!(over_the_clamp(deg, 8.0), Some(FixturePart::Holder), "lean {deg} deg");
    }
    // The issue's clamp: the holder's rim, and past 2 deg the shank too.
    for deg in [2.2, 10.0, 30.0] {
        assert!(over_the_clamp(deg, 6.0).is_some(), "lean {deg} deg");
    }
}

/// Continuity: across a sweep of leans the verdict turns exactly once,
/// at the lean where the true band touches - no cliff at a tolerance.
#[test]
fn the_verdict_turns_once_and_at_the_true_touching_lean() {
    let touch = touching_lean_deg();
    assert!((2.0..2.3).contains(&touch), "{touch}");
    let mut first = None;
    for k in 1..=1000 {
        let deg = 0.005 * k as f64;
        let hit = over_the_clamp(deg, 8.0).is_some();
        if hit && first.is_none() {
            first = Some(deg);
        }
        assert_eq!(hit, first.is_some(), "the verdict flipped back at {deg} deg (first hit {first:?})");
    }
    let first = first.unwrap_or(f64::NAN);
    assert!((first - touch).abs() <= 0.006, "first hit at {first} deg, the band touches at {touch} deg");
}

/// Under a rotary the machine stands the leaning stack upright: a FIXED
/// clamp under the holder is then under a flat bottom at every lean.
#[test]
fn a_fixed_clamp_under_the_stood_up_holder_is_clear_at_any_lean() {
    let ax = RotaryAxis { origin: Point3::new(0.0, 0.0, 0.0), dir: [1.0, 0.0, 0.0] };
    let tool = wide_holder();
    let read = |deg: f64, top: f64| {
        let r = deg.to_radians();
        let path = Toolpath::new(vec![feed(Point3::new(0.0, 0.0, 0.0), Point3::new(2.0, 0.0, 0.0), [0.0, r.sin(), r.cos()])]);
        let clamp = [FixtureBox { min: Point3::new(-4.0, 6.0, -10.0), max: Point3::new(4.0, 14.0, top) }];
        sweep(&path, &tool, &HolderStack::cylinder(&tool), &clamp, None, 0.01, false, Some(&ax))[0]
    };
    for deg in [0.01, 1.0, 20.0, -35.0] {
        assert_eq!(read(deg, 29.5), Verdict::Clear, "lean {deg} deg");
        assert_eq!(read(deg, 30.5), Verdict::Hit(FixturePart::Holder), "lean {deg} deg");
    }
}

/// A tiny deterministic generator (no dependency): uniform in 0..1.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn span(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

/// A point of the TRUE band at `s` of the move: the tip on its line,
/// the pose the letters hold there, `h` up the pose and `rho` off it.
fn band_point(m: &Move, a: [f64; 3], b: [f64; 3], s: f64, h: f64, rho: f64, phi: f64) -> (Point3, [f64; 3]) {
    let (la, lb) = (axis_to_angles(a), axis_to_angles(b));
    let l = [0, 1, 2].map(|k| la[k] + (lb[k] - la[k]) * s);
    let p = angles_to_axis(l).unwrap_or([0.0, 0.0, 1.0]);
    let seed = if p[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let e1 = unit3(cross3(p, seed)).unwrap_or([0.0, 1.0, 0.0]);
    let e2 = cross3(p, e1);
    let tip = Point3::new(
        m.from.x + (m.to.x - m.from.x) * s,
        m.from.y + (m.to.y - m.from.y) * s,
        m.from.z + (m.to.z - m.from.z) * s,
    );
    let (c, sn) = (rho * phi.cos(), rho * phi.sin());
    let off = [0, 1, 2].map(|k| p[k] * h + e1[k] * c + e2[k] * sn);
    (Point3::new(tip.x + off[0], tip.y + off[1], tip.z + off[2]), p)
}

fn speck(p: Point3) -> [FixtureBox; 1] {
    let e = 1e-6;
    [FixtureBox { min: Point3::new(p.x - e, p.y - e, p.z - e), max: Point3::new(p.x + e, p.y + e, p.z + e) }]
}

/// The safety property: the flat-ended reading still ENCLOSES the swept
/// band. A speck of steel at any sampled point of the true band - rims
/// and end faces most of all - at any instant of a move whose pose
/// swings, is struck; in the path frame and under a rotary.
#[test]
fn steel_at_any_point_of_the_true_leaning_band_is_struck() {
    let tool = wide_holder();
    let stack = HolderStack::cylinder(&tool);
    let bands = [(0.0, 10.0, 3.0), (10.0, 30.0, 3.0), (30.0, 60.0, 15.0)];
    let ax = RotaryAxis { origin: Point3::new(3.0, -2.0, -40.0), dir: [1.0, 0.0, 0.0] };
    let mut g = Lcg(1305);
    for case in 0..400 {
        let framed = case % 2 == 1;
        // Poses up to 60 deg off the upright; under the rotary, in its plane.
        let pose = |g: &mut Lcg| {
            let (t, az) = (g.span(0.02, 1.05), if framed { std::f64::consts::FRAC_PI_2 } else { g.span(0.0, std::f64::consts::TAU) });
            [t.sin() * az.cos(), t.sin() * az.sin(), t.cos()]
        };
        let (a, b) = (pose(&mut g), if case % 4 < 2 { pose(&mut g) } else { [0.0, 0.0, 1.0] });
        let from = Point3::new(g.span(-5.0, 5.0), g.span(-5.0, 5.0), g.span(-5.0, 5.0));
        let to = Point3::new(from.x + g.span(-6.0, 6.0), from.y + g.span(-6.0, 6.0), from.z + g.span(-6.0, 6.0));
        let m = feed(from, to, b);
        let path = Toolpath::new(vec![feed(from, from, a), m]);
        for _ in 0..12 {
            let (lo, hi, r) = bands[(g.next() * 3.0) as usize % 3];
            // Half the samples on a rim or an end face, the rest inside.
            let edge = g.next() < 0.5;
            let h = if edge { if g.next() < 0.5 { lo } else { hi } } else { g.span(lo, hi) };
            let rho = if edge && g.next() < 0.7 { r } else { g.span(0.0, r) };
            let (pt, p) = band_point(&m, a, b, g.next(), h, rho, g.span(0.0, std::f64::consts::TAU));
            if framed {
                let th = upright_turn(&ax, p).unwrap_or_else(|| panic!("case {case}: pose {p:?} left the rotary's plane"));
                let at = turn_about(&ax, pt, th);
                let v = sweep(&path, &tool, &stack, &speck(at), None, 0.01, false, Some(&ax))[1];
                assert!(matches!(v, Verdict::Hit(_)), "case {case}: machine-frame steel at {at:?} read {v:?}");
            } else {
                let v = posed_hits(&path, &tool, &stack, &speck(pt), None, 0.01, false)[1];
                assert!(v.is_some(), "case {case}: steel at the band point {pt:?} passed clear");
            }
        }
    }
}

/// Non-finite numbers are never a proof of clearance.
#[test]
fn a_non_finite_band_or_box_is_never_proven_clear() {
    let iv = midpose::Interval {
        p0: Point3::new(0.0, 0.0, 0.0),
        p1: Point3::new(1.0, 0.0, 0.0),
        pm: Point3::new(0.5, 0.0, 0.0),
        am: lean(5.0),
        a0: lean(5.0),
        a1: lean(5.0),
        swing: 0.0,
    };
    let far = FixtureBox { min: Point3::new(500.0, 500.0, 500.0), max: Point3::new(510.0, 510.0, 510.0) };
    assert!(flat_band_clear(&iv, 0.0, (30.0, 60.0, 15.0), None, &far));
    assert!(!flat_band_clear(&iv, f64::NAN, (30.0, 60.0, 15.0), None, &far));
    assert!(!flat_band_clear(&iv, 0.0, (30.0, f64::INFINITY, 15.0), None, &far));
    assert!(!flat_band_clear(&iv, 0.0, (30.0, 60.0, f64::NAN), None, &far));
    assert!(!flat_band_clear(&midpose::Interval { swing: f64::NAN, ..iv }, 0.0, (30.0, 60.0, 15.0), None, &far));
    let nowhere = FixtureBox { min: Point3::new(f64::NAN, 0.0, 0.0), max: far.max };
    assert!(!flat_band_clear(&iv, 0.0, (30.0, 60.0, 15.0), None, &nowhere));
    let bad = RotaryAxis { origin: Point3::new(0.0, 0.0, 0.0), dir: [0.0, 0.0, 0.0] };
    assert!(!flat_band_clear(&iv, 0.0, (30.0, 60.0, 15.0), Some((&bad, 0.0, 0.0)), &far));
}
