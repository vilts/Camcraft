//! Pins for the thread mill's ride (#894): hand and climb senses, one shared helix across passes, the pass depths, the floor and rapid laws, the lead arcs' form law, and the refusals.
//!
//! Provenance: the laws are the public handbook statements
//! [`super::thread_mill`] cites; the checks are our own.

use crate::cl::{MoveKind, ThreadHand, Toolpath};
use crate::geo2d::Point2;

use super::thread_mill::{thread_mill, ThreadHole, ThreadParams, EXTERNAL_THREAD_DEPTH_PER_PITCH, THREAD_DEPTH_PER_PITCH};

const R: f64 = 5.0;
const TOP: f64 = 0.0;
const FLOOR: f64 = -6.0;

fn params(rows: u32, external: bool, hand: ThreadHand, climb: bool) -> ThreadParams {
    ThreadParams {
        holes: vec![ThreadHole { xy: Point2::new(10.0, 20.0), radius: R, top_z: TOP, floor_z: FLOOR, stepped: false, through: false }],
        pitch: 1.5,
        tool_diameter: 6.0,
        rows,
        infeed: 0.3,
        external,
        hand,
        climb,
        passes: super::ThreadPasses::default(),
        clearance_z: 10.0,
        r_z: None,
        retract_z: None,
        feed: 400.0,
        plunge_feed: 120.0,
    }
}

fn ride_of(p: &ThreadParams) -> Toolpath {
    thread_mill(p).expect("a valid thread ride")
}

/// Distance of a point from the first feature's axis.
fn rho(p: &ThreadParams, x: f64, y: f64) -> f64 {
    (x - p.holes[0].xy.x).hypot(y - p.holes[0].xy.y)
}

/// The helix's own half-arcs: XY arcs about the feature's axis.
fn orbit(p: &ThreadParams, path: &Toolpath) -> Vec<crate::cl::Move> {
    let c = p.holes[0].xy;
    path.moves
        .iter()
        .filter(|m| m.arc.is_some_and(|a| (a.center.x - c.x).abs() < 1e-9 && (a.center.y - c.y).abs() < 1e-9))
        .copied()
        .collect()
}

fn err_of(p: &ThreadParams) -> String {
    format!("{:?}", thread_mill(p).expect_err("refused"))
}

#[test]
fn every_arc_lays_the_stated_hand_and_orbits_by_the_climb_rule() {
    for external in [false, true] {
        for hand in ThreadHand::ALL {
            for climb in [true, false] {
                for rows in [1, 4] {
                    let p = params(rows, external, hand, climb);
                    let path = ride_of(&p);
                    let helix = orbit(&p, &path);
                    assert!(helix.len() >= 2, "a helix posts its half-turns");
                    for m in &helix {
                        assert_eq!(ThreadHand::of_move(m), Some(hand), "{external} {hand:?} {climb} {rows}: {m:?}");
                        // Climb under an M3 spindle: counter-clockwise in a hole, clockwise round a boss.
                        assert_eq!(m.arc.map(|a| a.ccw), Some(climb != external));
                        assert!(((m.to.z - m.from.z).abs() - 0.75).abs() < 1e-9, "half a pitch per half-turn");
                    }
                }
            }
        }
    }
    // The handbook's everyday case: right-hand, internal, climb - from the bottom up, counter-clockwise.
    let p = params(1, false, ThreadHand::Right, true);
    assert!(p.ccw() && p.rising());
    assert!(orbit(&p, &ride_of(&p)).iter().all(|m| m.to.z > m.from.z));
}

#[test]
fn radial_passes_are_equal_steps_landing_on_the_thread_depth() {
    for external in [false, true] {
        let p = params(1, external, ThreadHand::Right, true);
        let h = 1.5 * if external { EXTERNAL_THREAD_DEPTH_PER_PITCH } else { THREAD_DEPTH_PER_PITCH };
        let n = (h / 0.3).ceil();
        let mut radii: Vec<f64> = orbit(&p, &ride_of(&p)).iter().map(|m| rho(&p, m.to.x, m.to.y)).collect();
        radii.sort_by(f64::total_cmp);
        radii.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
        assert_eq!(radii.len() as f64, n, "{radii:?}");
        for (i, got) in radii.iter().enumerate() {
            let k = if external { n - i as f64 } else { i as f64 + 1.0 };
            let want = if external { R + 3.0 - h * k / n } else { R - 3.0 + h * k / n };
            assert!((got - want).abs() < 2e-6, "pass {i}: {got} vs {want}");
        }
    }
}

#[test]
fn all_passes_ride_one_helix_and_cover_the_thread() {
    for (rows, external) in [(1, false), (4, false), (1, true), (4, true)] {
        let p = params(rows, external, ThreadHand::Right, true);
        let path = ride_of(&p);
        let helix = orbit(&p, &path);
        // At angle zero (the +X side) every pass and level meets the helix a whole number of pitches apart.
        let at_zero: Vec<f64> = helix.iter().filter(|m| m.to.x > p.holes[0].xy.x).map(|m| m.to.z).collect();
        for z in &at_zero {
            let turns = (z - at_zero[0]) / 1.5;
            assert!((turns - turns.round()).abs() < 1e-6, "rows {rows}: {z} is off the helix");
        }
        // The cut runs out of the top: the highest tooth row (rows - 1 pitches above the lowest) ends at or above the entry.
        let hi = helix.iter().map(|m| m.to.z.max(m.from.z)).fold(f64::NEG_INFINITY, f64::max);
        assert!(hi + f64::from(rows - 1) * 1.5 >= TOP - 1e-9, "rows {rows}: the thread stops short at {hi}");
        // A multi-form mill turns once per level; a single-form mill winds the whole thread.
        let zs: Vec<f64> = helix.iter().filter(|m| (rho(&p, m.to.x, m.to.y) - rho(&p, helix[0].to.x, helix[0].to.y)).abs() < 1e-6).map(|m| m.to.z).collect();
        let half_turns_per_pass = zs.len();
        // 6 mm of thread at pitch 1.5: four turns of one tooth, or one revolution of four rows.
        let turns = if rows == 1 { 4 } else { 1 };
        assert_eq!(half_turns_per_pass, 2 * turns, "rows {rows} external {external}");
    }
    // A thread longer than the cutting length takes a second level, one cutting length higher.
    let mut deep = params(2, false, ThreadHand::Right, true);
    deep.holes[0].floor_z = -6.0;
    let zs: Vec<f64> = orbit(&deep, &ride_of(&deep)).iter().flat_map(|m| [m.from.z, m.to.z]).collect();
    let (lo, hi) = (zs.iter().copied().fold(f64::INFINITY, f64::min), zs.iter().copied().fold(f64::NEG_INFINITY, f64::max));
    assert!((hi - lo - (3.0 + 1.5)).abs() < 1e-9, "two levels one cutting length (3 mm) apart, one pitch of travel each: {lo}..{hi}");
    {
    }
}

#[test]
fn the_tip_keeps_the_floor_and_no_rapid_runs_below_the_top() {
    for hand in ThreadHand::ALL {
        for through in [false, true] {
            let mut p = params(1, false, hand, true);
            p.holes[0].through = through;
            let path = ride_of(&p);
            let low = path.moves.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
            let want = if through { FLOOR - 1.5 } else { FLOOR };
            assert!((low - want).abs() < 1e-9, "{hand:?} through {through}: lowest {low}");
            for m in path.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
                assert!(m.to.z >= TOP - 1e-9 && m.from.z >= TOP - 1e-9, "a rapid below the top: {m:?}");
            }
            assert!(path.is_chained());
        }
    }
}

#[test]
fn the_teeth_never_leave_the_thread_form_on_the_lead_arcs() {
    // A 60-degree tooth displaced axially by dz from the groove stays inside it only if it has backed out
    // radially by at least sqrt(3) x dz. Inside a hole the lead-in from the axis is on the helix exactly.
    for external in [false, true] {
        for hand in ThreadHand::ALL {
            let p = params(1, external, hand, true);
            let c = p.holes[0].xy;
            let path = ride_of(&p);
            for (i, m) in path.moves.iter().enumerate() {
                let Some(arc) = m.arc else { continue };
                if (arc.center.x - c.x).abs() < 1e-9 {
                    continue; // the helix itself
                }
                // The tangent point: the lead's end on the centre circle (lead-in) or its start (lead-out).
                let on_circle = if path.moves.get(i + 1).is_some_and(|n| n.arc.is_some_and(|a| (a.center.x - c.x).abs() < 1e-9)) { m.to } else { m.from };
                let big = rho(&p, on_circle.x, on_circle.y);
                let rate = 1.5 / std::f64::consts::TAU * if p.rising() { 1.0 } else { -1.0 } * if p.ccw() { 1.0 } else { -1.0 };
                let th0 = (on_circle.y - c.y).atan2(on_circle.x - c.x);
                for k in 1..40 {
                    let q = arc.point_at(m.from, m.to, f64::from(k) / 40.0).expect("on the arc");
                    let r = rho(&p, q.x, q.y);
                    if r < 1e-6 {
                        continue;
                    }
                    let mut dth = (q.y - c.y).atan2(q.x - c.x) - th0;
                    dth -= (dth / std::f64::consts::TAU).round() * std::f64::consts::TAU;
                    let off_helix = (q.z - (on_circle.z + rate * dth)).abs();
                    let backed_out = (big - r).abs();
                    assert!(backed_out + 1e-3 >= 3f64.sqrt() * off_helix, "external {external}: {backed_out} vs {off_helix}");
                    if !external {
                        assert!(off_helix < 2e-6, "the semicircle from the axis is on the helix: {off_helix}");
                    }
                }
            }
        }
    }
}

#[test]
fn the_centre_feed_is_the_edge_feed_scaled_by_the_circles() {
    // Final internal pass: centre circle R + H - r, edge circle R + H.
    let p = params(1, false, ThreadHand::Right, true);
    let h = 1.5 * THREAD_DEPTH_PER_PITCH;
    let slowest = orbit(&p, &ride_of(&p)).iter().filter_map(|m| m.feed).fold(0.0, f64::max);
    assert!((slowest - 400.0 * (R + h - 3.0) / (R + h)).abs() < 1e-3, "{slowest}");
    // Round a boss the centre runs the larger circle, so it is programmed faster than the edge feed.
    let e = params(1, true, ThreadHand::Right, true);
    assert!(orbit(&e, &ride_of(&e)).iter().all(|m| m.feed.is_some_and(|f| f > 400.0)));
}

#[test]
fn a_boss_is_never_cut_past_its_thread_depth() {
    let p = params(4, true, ThreadHand::Left, false);
    let h = 1.5 * EXTERNAL_THREAD_DEPTH_PER_PITCH;
    for m in ride_of(&p).moves.iter().filter(|m| m.to.z < TOP) {
        assert!(rho(&p, m.to.x, m.to.y) >= R + 3.0 - h - 2e-6, "{m:?}");
    }
}

#[test]
fn impossible_threads_are_refused_by_name() {
    let base = params(1, false, ThreadHand::Right, true);
    assert!(err_of(&ThreadParams { tool_diameter: 12.0, ..base.clone() }).contains("no air to enter"));
    assert!(err_of(&ThreadParams { pitch: 0.0, ..base.clone() }).contains("positive pitch"));
    assert!(err_of(&ThreadParams { infeed: 0.0, ..base.clone() }).contains("infeed"));
    assert!(err_of(&ThreadParams { infeed: 5.0, ..base.clone() }).contains("infeed"));
    assert!(err_of(&ThreadParams { rows: 0, ..base.clone() }).contains("row of teeth"));
    assert!(err_of(&ThreadParams { tool_diameter: 1.5, ..base.clone() }).contains("no core"));
    assert!(err_of(&ThreadParams { feed: 0.0, ..base.clone() }).contains("feeds"));
    assert!(err_of(&ThreadParams { clearance_z: 0.5, ..base.clone() }).contains("retract plane"));
    assert!(err_of(&ThreadParams { retract_z: Some(-1.0), ..base.clone() }).contains("retract plane"));
    assert!(err_of(&ThreadParams { holes: Vec::new(), ..base.clone() }).contains("at least one hole"));
    let mut stepped = base.clone();
    stepped.holes[0].stepped = true;
    assert!(err_of(&stepped).contains("counterbore fold"));
    let mut nan = base.clone();
    nan.holes[0].radius = f64::NAN;
    assert!(err_of(&nan).contains("finite"));
    let mut thin = params(1, true, ThreadHand::Right, true);
    thin.holes[0].radius = 0.5;
    assert!(err_of(&thin).contains("thinner than"));
}

#[test]
fn the_ride_rapids_to_the_r_plane_and_starts_its_feed_there() {
    // #1212: a raised R plane, inside and beside, as on a drill cycle.
    for external in [false, true] {
        let base = params(1, external, ThreadHand::Right, true);
        // An R plane on the feature's top is the #894 ride, move for move.
        assert_eq!(ride_of(&ThreadParams { r_z: Some(TOP), ..base.clone() }), ride_of(&base));
        let p = ThreadParams { r_z: Some(TOP + 3.0), ..base };
        let path = ride_of(&p);
        let first_feed = path.moves.iter().position(|m| m.kind == MoveKind::Feed).expect("a feed");
        let dive = &path.moves[first_feed - 1];
        assert_eq!((dive.kind, dive.to.z), (MoveKind::Rapid, TOP + 3.0), "the rapid stops on R");
        assert_eq!(path.moves[first_feed].from.z, TOP + 3.0, "the feed starts on R");
        assert!(path.moves.iter().filter(|m| m.kind == MoveKind::Rapid).all(|m| m.to.z >= TOP + 3.0 - 1e-9));
        // The ride back out climbs to R at feed, then links.
        let out = path.moves.iter().rposition(|m| m.kind == MoveKind::Feed).expect("a feed");
        assert_eq!(path.moves[out].to.z, TOP + 3.0);
    }
}

#[test]
fn an_r_plane_under_a_top_or_over_the_link_plane_is_refused_by_name() {
    for (external, what) in [(false, "hole"), (true, "boss")] {
        let base = params(1, external, ThreadHand::Right, true);
        let low = err_of(&ThreadParams { r_z: Some(TOP - 0.5), ..base.clone() });
        assert!(low.contains(&format!("R plane at z -0.5 is below the top of the {what} at (10, 20) at z 0")), "{low}");
        let high = err_of(&ThreadParams { r_z: Some(11.0), ..base.clone() });
        assert!(high.contains("stands above the 10 mm retract plane"), "{high}");
        assert!(err_of(&ThreadParams { r_z: Some(f64::NAN), ..base }).contains("finite"));
    }
}
