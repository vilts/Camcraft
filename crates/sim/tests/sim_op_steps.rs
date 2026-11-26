//! #2608: the operation steps of the simulation timeline. "Next" from
//! inside an operation goes to the start of the next one; "previous"
//! goes to the start of the current one, then to the one before it. The
//! first and the last operation have no step past them, and an operation
//! with no moves is not a stop.
//! Provenance: our own tests over our own timeline.
//! Gate map: covers camcraft_sim::sim (sim_steps).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geometry::Point3;
use camcraft_sim::sim::{OpStarts, Script, Traverse, script};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A short path: a rapid in, a plunge and one cut, `x` mm along X.
fn path(x: f64) -> Toolpath {
    let mut t = Toolpath::default();
    t.moves.push(Move::new(MoveKind::Rapid, p(x, 0.0, 10.0), p(x, 0.0, 5.0), None));
    t.moves.push(Move::new(MoveKind::Plunge, p(x, 0.0, 5.0), p(x, 0.0, 0.0), Some(300.0)));
    t.moves.push(Move::new(MoveKind::Feed, p(x, 0.0, 0.0), p(x + 20.0, 0.0, 0.0), Some(600.0)));
    t
}

/// Three operations; the middle path of `with_empty` has no moves.
fn three(with_empty: bool) -> Script {
    let (a, b, c, e) = (path(0.0), path(40.0), path(80.0), Toolpath::default());
    let mut ops: Vec<(&str, &Toolpath)> = vec![("Face1", &a), ("Contour2", &b)];
    if with_empty {
        ops.push(("Empty", &e));
    }
    ops.push(("Pocket3", &c));
    script(ops, &Traverse::default())
}

fn near(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 1e-9)
}

#[test]
fn each_operation_has_one_start_in_program_order() {
    let sc = three(false);
    let starts = sc.op_starts();
    assert_eq!(starts.iter().map(|&(op, _)| sc.ops[op].as_str()).collect::<Vec<_>>(), ["Face1", "Contour2", "Pocket3"]);
    assert_eq!(starts[0].1, 0.0, "the first operation starts at the start");
    assert!(starts[0].1 < starts[1].1 && starts[1].1 < starts[2].1, "the starts go forward: {starts:?}");
    for &(op, s) in &starts {
        assert_eq!(sc.at(s).op, Some(op), "the playhead at a start is in that operation");
    }
}

#[test]
fn next_from_inside_the_first_operation_goes_to_the_start_of_the_second() {
    let sc = three(false);
    let s = sc.op_starts();
    let inside = (s[0].1 + s[1].1) * 0.5;
    assert!(near(sc.next_op_start(inside), s[1].1));
    assert!(near(sc.next_op_start(0.0), s[1].1), "from the very start too");
    assert_eq!(sc.op_starting_at(s[1].1), Some("Contour2"));
    // The step lands on the start: a second "next" goes on.
    assert!(near(sc.next_op_start(s[1].1), s[2].1));
}

#[test]
fn previous_goes_to_the_current_start_then_to_the_one_before() {
    let sc = three(false);
    let s = sc.op_starts();
    let inside_second = (s[1].1 + s[2].1) * 0.5;
    assert!(near(sc.prev_op_start(inside_second), s[1].1), "first press: the start of the current operation");
    assert!(near(sc.prev_op_start(s[1].1), s[0].1), "second press: the start of the one before");
    assert!(near(sc.prev_op_start(sc.total), s[2].1), "at the end: the start of the last operation");
}

#[test]
fn the_first_and_the_last_operation_have_no_step_past_them() {
    let sc = three(false);
    let s = sc.op_starts();
    assert_eq!(sc.prev_op_start(0.0), None, "previous is off at the start of the first operation");
    assert_eq!(sc.next_op_start(s[2].1), None, "next is off in the last operation");
    assert_eq!(sc.next_op_start(sc.total), None, "next is off at the end");
    let empty = Script::default();
    assert_eq!((empty.next_op_start(0.0), empty.prev_op_start(0.0)), (None, None));
    assert!(empty.op_starts().is_empty());
}

#[test]
fn an_operation_with_no_moves_is_not_a_stop() {
    let sc = three(true);
    let names: Vec<&str> = sc.op_starts().iter().map(|&(op, _)| sc.ops[op].as_str()).collect();
    assert_eq!(names, ["Face1", "Contour2", "Pocket3"], "the empty path is no stop");
    let s = sc.op_starts();
    assert_eq!(sc.op_starting_at(sc.next_op_start(s[1].1).unwrap_or(f64::NAN)), Some("Pocket3"));
}

#[test]
fn a_non_finite_playhead_reads_as_the_end() {
    let sc = three(false);
    assert_eq!(sc.next_op_start(f64::NAN), None);
    assert!(near(sc.prev_op_start(f64::INFINITY), sc.op_starts()[2].1));
}

/// Review of PR #2763: the value of each start, from the segments and not
/// from `op_starts` itself. An operation starts where the last segment
/// of the operation before it ends (`cum[k]`), and a moment before that
/// the playhead is still in the operation before.
#[test]
fn each_start_is_the_end_of_the_last_segment_of_the_operation_before() {
    let sc = three(true);
    let s = sc.op_starts();
    assert_eq!(s.len(), 3);
    for w in 1..s.len() {
        let (before, now) = (s[w - 1].0, s[w].0);
        let k = sc.segs.iter().rposition(|g| g.op == before).expect("the operation before has segments");
        assert_eq!(sc.segs[k + 1].op, now, "segment {} opens {}", k + 1, sc.ops[now]);
        assert_eq!(s[w].1, sc.cum[k], "{} starts where {} ends", sc.ops[now], sc.ops[before]);
        assert_eq!(sc.at(s[w].1 - 1e-6).op, Some(before), "a moment before the start of {}", sc.ops[now]);
    }
}

/// The kept starts answer as the `Script` lookups do, at each start, a
/// moment either side of it, and the two ends.
#[test]
fn kept_starts_answer_as_the_script_lookups() {
    let sc = three(true);
    let kept = OpStarts::of(&sc);
    assert_eq!(kept.as_slice(), sc.op_starts().as_slice());
    let mut ts = vec![0.0, sc.total, f64::NAN, f64::INFINITY, -1.0];
    for &(_, s) in kept.as_slice() {
        ts.extend([s, s - 1e-6, s + 1e-6, s + 1e-12]);
    }
    for t in ts {
        assert_eq!(kept.next(t), sc.next_op_start(t), "next at {t}");
        assert_eq!(kept.prev(t), sc.prev_op_start(t), "prev at {t}");
    }
    for &(op, s) in kept.as_slice() {
        assert_eq!(kept.op_at(s), Some(op));
        assert_eq!(sc.op_starting_at(s), sc.op_name(op));
    }
    assert_eq!(kept.op_at(sc.total + 1.0), None, "no operation starts past the end");
    assert_eq!(OpStarts::of(&Script::default()), OpStarts::default());
}


/// A path that opens with a zero-length rapid at `(x, 0, z)` (#2901, PR
/// #2985: the opening rapid is seated on its own entry X/Y, so it takes
/// no time), then a plunge and one cut.
fn seated(x: f64, z: f64) -> Toolpath {
    let mut t = Toolpath::default();
    t.moves.push(Move::new(MoveKind::Rapid, p(x, 0.0, z), p(x, 0.0, z), None));
    t.moves.push(Move::new(MoveKind::Plunge, p(x, 0.0, z), p(x, 0.0, -1.0), Some(300.0)));
    t.moves.push(Move::new(MoveKind::Feed, p(x, 0.0, -1.0), p(x + 20.0, 0.0, -1.0), Some(600.0)));
    t
}

/// An operation whose first segment takes no time (a seated opening
/// rapid) is still a stop, and a step neither sticks on it nor skips the
/// operation. "Second" opens where "First" ends (no connector: its first
/// segment is the zero-time rapid); "Third" opens with a connector, then
/// the zero-time rapid. At each start the playhead is in the new
/// operation, all segments of the one before are complete, and "next"
/// from a start goes on.
#[test]
fn a_zero_time_opening_segment_is_a_stop_and_does_not_hold_the_step() {
    let (a, b, c) = (path(0.0), seated(20.0, 0.0), seated(80.0, 10.0));
    let sc = script(vec![("First", &a), ("Second", &b), ("Third", &c)], &Traverse::default());
    let s = sc.op_starts();
    assert_eq!(s.iter().map(|&(op, _)| sc.ops[op].as_str()).collect::<Vec<_>>(), ["First", "Second", "Third"]);
    let b0 = sc.segs.iter().position(|g| g.op == s[1].0).expect("Second has segments");
    assert_eq!(sc.segs[b0].secs, 0.0, "Second opens with a zero-time segment");
    assert!(sc.segs.iter().any(|g| g.op == s[2].0 && g.secs == 0.0), "Third has its zero-time opening rapid");
    for w in 1..s.len() {
        let (before, now) = (s[w - 1].0, s[w].0);
        let k = sc.segs.iter().rposition(|g| g.op == before).expect("the operation before has segments");
        assert_eq!(s[w].1, sc.cum[k], "{} starts where {} ends", sc.ops[now], sc.ops[before]);
        assert_eq!(sc.at(s[w].1).op, Some(now), "the playhead at the start of {} is in it", sc.ops[now]);
        assert!(near(sc.prev_op_start(s[w].1), s[w - 1].1), "previous from the start of {}", sc.ops[now]);
    }
    assert!(near(sc.next_op_start(0.0), s[1].1), "next from the start stops at Second");
    assert!(near(sc.next_op_start(s[1].1), s[2].1), "next from Second goes on to Third");
    assert_eq!(sc.next_op_start(s[2].1), None, "Third is the last operation");
}
