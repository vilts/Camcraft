//! The threading infeed plan (issue #416): radial, flank and
//! alternating infeed; constant-depth and constant-area (first depth
//! times sqrt(n), clamped at the minimum depth) schedules; the finish
//! allowance and spring passes. Pinned here: the default plan lays the
//! very path the pre-#416 kernel laid; the pass depths of the sqrt(n)
//! schedule; the flank station shift on the lattice; the alternating
//! side flip; the spring-pass count; the verdict silent on the modeled
//! thread for every method (and convicting a pass shifted past the
//! insert's half-angle - it reads the laid geometry); the refusals by
//! name; the repeat of each roughing depth (issue #418) - the pass
//! count, the depths, and every repeat the very same moves.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (infeed plan,
//! thread kernel's pass layout and attach_cycle, thread_warnings).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, ThreadHand, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::infeed::{DepthSchedule, InfeedMethod, ThreadInfeed};
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{thread_in, thread_planned_in, thread_warnings, ThreadParams};

const TAU: f64 = std::f64::consts::TAU;
const TOL: f64 = 0.05;
const MAJOR: f64 = 6.0;
const DEPTH: f64 = 1.2;
const ROOT: f64 = MAJOR - DEPTH;

/// The motion threading suites' bar: a 60-degree V every 2.0 from z 8
/// to z 14 on a wall of r 6, 16 ways round.
fn threaded() -> Vec<Triangle> {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let mut m = vec![(0.0, -16.0), (MAJOR, -16.0), (MAJOR, 8.0 - half)];
    let mut c = 8.0;
    while c <= 14.0 + 1e-9 {
        m.push((ROOT, c));
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

fn width() -> f64 {
    2.0 * DEPTH * (30.0_f64).to_radians().tan()
}

fn th() -> ThreadParams {
    ThreadParams {
        tolerance: TOL,
        clearance_z: 24.0,
        top_z: 14.3,
        bottom_z: 7.7,
        pitch: 2.0,
        depth: DEPTH,
        infeed: 0.4,
        feed: 0.0,
        plunge_feed: 40.0,
        insert_width: width(),
        form_angle: 60.0,
        hand: ThreadHand::Right,
    }
}

fn plan(method: InfeedMethod, schedule: DepthSchedule) -> ThreadInfeed {
    ThreadInfeed { method, schedule, ..ThreadInfeed::default() }
}

fn run(p: &ThreadParams, plan: &ThreadInfeed) -> Toolpath {
    thread_planned_in(&threaded(), p, plan, None).expect("the thread cuts")
}

/// Each pass's (station, tip radius) at its plunge: the radial feed
/// that opens every pass.
fn plunges(path: &Toolpath) -> Vec<(f64, f64)> {
    let f = Frame::about(&threaded()).expect("frame");
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.arc.is_none())
        .map(|m| f.sr(m.to))
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn the_default_plan_is_the_pre_416_ladder() {
    let p = th();
    let old = thread_in(&threaded(), &p, None).expect("cuts");
    let new = run(&p, &ThreadInfeed::default());
    assert_eq!(old.moves, new.moves, "the default plan moved a pass");
    assert_eq!(old.threads, new.threads, "the default plan changed the record");
    let depths: Vec<f64> = plunges(&new).iter().map(|&(_, r)| MAJOR - r).collect();
    assert_eq!(depths.len(), 3, "{depths:?}");
    for (d, want) in depths.iter().zip([0.4, 0.8, 1.2]) {
        assert!((d - want).abs() < 1e-9, "{depths:?}");
    }
}

#[test]
fn constant_area_passes_follow_sqrt_n_clamped_at_the_minimum() {
    // Pure plan: crest 6, root 4.8 (1.2 high), first depth 0.4,
    // minimum 0.15. sqrt(n) steps: 0.4, 0.566 (+0.166), 0.693 (+0.127
    // -> clamped 0.15 => 0.716), then +0.15 each: 0.866, 1.016, 1.166,
    // then the root 1.2.
    let pl = ThreadInfeed { min_depth: 0.15, ..plan(InfeedMethod::Radial, DepthSchedule::ConstantArea) };
    let passes = pl.passes(MAJOR, ROOT, 0.4).expect("plan");
    let cum: Vec<f64> = passes.iter().map(|q| MAJOR - q.tip).collect();
    let mut want = vec![0.4, 0.4 * 2f64.sqrt()];
    let mut c = want[1];
    let mut n = 3.0_f64;
    loop {
        let next = (0.4 * n.sqrt()).max(c + 0.15);
        if next >= DEPTH {
            want.push(DEPTH);
            break;
        }
        want.push(next);
        c = next;
        n += 1.0;
    }
    assert_eq!(cum.len(), want.len(), "{cum:?} vs {want:?}");
    for (a, b) in cum.iter().zip(&want) {
        assert!((a - b).abs() < 1e-9, "{cum:?} vs {want:?}");
    }
    assert!(close(cum[2], 0.4 * 2f64.sqrt() + 0.15), "the third pass is clamped: {cum:?}");
    for w in cum.windows(2) {
        assert!(w[1] - w[0] >= 0.15 - 1e-9 || close(w[1], DEPTH), "a step under the minimum: {cum:?}");
    }
    assert!(passes.iter().all(|q| q.shift == 0.0), "radial: no shift");
    // The kernel lays exactly these depths.
    let path = run(&th(), &ThreadInfeed { min_depth: 0.15, ..pl });
    let laid: Vec<f64> = plunges(&path).iter().map(|&(_, r)| MAJOR - r).collect();
    assert_eq!(laid.len(), cum.len());
    for (a, b) in laid.iter().zip(&cum) {
        assert!((a - b).abs() < 1e-6, "{laid:?} vs {cum:?}");
    }
    let t = path.threads[0];
    assert!(close(t.first_depth, 0.4) && close(t.min_depth, 0.15), "{t:?}");
}

#[test]
fn a_finish_allowance_and_spring_passes_end_the_ladder() {
    let pl = ThreadInfeed {
        finish_allowance: 0.1,
        spring_passes: 2,
        ..ThreadInfeed::default()
    };
    let path = run(&th(), &pl);
    let depths: Vec<f64> = plunges(&path).iter().map(|&(_, r)| MAJOR - r).collect();
    // 0.4, 0.8, the floor 1.1, the finishing pass 1.2, two spring passes.
    let want = [0.4, 0.8, 1.1, 1.2, 1.2, 1.2];
    assert_eq!(depths.len(), want.len(), "{depths:?}");
    for (a, b) in depths.iter().zip(want) {
        assert!((a - b).abs() < 1e-6, "{depths:?}");
    }
    let at_root = depths.iter().filter(|d| (*d - DEPTH).abs() < 1e-6).count();
    assert_eq!(at_root, 1 + 2, "the finish pass and its two spring passes");
    let t = path.threads[0];
    assert_eq!(t.spring_passes, 2);
    assert!(close(t.finish_allowance, 0.1), "{t:?}");
    let warns = thread_warnings(&threaded(), &path, width(), 60.0, TOL);
    assert!(warns.is_empty(), "{warns:?}");
}

#[test]
fn flank_infeed_shifts_each_pass_along_its_flank() {
    let ang: f64 = 27.5;
    let tan = ang.to_radians().tan();
    let pl = ThreadInfeed { flank_angle: ang, ..plan(InfeedMethod::Flank, DepthSchedule::ConstantDepth) };
    let path = run(&th(), &pl);
    let ps = plunges(&path);
    assert_eq!(ps.len(), 3);
    // Every pass runs toward the chuck (-Z, #629) from the top lattice
    // centre (14): the last pass stands on it; each shallower pass
    // stands (its height above the root) * tan(angle) behind it - toward
    // the free end, +Z.
    for &(s, r) in &ps {
        let want = 14.0 + (r - ROOT) * tan;
        assert!((s - want).abs() < 1e-9, "pass at r {r}: s {s}, want {want}");
    }
    assert!((ps[2].0 - 14.0).abs() < 1e-9, "the finishing pass is on the lattice");
    // Every arc of a pass rides the same lattice offset: the helix
    // keeps its lead, only its start moved.
    let f = Frame::about(&threaded()).expect("frame");
    let t = path.threads[0];
    assert!(close(t.infeed_angle, 2.0 * ang), "{t:?}");
    let (s0, _) = f.sr(t.start);
    assert!((s0 - (14.0 + DEPTH * tan)).abs() < 1e-9, "the cycle starts on the crest's flank station: {s0}");
    assert_eq!(path.moves[t.first].to, t.start);
    let warns = thread_warnings(&threaded(), &path, width(), 60.0, TOL);
    assert!(warns.is_empty(), "flank infeed reported: {warns:?}");
    // At the full half-angle the trailing flank rides the finished
    // flank exactly - still silent.
    let pl = ThreadInfeed { flank_angle: 30.0, ..pl };
    let path = run(&th(), &pl);
    let warns = thread_warnings(&threaded(), &path, width(), 60.0, TOL);
    assert!(warns.is_empty(), "30-degree flank infeed reported: {warns:?}");
}

#[test]
fn alternating_infeed_flips_the_side_every_pass() {
    let ang: f64 = 29.5;
    let tan = ang.to_radians().tan();
    let pl = ThreadInfeed {
        flank_angle: ang,
        min_depth: 0.1,
        ..plan(InfeedMethod::Alternating, DepthSchedule::ConstantArea)
    };
    let path = run(&th(), &pl);
    let ps = plunges(&path);
    assert!(ps.len() >= 4, "{ps:?}");
    for (j, &(s, r)) in ps.iter().enumerate() {
        let back = (r - ROOT) * tan;
        // Toward the chuck from 14 (#629): behind the travel is +Z.
        let want = if j % 2 == 0 { 14.0 + back } else { 14.0 - back };
        assert!((s - want).abs() < 1e-9, "pass {j}: s {s}, want {want}");
    }
    let t = path.threads.first().copied().expect("a record, flagged");
    assert!(t.alternating && (t.infeed_angle - 2.0 * ang).abs() < 1e-9, "{t:?}");
    let warns = thread_warnings(&threaded(), &path, width(), 60.0, TOL);
    assert!(warns.is_empty(), "alternating infeed reported: {warns:?}");
}

#[test]
fn every_method_and_schedule_leaves_the_judge_silent() {
    for method in [InfeedMethod::Radial, InfeedMethod::Flank, InfeedMethod::Alternating] {
        for schedule in [DepthSchedule::ConstantDepth, DepthSchedule::ConstantArea] {
            let pl = ThreadInfeed { min_depth: 0.1, finish_allowance: 0.05, spring_passes: 1, ..plan(method, schedule) };
            let path = run(&th(), &pl);
            let warns = thread_warnings(&threaded(), &path, width(), 60.0, TOL);
            assert!(warns.is_empty(), "{method:?} {schedule:?}: {warns:?}");
        }
    }
}

#[test]
fn the_verdict_reads_the_shifted_pass() {
    // A first pass carried 0.8 along the axis (past what the 30-degree
    // half-angle allows 0.8 above the root, 0.46) puts its flank in
    // the finished thread's: the judge must see the moved V, not the
    // lattice it assumes.
    let mut path = run(&th(), &ThreadInfeed::default());
    let plunge = path.moves.iter().position(|m| m.kind == MoveKind::Feed).expect("plunge");
    // The pass ends where it reaches the bottom lattice point and
    // retracts (toward the chuck since #629): the first rapid after it.
    let first_retract = path
        .moves
        .iter()
        .enumerate()
        .skip(plunge)
        .find(|(_, m)| m.kind == MoveKind::Rapid)
        .map(|(i, _)| i)
        .expect("the first pass's retract");
    for m in &mut path.moves[plunge..first_retract] {
        m.from.z += 0.8;
        m.to.z += 0.8;
    }
    let warns = thread_warnings(&threaded(), &path, width(), 60.0, TOL);
    assert!(!warns.is_empty(), "a pass moved into the flank judged silent");
}

#[test]
fn plans_that_cannot_cut_refuse_by_name() {
    let p = th();
    let cases: [(ThreadInfeed, &str); 6] = [
        (ThreadInfeed { flank_angle: 35.0, ..plan(InfeedMethod::Flank, DepthSchedule::ConstantDepth) }, "half-angle"),
        (ThreadInfeed { flank_angle: f64::NAN, ..plan(InfeedMethod::Alternating, DepthSchedule::ConstantDepth) }, "not a flank"),
        (ThreadInfeed { min_depth: 0.5, ..plan(InfeedMethod::Radial, DepthSchedule::ConstantArea) }, "first-pass depth"),
        (ThreadInfeed { min_depth: 0.0, ..plan(InfeedMethod::Radial, DepthSchedule::ConstantArea) }, "minimum depth"),
        (ThreadInfeed { finish_allowance: 1.2, ..ThreadInfeed::default() }, "finish allowance"),
        (ThreadInfeed { spring_passes: 99, ..ThreadInfeed::default() }, "spring passes"),
    ];
    for (pl, why) in cases {
        let msg = pl.refusal(p.depth, p.infeed, p.form_angle).expect("refused");
        assert!(msg.contains(why), "{why}: {msg}");
        let err = thread_planned_in(&threaded(), &p, &pl, None).expect_err("the kernel refuses too");
        assert!(err.to_string().contains(why), "{why}: {err}");
    }
    // A minimum deeper than the first depth is fine for constant depth
    // (it is not read there), and a flank angle is not read by radial.
    let ok = ThreadInfeed { min_depth: 0.5, flank_angle: 80.0, ..ThreadInfeed::default() };
    assert!(ok.refusal(p.depth, p.infeed, p.form_angle).is_none());
    let nan = ThreadInfeed { finish_allowance: f64::INFINITY, ..ThreadInfeed::default() };
    assert!(nan.refusal(p.depth, p.infeed, p.form_angle).is_some() && !nan.is_sane());
}

/// The move span of each pass: from its plunge up to (not including)
/// the next pass's approach rapid.
fn pass_spans(path: &Toolpath) -> Vec<&[camcraft_motion::cl::Move]> {
    let at: Vec<usize> = path
        .moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind == MoveKind::Feed && m.arc.is_none())
        .map(|(i, _)| i)
        .collect();
    let len = match at.as_slice() {
        [a, b, ..] => b - a - 1,
        _ => 0,
    };
    at.iter().filter_map(|&i| path.moves.get(i..i + len)).collect()
}

#[test]
fn repeat_each_depth_cuts_every_roughing_depth_twice() {
    // 0.4, 0.8, the floor 1.1 each twice; the finishing pass 1.2 and
    // the one spring pass are not repeated.
    let pl = ThreadInfeed { finish_allowance: 0.1, spring_passes: 1, repeat_each_depth: true, ..ThreadInfeed::default() };
    let path = run(&th(), &pl);
    let depths: Vec<f64> = plunges(&path).iter().map(|&(_, r)| MAJOR - r).collect();
    let want = [0.4, 0.4, 0.8, 0.8, 1.1, 1.1, 1.2, 1.2];
    assert_eq!(depths.len(), want.len(), "{depths:?}");
    for (d, w) in depths.iter().zip(want) {
        assert!((d - w).abs() < 1e-9, "{depths:?}");
    }
    assert!(depths.iter().all(|&d| d <= DEPTH + 1e-9), "a repeat went deeper: {depths:?}");
    let t = path.threads.first().copied().expect("the record, flagged");
    assert!(t.repeat_each_depth, "{t:?}");
    // No allowance: the last rung is the root, the finishing pass, cut
    // once - 0.4, 0.4, 0.8, 0.8, 1.2.
    let pl = ThreadInfeed { repeat_each_depth: true, ..ThreadInfeed::default() };
    let depths: Vec<f64> = plunges(&run(&th(), &pl)).iter().map(|&(_, r)| MAJOR - r).collect();
    assert_eq!(depths.len(), 5, "{depths:?}");
    // Off is the plan without the flag, move for move.
    let off = run(&th(), &ThreadInfeed::default());
    assert!(!off.threads[0].repeat_each_depth);
    assert_eq!(off.moves, thread_in(&threaded(), &th(), None).expect("cuts").moves);
}

#[test]
fn a_repeat_is_the_same_pass_on_every_method_and_the_judge_stays_silent() {
    for method in [InfeedMethod::Radial, InfeedMethod::Flank, InfeedMethod::Alternating] {
        for schedule in [DepthSchedule::ConstantDepth, DepthSchedule::ConstantArea] {
            let base = ThreadInfeed { min_depth: 0.1, finish_allowance: 0.05, spring_passes: 1, ..plan(method, schedule) };
            let once = pass_spans(&run(&th(), &base)).len();
            let pl = ThreadInfeed { repeat_each_depth: true, ..base };
            let path = run(&th(), &pl);
            let spans = pass_spans(&path);
            // Every roughing pass doubles; the finishing and spring
            // passes (2) do not.
            assert_eq!(spans.len(), 2 * (once - 2) + 2, "{method:?} {schedule:?}");
            for j in (0..2 * (once - 2)).step_by(2) {
                // Same plunge, same arcs, same retract: same station,
                // same depth, same helix phase - never deeper, never moved.
                assert_eq!(spans[j], spans[j + 1], "{method:?} {schedule:?}: repeat {j} differs");
            }
            let warns = thread_warnings(&threaded(), &path, width(), 60.0, TOL);
            assert!(warns.is_empty(), "{method:?} {schedule:?}: {warns:?}");
        }
    }
}
