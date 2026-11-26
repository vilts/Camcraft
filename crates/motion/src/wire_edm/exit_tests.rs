//! The wire's exit pick (#3332): an open run ends at the end nearest
//! the pick, a closed profile is left at the pick after its whole lap,
//! and each pick the wire cannot hold is refused by name. The wire and
//! gap are SYNTHETIC - a plausible size, from no machine's manual
//! (#290 D4, D10).

use super::*;
use crate::wire_edm::{wire_contour, TabParams};

const K: f64 = 0.165;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap()
}

fn params(side: WireSide, start: Option<(f64, f64)>) -> WireParams {
    WireParams {
        wire_diameter: 0.25,
        gap: 0.04,
        condition: "C110".into(),
        side,
        start: start.map(|(x, y)| Point2::new(x, y)),
        plane_z: -12.0,
        stock: Some(Region::new(vec![rect(0.0, 0.0, 100.0, 50.0)]).unwrap()),
        clearance: 1.0,
        // A die on the sharp test rectangle: its corners are stated.
        sharp_corners: side == WireSide::Die,
        tabs: None,
    }
}

fn line() -> Vec<Vec<Point2>> {
    vec![vec![Point2::new(60.0, 0.0), Point2::new(60.0, 50.0)]]
}

fn pt(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn refusal(loops: &[Loop], chains: &[Vec<Point2>], p: &WireParams, exit: Point2) -> String {
    wire_contour_exit(loops, chains, p, exit).expect_err("refused").to_string()
}

#[test]
fn an_open_run_ends_at_the_end_nearest_the_exit_pick() {
    for side in [WireSide::Right, WireSide::Left] {
        let p = params(side, None);
        let auto = points(&wire_contour(&[], &line(), &p).unwrap());
        // The pick names the last end: the moves are the automatic cut's.
        let kept = wire_contour_exit(&[], &line(), &p, pt(55.0, 48.0)).unwrap();
        assert_eq!(points(&kept), auto);
        assert!(kept.notes.iter().any(|n| n.contains("exit point at X55.000 Y48.000") && n.contains("last end")), "{:?}", kept.notes);
        // The pick names the first end: the same points, the other way.
        let turned = wire_contour_exit(&[], &line(), &p, pt(70.0, 3.0)).unwrap();
        let v = points(&turned);
        let back: Vec<Point2> = auto.iter().rev().copied().collect();
        assert_eq!(v, back);
        assert!((v[0].y - (50.0 + K + 1.0)).abs() < 1e-6 && (v[v.len() - 1].y + (K + 1.0)).abs() < 1e-6, "{v:?}");
        let x = if side == WireSide::Right { 60.0 + K } else { 60.0 - K };
        assert!(v.iter().all(|q| (q.x - x).abs() < 1e-9), "the kerf stays on the stated side: {v:?}");
        assert!(turned.moves.iter().all(|m| m.kind == MoveKind::Feed && m.feed.is_none() && m.to.z == -12.0));
        let said = turned.notes.iter().find(|n| n.starts_with("wire EDM: threaded outside the stock")).expect("the threading note");
        assert!(said.contains("threaded outside the stock at X") && said.contains("Y51.165 and run out at") && said.contains("Y-1.165"), "{said}");
        assert_eq!(turned.notes.iter().filter(|n| n.contains("threaded outside the stock")).count(), 1, "one threading note");
        assert!(turned.notes.iter().any(|n| n.contains("first end") && n.contains("against")), "{:?}", turned.notes);
    }
}

#[test]
fn an_exit_pick_that_names_no_end_of_an_open_run_is_refused() {
    let p = params(WireSide::Right, None);
    let why = refusal(&[], &line(), &p, pt(80.0, 25.0));
    assert!(why.contains("exit point at X80.000 Y25.000") && why.contains("both ends") && why.contains("names no end"), "{why}");
    let why = refusal(&[], &line(), &p, pt(f64::NAN, 0.0));
    assert!(why.contains("exit point") && why.contains("not a finite point"), "{why}");
    // The run's own refusals come first: an exit does not excuse them.
    let why = refusal(&[], &line(), &params(WireSide::Die, None), pt(60.0, 49.0));
    assert!(why.contains("no punch or die side"), "{why}");
}

#[test]
fn a_closed_profile_is_cut_all_the_way_round_and_left_at_the_exit_pick() {
    let profile = [rect(-20.0, -10.0, 20.0, 10.0)];
    for (side, start, exit, foot) in [
        (WireSide::Punch, (-30.0, 0.0), pt(30.0, 2.0), pt(20.0 + K, 2.0)),
        (WireSide::Punch, (-30.0, 0.0), pt(-30.0, 5.0), pt(-20.0 - K, 5.0)),
        (WireSide::Punch, (-30.0, 0.0), pt(-30.0, -5.0), pt(-20.0 - K, -5.0)),
        (WireSide::Die, (0.0, 0.0), pt(15.0, 0.0), pt(20.0 - K, 0.0)),
    ] {
        let p = params(side, Some(start));
        let auto = points(&wire_contour(&profile, &[], &p).unwrap());
        let path = wire_contour_exit(&profile, &[], &p, exit).unwrap();
        let v = points(&path);
        let n = v.len();
        assert_eq!(v[n - 1], exit, "{side:?}: the cut ends at the pick");
        assert!(dist(v[n - 2], foot) < 1e-9, "{side:?}: left from the path's nearest point {:?}", v[n - 2]);
        // The whole lap, seam to seam, is the automatic cut's.
        assert_eq!(&v[..auto.len() - 1], &auto[..auto.len() - 1], "{side:?}");
        // After the seam the wire stays on the path it has cut.
        let lap = &auto[1..auto.len() - 1];
        for q in &v[auto.len() - 1..n - 1] {
            assert!(ring::pt_ring(*q, &lap[..lap.len() - 1]) < 1e-9, "{side:?}: {q:?} is off the cut path");
        }
        assert!(path.moves.iter().all(|m| m.kind == MoveKind::Feed && m.feed.is_none() && m.to.z == -12.0));
        let said = path.notes.iter().find(|n| n.starts_with("wire EDM: threaded at")).expect("the threading note");
        assert!(said.contains("lead-out of") && said.contains("does not return to the threading point"), "{said}");
        assert!(!path.notes.iter().any(|n| n.contains("back along the lead-in")), "{:?}", path.notes);
    }
}

/// The ride from the seam to the exit's foot goes the lap's own way
/// round, and a foot behind the seam on the seam's own side takes the
/// whole lap again.
#[test]
fn the_ride_to_the_exit_follows_the_lap() {
    let profile = [rect(-20.0, -10.0, 20.0, 10.0)];
    let p = params(WireSide::Punch, Some((-30.0, 0.0)));
    let len = |v: &[Point2]| v.windows(2).map(|w| dist(w[0], w[1])).sum::<f64>();
    let auto = len(&points(&wire_contour(&profile, &[], &p).unwrap()));
    let lap = 2.0 * (40.0 + 2.0 * K) + 2.0 * (20.0 + 2.0 * K);
    // The lap is counter-clockwise: from the seam on the left side it
    // runs down (-Y). A foot 5 mm below the seam is 5 mm on.
    let ahead = len(&points(&wire_contour_exit(&profile, &[], &p, pt(-30.0, -5.0)).unwrap()));
    assert!((ahead - (auto + 5.0)).abs() < 1e-6, "{ahead} vs {auto}");
    // A foot 5 mm above the seam is a whole lap less 5 mm on.
    let behind = len(&points(&wire_contour_exit(&profile, &[], &p, pt(-30.0, 5.0)).unwrap()));
    assert!((behind - (auto + lap - 5.0)).abs() < 1e-6, "{behind} vs {auto} + {lap}");
}

#[test]
fn each_exit_pick_a_closed_profile_cannot_hold_is_refused_by_name() {
    let profile = [rect(-20.0, -10.0, 20.0, 10.0)];
    let punch = params(WireSide::Punch, Some((-30.0, 0.0)));
    let die = params(WireSide::Die, Some((0.0, 0.0)));
    let why = refusal(&profile, &[], &punch, pt(0.0, 0.0));
    assert!(why.contains("exit point at X0.000 Y0.000") && why.contains("inside the profile") && why.contains("punch"), "{why}");
    let why = refusal(&profile, &[], &punch, pt(20.1, 0.0));
    assert!(why.contains("exit point") && why.contains("closer to the profile than the kerf offset"), "{why}");
    let why = refusal(&profile, &[], &punch, pt(20.0 + K, 0.0));
    assert!(why.contains("exit point") && why.contains("on the wire path itself"), "{why}");
    let why = refusal(&profile, &[], &die, pt(30.0, 0.0));
    assert!(why.contains("exit point at X30.000 Y0.000") && why.contains("outside the profile") && why.contains("die"), "{why}");
    let why = refusal(&profile, &[], &die, pt(19.9, 0.0));
    assert!(why.contains("exit point") && why.contains("closer to the profile than the kerf offset"), "{why}");
    // Tabs keep the wire threaded at the threading point.
    let tabs = TabParams { count: Some(1), width: Some(2.0), at: vec![pt(20.0, 0.0)], cutoff: Some(("C110".into(), K)) };
    let why = refusal(&profile, &[], &WireParams { tabs: Some(tabs), ..punch.clone() }, pt(30.0, 2.0));
    assert!(why.contains("exit point") && why.contains("tabs"), "{why}");
    // The cut's own refusals come first: a closed profile with no
    // threading point stays refused, with or without an exit.
    let why = refusal(&profile, &[], &WireParams { start: None, ..punch }, pt(30.0, 2.0));
    assert!(why.contains("no start hole is stated"), "{why}");
}
