//! The Slot's start and exit picks (#3325, design #1448): the pass starts
//! at the slot end nearest the start pick and stops at the end nearest
//! the exit pick, a pick the pass cannot hold is refused by name, and a
//! slot without a pick is the earlier slot to the move.
//! Gate map: covers camcraft_motion::machining2d (slot_picks, slots).
//!
//! The law under the picks is closed form: `slot(b, a)` is `slot(a, b)`
//! turned half a turn about the slot's middle. A half turn keeps the
//! stadium, the rows and the climb sense, so a pick adds no move shape.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::Point2;
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{SlotParams, slot, slot_picked_ends};
use camcraft_motion::path_picks::{PathPick, PathPicks};

const A: Point2 = Point2 { x: -25.0, y: 15.0 };
const B: Point2 = Point2 { x: 25.0, y: 15.0 };

/// Tool 6 mm, two levels of 3 mm; `width` 6 is one row on the centerline.
fn params(width: f64, stepover: f64, climb: bool) -> SlotParams {
    SlotParams {
        tool_diameter: 6.0,
        width,
        stock_to_leave: 0.0,
        clearance_z: 17.0,
        retract_z: None,
        top_z: 12.0,
        bottom_z: 6.0,
        stepdown: 3.0,
        stepover,
        feed: 600.0,
        plunge_feed: 150.0,
        climb,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        links: Links::PLANAR,
        blend_turns: false,
        blend_radius: None,
    }
}

fn picks(start: Option<(f64, f64)>, exit: Option<(f64, f64)>) -> PathPicks {
    let one = |p: Option<(f64, f64)>| p.map(|(x, y)| PathPick::xy(Point2::new(x, y))).into_iter().collect();
    PathPicks { start: one(start), exit: one(exit) }
}

fn picked(p: &SlotParams, k: &PathPicks) -> Result<Toolpath, String> {
    let (a, b) = slot_picked_ends(A, B, p, k).map_err(|e| e.to_string())?;
    slot(a, b, p).map_err(|e| e.to_string())
}

fn dist(p: (f64, f64), q: Point2) -> f64 {
    ((p.0 - q.x).powi(2) + (p.1 - q.y).powi(2)).sqrt()
}

/// Is `p` nearer `A` than `B`?
fn near_a(p: (f64, f64)) -> bool {
    dist(p, A) < dist(p, B)
}

/// Where the cutter first goes down and where its last feed stops (XY).
fn entry_and_exit(path: &Toolpath) -> ((f64, f64), (f64, f64)) {
    let first = path.moves.iter().find(|m| m.kind != MoveKind::Rapid).expect("a first descent");
    let last = path.moves.iter().rev().find(|m| m.kind == MoveKind::Feed).expect("a last feed");
    ((first.to.x, first.to.y), (last.to.x, last.to.y))
}

/// Every slot shape of this file: one row, three rows, two rows; climb and not.
fn shapes() -> Vec<SlotParams> {
    let mut out = Vec::new();
    for (width, stepover) in [(6.0, 2.0), (10.0, 2.0), (10.0, 4.0)] {
        for climb in [true, false] {
            out.push(params(width, stepover, climb));
        }
    }
    out
}

#[test]
fn no_pick_is_the_earlier_slot() {
    for p in shapes() {
        let (a, b) = slot_picked_ends(A, B, &p, &PathPicks::default()).expect("no pick");
        assert_eq!((a, b), (A, B), "the ends come back as they are");
        let (plain, same) = (slot(A, B, &p).expect("plain"), picked(&p, &PathPicks::default()).expect("no pick"));
        assert_eq!(plain.moves, same.moves);
    }
}

#[test]
fn a_swap_is_the_same_pass_turned_half_a_turn() {
    // The hand-trace law: each move of slot(B, A) is the move of
    // slot(A, B) mirrored through the slot's middle, kind and feed kept.
    let (mx, my) = ((A.x + B.x) * 0.5, (A.y + B.y) * 0.5);
    for p in shapes() {
        let (fwd, rev) = (slot(A, B, &p).expect("a -> b"), slot(B, A, &p).expect("b -> a"));
        assert_eq!(fwd.moves.len(), rev.moves.len());
        // The first and the last move reach the approach origin; the cut is between them.
        for (f, r) in fwd.moves.iter().zip(&rev.moves).skip(1) {
            assert_eq!((f.kind, f.feed), (r.kind, r.feed));
            assert!((f.to.x + r.to.x - 2.0 * mx).abs() < 1e-9 && (f.to.y + r.to.y - 2.0 * my).abs() < 1e-9, "{f:?} / {r:?}");
            assert!((f.to.z - r.to.z).abs() < 1e-12);
            assert_eq!(f.arc.map(|a| a.ccw), r.arc.map(|a| a.ccw), "a half turn keeps each arc's sense");
        }
    }
}

#[test]
fn the_entry_is_at_the_end_nearest_the_start_pick() {
    for p in shapes() {
        for (pick, want_a) in [((-24.0, 16.0), true), ((26.0, 14.0), false)] {
            let path = picked(&p, &picks(Some(pick), None)).expect("a start pick beside an end is taken");
            let (entry, _) = entry_and_exit(&path);
            assert_eq!(near_a(entry), want_a, "width {} climb {}: entry {entry:?} for pick {pick:?}", p.width, p.climb);
            let end = if want_a { A } else { B };
            assert!(dist(entry, end) <= p.width * 0.5 - 3.0 + 1e-9, "the entry is on that end's round: {entry:?}");
        }
    }
}

#[test]
fn the_exit_is_at_the_end_nearest_the_exit_pick() {
    for p in shapes() {
        for (pick, want_a) in [((-24.0, 16.0), true), ((26.0, 14.0), false)] {
            let path = picked(&p, &picks(None, Some(pick))).expect("an exit pick beside an end is taken");
            let (_, exit) = entry_and_exit(&path);
            assert_eq!(near_a(exit), want_a, "width {} climb {}: exit {exit:?} for pick {pick:?}", p.width, p.climb);
            let end = if want_a { A } else { B };
            assert!(dist(exit, end) <= p.width * 0.5 - 3.0 + 1e-9, "the exit is on that end's round: {exit:?}");
        }
    }
}

#[test]
fn both_picks_hold_or_the_exit_is_refused_by_name() {
    let ends = [(-25.0, 15.0), (25.0, 15.0)];
    for p in shapes() {
        // Where the unpicked pass starts and stops says which pairs can hold.
        let (e0, x0) = entry_and_exit(&slot(A, B, &p).expect("plain"));
        let same_end = near_a(e0) == near_a(x0);
        for s in ends {
            for x in ends {
                let got = picked(&p, &picks(Some(s), Some(x)));
                if (s == x) == same_end {
                    let (entry, exit) = entry_and_exit(&got.expect("a pair the pass can hold"));
                    assert_eq!((near_a(entry), near_a(exit)), (near_a(s), near_a(x)));
                } else {
                    let why = got.expect_err("a pair the pass cannot hold");
                    assert!(why.contains("exit point") && why.contains("stops at the slot end"), "{why}");
                }
            }
        }
    }
    // Three rows stop opposite the start; two rows stop where they start.
    let odd = entry_and_exit(&slot(A, B, &params(10.0, 2.0, true)).expect("three rows"));
    let even = entry_and_exit(&slot(A, B, &params(10.0, 4.0, true)).expect("two rows"));
    assert!(near_a(odd.0) != near_a(odd.1) && near_a(even.0) == near_a(even.1), "{odd:?} {even:?}");
}

#[test]
fn a_pick_that_names_no_end_is_refused_by_name() {
    let p = params(10.0, 2.0, true);
    // Reach: cutter path half-width 2, a pick may sit 9 mm (1.5 d) from it.
    let far = picked(&p, &picks(Some((25.0, 15.0 + 2.0 + 9.5)), None)).expect_err("out of reach");
    assert!(far.contains("start point") && far.contains("farther than the 9.000 mm"), "{far}");
    assert!(picked(&p, &picks(Some((25.0, 15.0 + 2.0 + 8.5)), None)).is_ok(), "in reach");
    let past = picked(&p, &picks(None, Some((40.0, 15.0)))).expect_err("past the end");
    assert!(past.contains("exit point") && past.contains("farther"), "{past}");
    // The middle is the same distance from both ends.
    let mid = picked(&p, &picks(Some((0.0, 16.0)), None)).expect_err("the middle");
    assert!(mid.contains("start point") && mid.contains("same distance from both slot ends"), "{mid}");
    // A pick no planner can read.
    let nan = picked(&p, &picks(Some((f64::NAN, 0.0)), None)).expect_err("not finite");
    assert!(nan.contains("start point") && nan.contains("finite"), "{nan}");
    let two = PathPicks { start: vec![PathPick::xy(A), PathPick::xy(B)], exit: Vec::new() };
    assert!(picked(&p, &two).expect_err("two start picks").contains("one start point"));
}

#[test]
fn a_slot_that_cannot_be_cut_keeps_its_own_refusal_under_a_pick() {
    let k = picks(Some((25.0, 15.0)), None);
    // Narrower than the tool: the ends come back and `slot` says why.
    let narrow = params(4.0, 2.0, true);
    assert!(picked(&narrow, &k).expect_err("narrow").contains("narrower than the tool"));
    // Coincident ends.
    let p = params(6.0, 2.0, true);
    let (a, b) = slot_picked_ends(A, A, &p, &k).expect("the ends come back");
    assert!(slot(a, b, &p).is_err(), "coincident ends are refused by the slot");
    // Bad numbers are refused before a pick is read.
    let mut bad = params(6.0, 2.0, true);
    bad.stepover = 0.0;
    assert!(slot_picked_ends(A, B, &bad, &k).is_err());
}
