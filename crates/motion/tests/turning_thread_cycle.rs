//! The thread kernel's cycle record (issue #34): beside its explicit
//! passes the thread path carries one `ThreadCycle` stating the same
//! thread - start on the first centre at the clearance radius, end at
//! the root on the last centre, the ladder's rung as first and minimum
//! depth, radial infeed - and the passes it spans are exactly the
//! passes the sim and verify read.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (thread kernel's
//! attach_cycle), camcraft_cl::cl::ThreadCycle.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, ThreadHand, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::{thread, thread_hand_warnings, ThreadParams};
use camcraft_motion::verify::Warning;

const TAU: f64 = std::f64::consts::TAU;
const MAJOR: f64 = 6.0;
const DEPTH: f64 = 1.2;

/// The motion threading suite's bar: a 60-degree V every 2.0 from
/// z 8 to z 14 on a wall of r 6, 16 ways round.
fn threaded() -> Vec<Triangle> {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let mut m = vec![(0.0, -16.0), (MAJOR, -16.0), (MAJOR, 8.0 - half)];
    let mut c = 8.0;
    while c <= 14.0 + 1e-9 {
        m.push((MAJOR - DEPTH, c));
        m.push((MAJOR, c + half));
        if c + 2.0 <= 14.0 + 1e-9 {
            m.push((MAJOR, c + 2.0 - half));
        }
        c += 2.0;
    }
    m.extend([(MAJOR, 14.0 + half), (MAJOR, 40.0), (0.0, 40.0)]);
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in m.windows(2) {
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

fn params(hand: ThreadHand) -> ThreadParams {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    ThreadParams {
        tolerance: 0.05,
        clearance_z: 24.0,
        top_z: 14.3,
        bottom_z: 7.7,
        pitch: 2.0,
        depth: DEPTH,
        infeed: 0.4,
        feed: 0.0,
        plunge_feed: 40.0,
        insert_width: 2.0 * half,
        form_angle: 60.0,
        hand,
    }
}

#[test]
fn the_thread_path_carries_its_cycle_record() {
    let p = params(ThreadHand::Right);
    let path = thread(&threaded(), &p).expect("the thread cuts");
    assert_eq!(path.threads.len(), 1, "one record: {:?}", path.notes);
    let t = path.threads[0];
    assert!(t.is_sane(&path.moves));
    let rho = |q: Point3| (q.x * q.x + q.y * q.y).sqrt();
    // #629: the passes run toward the chuck - start on the TOP centre,
    // end on the bottom one.
    assert!((t.start.z - 14.0).abs() < 1e-9 && (t.end.z - 8.0).abs() < 1e-9, "{t:?}");
    assert!(t.toward_minus_z() && t.hand == ThreadHand::Right && !t.reverse_spindle(), "{t:?}");
    assert!((rho(t.end) - (MAJOR - DEPTH)).abs() < 1e-9, "the end stands at the root: {t:?}");
    assert!(rho(t.start) > MAJOR, "the start clears the crest: {t:?}");
    assert!((t.major_diameter - 2.0 * MAJOR).abs() < 1e-9 && (t.minor_diameter - 2.0 * (MAJOR - DEPTH)).abs() < 1e-9);
    assert!((t.lead - 2.0).abs() < 1e-12 && (t.height - DEPTH).abs() < 1e-9);
    assert!((t.first_depth - 0.4).abs() < 1e-9 && (t.min_depth - 0.4).abs() < 1e-9);
    // #417: the finishing pass is the root pass, 0.4 below the 0.8 rung
    // before it - the depth a control's cycle finishes with.
    assert!((t.finish_allowance - 0.4).abs() < 1e-9, "{t:?}");
    assert_eq!((t.infeed_angle, t.spring_passes, t.pull_out, t.alternating), (0.0, 0, 0.0, false));
    // The span is every pass: the moves before it and after it are the
    // approach and the out-rapid, and every arc lies inside it.
    assert!(path.moves[..t.first].iter().all(|m| m.kind == MoveKind::Rapid));
    assert!(path.moves[t.last + 1..].iter().all(|m| m.kind == MoveKind::Rapid));
    let arcs_in = path.moves[t.first..=t.last].iter().filter(|m| m.arc.is_some()).count();
    assert_eq!(arcs_in, path.arc_count(), "every half-turn inside the span");
    assert_eq!(path.moves[t.last].kind, MoveKind::Rapid, "the span ends on the retract");
}

/// The (ccw, dz) of every half-turn in the path.
fn turns(path: &Toolpath) -> Vec<(bool, f64)> {
    path.moves.iter().filter_map(|m| m.arc.map(|a| (a.ccw, m.to.z - m.from.z))).collect()
}

#[test]
fn every_pass_runs_toward_the_chuck_and_the_helix_winds_the_asked_hand() {
    // #629: a right-hand helix advances +Z turning counter-clockwise
    // seen from +Z, or -Z turning clockwise. Toward the chuck (-Z)
    // a right-hand thread's half-turns are therefore CLOCKWISE (the
    // tool's motion about a part turning M03), a left-hand one's
    // counter-clockwise (M04). Before the fix every pass rose (+Z) and
    // turned counter-clockwise whatever was asked.
    for (hand, ccw) in [(ThreadHand::Right, false), (ThreadHand::Left, true)] {
        let path = thread(&threaded(), &params(hand)).expect("the thread cuts");
        let t = turns(&path);
        assert!(!t.is_empty());
        assert!(t.iter().all(|&(c, dz)| c == ccw && dz < 0.0), "{hand:?}: every half-turn falls, ccw={ccw}: {t:?}");
        assert!(path.moves.iter().filter(|m| m.arc.is_some()).all(|m| ThreadHand::of_move(m) == Some(hand)));
        let rec = path.threads[0];
        assert_eq!(rec.hand, hand);
        assert!(rec.is_sane(&path.moves));
        assert_eq!(rec.reverse_spindle(), hand == ThreadHand::Left, "left-hand toward the chuck needs M04");
        assert!(thread_hand_warnings(&path, hand).is_empty(), "{hand:?}: the hand verdict is silent on its own path");
        // The other hand is convicted at the first half-turn, and a
        // record claiming the other hand is not sane.
        let other = if hand == ThreadHand::Right { ThreadHand::Left } else { ThreadHand::Right };
        let first_arc = path.moves.iter().position(|m| m.arc.is_some()).expect("arcs");
        assert_eq!(thread_hand_warnings(&path, other), vec![Warning::Gouge { at: first_arc }]);
        let mut lie = rec;
        lie.hand = other;
        assert!(!lie.is_sane(&path.moves), "a record whose hand its passes do not lay is refused");
    }
}

#[test]
fn the_hand_of_a_turn_is_the_screw_law() {
    assert_eq!(ThreadHand::of_turn(true, 1.0), Some(ThreadHand::Right));
    assert_eq!(ThreadHand::of_turn(false, -1.0), Some(ThreadHand::Right));
    assert_eq!(ThreadHand::of_turn(false, 1.0), Some(ThreadHand::Left));
    assert_eq!(ThreadHand::of_turn(true, -1.0), Some(ThreadHand::Left));
    assert_eq!(ThreadHand::of_turn(true, 0.0), None, "no travel, no helix");
    assert_eq!(ThreadHand::of_turn(true, f64::NAN), None);
    for h in ThreadHand::ALL {
        assert_eq!(ThreadHand::from_word(h.word()), Some(h));
        assert_eq!(ThreadHand::of_turn(h.ccw_for(true), -1.0), Some(h));
        assert_eq!(ThreadHand::of_turn(h.ccw_for(false), 1.0), Some(h));
    }
}
