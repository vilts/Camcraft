//! Tests of `sinker_picks` (#3333): the picked orbit keeps every law of
//! the automatic one, and the picked side burn visits the same positions.

use super::*;
use crate::geo2d::Point2;
use crate::path_picks::PathPick;
use crate::sinker::SinkerDatum;
use crate::sinker_side::SideDirection;
use crate::sinker_wear::{burn_compensated, burn_compensated_between, WearComp};

fn orbit(kind: OrbitKind, radius: f64) -> Orbit {
    Orbit { kind, radius, vectors: 4, turns: 3 }
}

fn picks(start: Option<(f64, f64)>, exit: Option<(f64, f64)>) -> PathPicks {
    let list = |q: Option<(f64, f64)>| q.map(|(x, y)| PathPick::xy(Point2::new(x, y))).into_iter().collect();
    PathPicks { start: list(start), exit: list(exit) }
}

fn params(orbit: Option<Orbit>) -> SinkerParams {
    SinkerParams {
        x: 2.0,
        y: -1.0,
        approach_z: 25.0,
        top_z: 20.0,
        floor_z: 10.0,
        model_floor_z: Some(0.0),
        datum: SinkerDatum::WorkTop,
        depth: 6.0,
        electrode_len: 30.0,
        feed: 40.0,
        gap: 0.05,
        stand: None,
        orbit,
    }
}

#[test]
fn sinker_picks_no_pick_is_the_automatic_orbit_and_a_start_at_plus_x_is_too() {
    for kind in [OrbitKind::Circular, OrbitKind::Vector, OrbitKind::Planetary] {
        for r in [0.0005, 0.05, 0.3, 1.7, 12.0] {
            let o = orbit(kind, r);
            let auto = orbit_offsets(&o).expect("an orbit");
            assert_eq!(orbit_offsets_between(&o, &OrbitEnds::default()).expect("auto"), auto);
            let plus_x = OrbitEnds { start: Some(0.0), exit: None };
            assert_eq!(orbit_offsets_between(&o, &plus_x).expect("from +X"), auto, "{kind:?} r {r}");
        }
    }
}

#[test]
fn sinker_picks_every_picked_orbit_stays_inside_its_radius_and_keeps_the_axis_vertices() {
    let angles = [0.0, 0.3, 1.0, std::f64::consts::FRAC_PI_2, 2.5, std::f64::consts::PI, 4.0, 6.2];
    for kind in [OrbitKind::Circular, OrbitKind::Planetary] {
        for r in [0.0005, 0.05, 0.3, 1.7, 12.0] {
            for a0 in angles {
                for a1 in angles {
                    let ends = OrbitEnds { start: Some(a0), exit: Some(a1) };
                    let pts = orbit_offsets_between(&orbit(kind, r), &ends).expect("an orbit");
                    assert!(pts.iter().all(|(x, y)| x.hypot(*y) <= r), "{kind:?} r {r} {a0} {a1} overshoots");
                    assert_eq!(pts.last(), Some(&(0.0, 0.0)), "returns to the burn position");
                    for axis in [(r, 0.0), (0.0, r), (-r, 0.0), (0.0, -r)] {
                        let on = pts.iter().any(|p| (p.0 - axis.0).abs() < 1e-12 && (p.1 - axis.1).abs() < 1e-12);
                        assert!(on, "{kind:?} r {r} {a0} {a1}: the axis vertex {axis:?} is kept");
                    }
                    // Every vertex of the automatic circle is still a vertex.
                    let auto = orbit_offsets(&orbit(OrbitKind::Circular, r)).expect("circle");
                    assert!(auto.iter().all(|p| pts.contains(p)), "{kind:?} r {r} {a0} {a1}: a circle vertex is lost");
                    // The start and the exit are on the radius at their angles.
                    let rim: Vec<_> = pts.iter().filter(|p| p.0.hypot(p.1) > r * (1.0 - 1e-9)).collect();
                    let at = |p: &(f64, f64), a: f64| apart(p.1.atan2(p.0).rem_euclid(TAU), a) < 1e-6;
                    assert!(at(rim[0], a0), "{kind:?} r {r}: the rim starts at {a0}");
                    assert!(at(rim[rim.len() - 1], a1), "{kind:?} r {r}: the rim ends at {a1}");
                }
            }
        }
    }
}

#[test]
fn sinker_picks_a_vector_orbit_keeps_its_directions_in_the_picked_order() {
    let o = Orbit { vectors: 8, ..orbit(OrbitKind::Vector, 0.2) };
    let auto = orbit_offsets(&o).expect("vectors");
    let step = TAU / 8.0;
    for s in 0..8u32 {
        let ends = OrbitEnds { start: Some(f64::from(s) * step), exit: None };
        let pts = orbit_offsets_between(&o, &ends).expect("vectors");
        assert_eq!(pts.len(), auto.len());
        assert_eq!(pts[0], auto[(2 * s) as usize], "the picked vector is first");
        let mut sorted = pts.clone();
        let mut want = auto.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        want.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        assert_eq!(sorted, want, "the same vectors, in a different order");
    }
    let e = orbit_offsets_between(&o, &OrbitEnds { start: Some(0.1), exit: None }).expect_err("between two vectors");
    assert!(e.to_string().contains("on no vector"), "{e}");
}

#[test]
fn sinker_picks_orbit_ends_reads_angles_and_refuses_by_name() {
    let c = (2.0, -1.0);
    let o = orbit(OrbitKind::Circular, 0.2);
    let ends = orbit_ends(c, Some(&o), &picks(Some((2.0, 4.0)), Some((-3.0, -1.0)))).expect("angles");
    assert!((ends.start.expect("start") - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    assert!((ends.exit.expect("exit") - std::f64::consts::PI).abs() < 1e-12);
    assert_eq!(orbit_ends(c, None, &PathPicks::default()), Ok(OrbitEnds::default()), "no pick, no orbit: nothing to refuse");
    let e = orbit_ends(c, None, &picks(None, Some((0.0, 0.0)))).expect_err("no orbit");
    assert!(e.starts_with("Sinker Burn:") && e.contains("exit point is not taken without an orbit"), "{e}");
    let e = orbit_ends(c, Some(&o), &picks(Some(c), None)).expect_err("on the burn position");
    assert!(e.contains("the start point is on the burn position"), "{e}");
    let e = orbit_ends(c, Some(&o), &picks(Some((f64::NAN, 0.0)), None)).expect_err("not a point");
    assert!(e.contains("finite"), "{e}");
    let v = orbit(OrbitKind::Vector, 0.2);
    let e = orbit_ends(c, Some(&v), &picks(Some((3.0, 0.0)), None)).expect_err("between two vectors");
    assert!(e.contains("the start point is on no vector"), "{e}");
    let e = orbit_ends(c, Some(&v), &picks(Some((3.0, -1.0)), Some((1.0, -1.0)))).expect_err("the opposite vector");
    assert!(e.contains("the exit point is on the vector at 180.000 degrees"), "{e}");
    assert!(orbit_ends(c, Some(&v), &picks(Some((3.0, -1.0)), Some((2.0, 1.0)))).is_ok(), "the next vector round ends the order");
}

#[test]
fn sinker_picks_a_layered_worn_burn_orbits_from_the_pick_at_every_layer() {
    let p = params(Some(orbit(OrbitKind::Circular, 0.3)));
    let w = SinkerWear { comp: Some(WearComp { rate: 0.02, from: "test".into() }), layer: Some(2.0) };
    let plain = burn_compensated(&p, &w).expect("burns");
    let auto = burn_compensated_between(&p, &w, &OrbitEnds::default()).expect("burns");
    assert_eq!((&auto.moves, &auto.notes), (&plain.moves, &plain.notes), "no pick is the burn of every earlier file");
    let ends = OrbitEnds { start: Some(std::f64::consts::FRAC_PI_2), exit: Some(std::f64::consts::PI) };
    let path = burn_compensated_between(&p, &w, &ends).expect("burns");
    let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    // After each plunge (a feed down on the burn axis) the orbit starts at
    // the pick; before each next plunge and the retract it is at the centre.
    let mut layers = 0;
    for (i, m) in feeds.iter().enumerate() {
        if m.from.z > m.to.z {
            layers += 1;
            assert_eq!((m.to.x, m.to.y), (p.x, p.y), "a plunge stays on the burn axis");
            let first = feeds[i + 1];
            assert!((first.to.x - p.x).abs() < 1e-12 && (first.to.y - (p.y + 0.3)).abs() < 1e-12, "{:?}", first.to);
            assert_eq!(first.to.z, m.to.z, "the orbit rides its layer's tip");
        }
    }
    assert_eq!(layers, 3, "three layers of 2 mm");
    let deepest = crate::sinker_wear::deepest_tip(&p, 0.02);
    assert!(path.moves.iter().all(|m| m.to.z >= deepest - 1e-9), "no move below the compensated floor");
    let last = path.moves.last().expect("a retract");
    assert_eq!((last.kind, last.from.x, last.from.y, last.to.z), (MoveKind::Rapid, p.x, p.y, 25.0));
    assert!(path.notes.iter().any(|n| n.contains("start point's angle (90.000") && n.contains("exit point's angle (180.000")), "{:?}", path.notes);
}

#[test]
fn sinker_picks_side_ends_choose_the_nearer_end_and_refuse_the_middle() {
    let c = (2.0, -1.0);
    let s = SideBurn { direction: SideDirection::MinusY, amount: 0.15, undersize: 0.2 };
    assert_eq!(side_ends(c, &s, &PathPicks::default()), Ok(SideEnds::default()));
    let got = side_ends(c, &s, &picks(Some((9.0, -1.2)), Some((2.0, -1.0)))).expect("ends");
    assert_eq!(got, SideEnds { start: SideEnd::Wall, exit: SideEnd::BurnPosition });
    let got = side_ends(c, &s, &picks(None, Some((2.0, -5.0)))).expect("ends");
    assert_eq!(got, SideEnds { start: SideEnd::BurnPosition, exit: SideEnd::Wall });
    let e = side_ends(c, &s, &picks(Some((0.0, -1.075)), None)).expect_err("the middle");
    assert!(e.contains("Sinker Side Burn: the start point is at the middle of the side move") && e.contains("-Y wall"), "{e}");
    let e = side_ends(c, &SideBurn { amount: f64::NAN, ..s }, &picks(Some((0.0, 0.0)), None)).expect_err("no amount");
    assert!(e.contains("moves no wall"), "{e}");
}

#[test]
fn sinker_picks_side_burn_between_visits_the_same_positions_and_keeps_every_refusal() {
    let p = params(None);
    let s = SideBurn { direction: SideDirection::PlusX, amount: 0.15, undersize: 0.2 };
    let w = SinkerWear { comp: Some(WearComp { rate: 0.02, from: "test".into() }), layer: Some(3.0) };
    let plain = side_burn_worn(&p, &s, &w).expect("burns");
    let auto = side_burn_between(&p, &s, &w, &SideEnds::default()).expect("burns");
    assert_eq!((&auto.moves, &auto.notes), (&plain.moves, &plain.notes), "the default ends are the side burn of every earlier file");
    let tip = crate::sinker_wear::deepest_tip(&p, 0.02);
    let (b, wall) = ((p.x, p.y), (p.x + 0.15, p.y));
    for (ends, want) in [
        (SideEnds { start: SideEnd::BurnPosition, exit: SideEnd::Wall }, vec![b, b, wall]),
        (SideEnds { start: SideEnd::Wall, exit: SideEnd::BurnPosition }, vec![wall, wall, b]),
        (SideEnds { start: SideEnd::Wall, exit: SideEnd::Wall }, vec![wall, wall, b, wall]),
    ] {
        let path = side_burn_between(&p, &s, &w, &ends).expect("burns");
        let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        assert_eq!(feeds.iter().map(|m| (m.to.x, m.to.y)).collect::<Vec<_>>(), want, "{ends:?}: two layers, then the side move");
        let plain_z: Vec<_> = plain.moves.iter().filter(|m| m.kind == MoveKind::Feed).take(2).map(|m| m.to.z).collect();
        assert_eq!(feeds.iter().take(2).map(|m| m.to.z).collect::<Vec<_>>(), plain_z, "{ends:?}: the plunge keeps its compensated layers");
        assert!(feeds.iter().skip(2).all(|m| (m.to.z - tip).abs() < 1e-9), "{ends:?}: the side move rides the deepest tip");
        assert!(path.moves.iter().all(|m| m.to.z >= tip - 1e-9), "{ends:?}: no move below the compensated floor");
        let (first, last) = (path.moves.first().expect("approach"), path.moves.last().expect("retract"));
        assert_eq!((first.kind, first.to.x, first.to.y, first.to.z), (MoveKind::Rapid, want[0].0, want[0].1, 25.0));
        let end = want[want.len() - 1];
        assert_eq!((last.kind, last.from.x, last.from.y, last.to.x, last.to.y, last.to.z), (MoveKind::Rapid, end.0, end.1, end.0, end.1, 25.0));
        assert!(plain.notes.iter().all(|n| path.notes.contains(n)), "{ends:?}: the plain notes are kept");
    }
    // The allowance law, the wear law and the orbit refusal answer first.
    let ends = SideEnds { start: SideEnd::Wall, exit: SideEnd::Wall };
    let e = side_burn_between(&p, &SideBurn { amount: 0.16, ..s }, &w, &ends).expect_err("past the allowance");
    assert!(e.to_string().contains("burns into the part"), "{e}");
    let bad = SinkerWear { comp: Some(WearComp { rate: 1.0, from: "test".into() }), layer: None };
    let e = side_burn_between(&p, &s, &bad, &ends).expect_err("rate");
    assert!(e.to_string().contains("not an axial wear rate"), "{e}");
    let e = side_burn_between(&params(Some(orbit(OrbitKind::Circular, 0.1))), &s, &w, &ends).expect_err("orbit");
    assert!(e.to_string().contains("takes no orbit"), "{e}");
}
