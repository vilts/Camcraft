//! Issue #130 kernel laws: the turning kinds read the revolved stock
//! envelope. In a bar wider than the part every rapid clears the bar's
//! OD plus the clearance and the rough's ladder starts at the bar's
//! skin; a bar the part-based rapid already clears, or no stock at all,
//! leaves the path exactly as it was. Issue #229: a pass that ends
//! inside a bar running on past it leaves its cut at feed.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (stock, prof,
//! blade, parting, thread, form) with this crate's own revolve fixture.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{
    groove_finish, groove_finish_in, profile_finish, profile_finish_in, profile_rough, profile_rough_in,
    GrooveFinishParams, ProfileFinishParams, ProfileRoughParams, StockEnv,
};

const TAU: f64 = std::f64::consts::TAU;
const NOSE: f64 = 0.8;
const CLEAR: f64 = 1.0;
/// The part's top end (Z): every window here climbs to it.
const PART_TOP: f64 = 28.0;

fn revolve(meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            } else {
                t.push(Triangle::new(a1, a2, b2));
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

/// A 12 mm part with a groove: r 6 over z -16 .. 28, floor r 4 over
/// z -6.8 .. -3.6.
fn part() -> Vec<Triangle> {
    revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, -6.8), (4.0, -6.8), (4.0, -3.6), (6.0, -3.6), (6.0, PART_TOP), (0.0, PART_TOP)])
}

fn rho(q: Point3) -> f64 {
    q.x.hypot(q.y)
}

/// A round bar of radius `r_out` the part's own length: it ends where
/// the part does, at Z [`PART_TOP`].
fn bar(r_out: f64) -> Option<StockEnv> {
    long_bar(r_out, 0.0)
}

/// [`bar`] running `past` mm on beyond the part's top end (#229).
fn long_bar(r_out: f64, past: f64) -> Option<StockEnv> {
    let s_top = Frame::about(&part()).expect("the part revolves").station_of(PART_TOP + past);
    Some(StockEnv { r_out, clearance: CLEAR, s_top, s_bot: f64::NAN })
}

fn same(a: &Toolpath, b: &Toolpath) -> bool {
    a.moves.len() == b.moves.len()
        && a.moves.iter().zip(&b.moves).all(|(p, q)| p.kind == q.kind && (p.to - q.to).dot(p.to - q.to) < 1e-18)
}

fn rough() -> ProfileRoughParams {
    ProfileRoughParams {
        clearance_z: 35.0,
        top_z: 28.0,
        bottom_z: -16.0,
        stepdown: 0.8,
        stock_to_leave: 0.2,
        feed: 150.0,
        plunge_feed: 80.0,
        nose_radius: NOSE,
        tolerance: 0.05,
    }
}

#[test]
fn a_rough_in_a_wider_bar_starts_at_the_bar_and_rapids_clear_of_it() {
    let path = profile_rough_in(&part(), &rough(), bar(10.0)).expect("rough");
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Rapid && (m.from.z - m.to.z).abs() > 1e-9) {
        assert!(rho(m.from) >= 10.0 + NOSE + CLEAR - 1e-9, "an axial rapid inside the bar: {m:?}");
    }
    // Over the plain wall (z 5) the ball's inner edge per pass: the
    // first bites under the bar's r 10 by at most a stepdown, the last
    // stands on the leave.
    let wall: Vec<f64> = path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z < 5.0 && m.to.z >= 5.0)
        .map(|m| {
            let t = (5.0 - m.from.z) / (m.to.z - m.from.z);
            rho(m.from) + (rho(m.to) - rho(m.from)) * t - NOSE
        })
        .collect();
    let top = wall.iter().copied().filter(|&r| r < 10.0).fold(f64::NEG_INFINITY, f64::max);
    assert!(top >= 10.0 - 0.8 - 1e-6, "the ladder does not start at the bar: {wall:?}");
    let floor = wall.iter().copied().fold(f64::INFINITY, f64::min);
    assert!((floor - 6.2).abs() < 0.02, "the ladder ends at the part plus the leave: {floor}");
}

#[test]
fn a_snug_bar_or_no_stock_leaves_the_part_based_path() {
    let blind = profile_rough(&part(), &rough()).expect("rough");
    assert!(same(&blind, &profile_rough_in(&part(), &rough(), None).expect("rough")));
    assert!(same(&blind, &profile_rough_in(&part(), &rough(), bar(6.0)).expect("rough")), "a snug bar moved the rough");
    let fin = ProfileFinishParams { clearance_z: 35.0, top_z: 28.0, bottom_z: -16.0, feed: 150.0, plunge_feed: 80.0, nose_radius: NOSE, tolerance: 0.05 };
    let blind = profile_finish(&part(), &fin).expect("finish");
    assert!(same(&blind, &profile_finish_in(&part(), &fin, bar(6.0)).expect("finish")), "a snug bar moved the finish");
    // A wider bar moves only the rapids out; the cut is the same ride.
    let wide = profile_finish_in(&part(), &fin, bar(10.0)).expect("finish");
    let feeds = |p: &Toolpath| p.moves.iter().filter(|m| m.kind == MoveKind::Feed).skip(1).map(|m| m.to).collect::<Vec<_>>();
    assert_eq!(feeds(&blind), feeds(&wide), "the finish ride itself moved");
}

#[test]
fn a_groove_finish_in_a_wider_bar_rapids_clear_of_it() {
    let p = GrooveFinishParams { tolerance: 0.05, clearance_z: 35.0, top_z: -3.6, bottom_z: -6.8, feed: 150.0, plunge_feed: 60.0, blade_width: 2.0 };
    let wide = groove_finish_in(&part(), &p, bar(10.0)).expect("groove");
    for m in wide.moves.iter().filter(|m| m.kind == MoveKind::Rapid && (m.from.z - m.to.z).abs() > 1e-9) {
        assert!(rho(m.from) >= 10.0 + CLEAR - 1e-9, "an axial rapid inside the bar: {m:?}");
    }
    let tips = |p: &Toolpath| p.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to).collect::<Vec<_>>();
    assert_eq!(tips(&wide), tips(&groove_finish(&part(), &p).expect("groove")), "the floor moved");
}

/// The moves of `path` that leave a pass's cut: radial moves outward
/// at the window's top station, world Z `top`.
fn exits(path: &Toolpath, top: f64) -> Vec<&Move> {
    let at_top = |q: Point3| (q.z - top).abs() < 1e-9;
    path.moves.iter().filter(|m| at_top(m.from) && at_top(m.to) && rho(m.to) > rho(m.from)).collect()
}

/// #229: a bar that runs on past the window's top leaves every pass
/// ending inside its uncut end, so each pass - the rough's rungs and
/// the finish's one ride alike - leaves its cut at the plunge feed.
/// Nothing else about the path moves.
#[test]
fn a_pass_ending_inside_a_longer_bar_leaves_its_cut_at_feed() {
    let short = profile_rough_in(&part(), &rough(), bar(10.0)).expect("rough");
    let long = profile_rough_in(&part(), &rough(), long_bar(10.0, 8.0)).expect("rough");
    let ends: Vec<Point3> = long.moves.iter().map(|m| m.to).collect();
    assert_eq!(ends, short.moves.iter().map(|m| m.to).collect::<Vec<_>>(), "the long bar moved the path");
    assert!(exits(&short, PART_TOP).iter().all(|m| m.kind == MoveKind::Rapid), "a bar ending with the part keeps the rapid out");
    let out = exits(&long, PART_TOP);
    assert!(out.len() > 1, "one exit per rung: {}", out.len());
    assert!(out.iter().all(|m| m.kind == MoveKind::Feed && m.feed == Some(rough().plunge_feed)), "{out:?}");
    let fin = ProfileFinishParams { clearance_z: 35.0, top_z: PART_TOP, bottom_z: -16.0, feed: 150.0, plunge_feed: 80.0, nose_radius: NOSE, tolerance: 0.05 };
    let finish = profile_finish_in(&part(), &fin, long_bar(10.0, 8.0)).expect("finish");
    let out = exits(&finish, PART_TOP);
    assert!(out.len() == 1 && out[0].kind == MoveKind::Feed, "{out:?}");
}

/// #229's other case: a window that stops partway along the bar. The
/// pass ends inside bar the cut never reaches, so it leaves at feed.
#[test]
fn a_pass_stopping_partway_along_the_bar_leaves_its_cut_at_feed() {
    const STOP: f64 = 20.0;
    let p = ProfileRoughParams { top_z: STOP, ..rough() };
    let path = profile_rough_in(&part(), &p, bar(10.0)).expect("rough");
    let out = exits(&path, STOP);
    assert!(!out.is_empty() && out.iter().all(|m| m.kind == MoveKind::Feed), "{out:?}");
}
