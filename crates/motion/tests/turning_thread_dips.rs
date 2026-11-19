//! In-pass chip-break dips (issue #419): the roughing passes lift
//! radially and come back inside one synchronized chain. Pinned here:
//! a plan with dips off lays the very path it laid before; no dip ever
//! stands deeper than its pass, or higher than its dip height; the
//! phase (station mod lead) of every half-turn end is the plain pass's;
//! every dip segment stays under 45 degrees; no dip stands in a chain's
//! first or last lead, on the finishing pass, or on a repeat of a depth;
//! the verdict is silent on the laid dips and convicts a path that
//! breaks a law (a dip in the first lead, on the finishing pass, deeper
//! than its pass, or a moved station); the refusals by name; the cycle
//! record marks the dips; dipped passes keep #629's chuck-ward travel
//! and wind the asked hand.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (thread_dips: the
//! dip layout and its verdict; the thread kernel's dipped half-turns;
//! attach_cycle's chip_break_dips; thread_warnings).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, ThreadHand, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::infeed::ThreadInfeed;
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{thread_hand_warnings, thread_planned_in, thread_warnings, ThreadDips, ThreadParams};

const TAU: f64 = std::f64::consts::TAU;
const TOL: f64 = 0.05;
const MAJOR: f64 = 6.0;
const DEPTH: f64 = 1.2;
const ROOT: f64 = MAJOR - DEPTH;
const PITCH: f64 = 2.0;

/// A 60-degree V every 2.0 from z 8 to `end` on a wall of r 6, 16 ways
/// round (the motion threading suites' bar, lengthened).
fn threaded(end: f64) -> Vec<Triangle> {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let mut m = vec![(0.0, -60.0), (MAJOR, -60.0), (MAJOR, 8.0 - half)];
    let mut c = 8.0;
    while c <= end + 1e-9 {
        m.push((ROOT, c));
        m.push((MAJOR, c + half));
        if c + PITCH <= end + 1e-9 {
            m.push((MAJOR, c + PITCH - half));
        }
        c += PITCH;
    }
    m.extend([(MAJOR, end + half), (MAJOR, end + 60.0), (0.0, end + 60.0)]);
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

/// The long bar: eight leads, z 8 to 24.
fn long_bar() -> Vec<Triangle> {
    threaded(24.0)
}

fn width() -> f64 {
    2.0 * DEPTH * (30.0_f64).to_radians().tan()
}

fn th(end: f64) -> ThreadParams {
    ThreadParams {
        tolerance: TOL,
        clearance_z: end + 10.0,
        top_z: end + 0.3,
        bottom_z: 7.7,
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        insert_width: width(),
        form_angle: 60.0,
        hand: ThreadHand::Right,
    }
}

/// Dips every two leads, 0.3 high, the shortest the ramps allow.
fn dips() -> ThreadDips {
    ThreadDips { interval: 4.0, height: 0.3, length: 0.0 }
}

fn plan_with(d: ThreadDips) -> ThreadInfeed {
    ThreadInfeed { dips: d, ..ThreadInfeed::default() }
}

fn run(plan: &ThreadInfeed) -> Toolpath {
    thread_planned_in(&long_bar(), &th(24.0), plan, None).expect("the thread cuts")
}

fn frame() -> Frame {
    Frame::about(&long_bar()).expect("frame")
}

/// Every chain: the plunge's tip radius and its half-turns as (move
/// index, start (station, radius), end (station, radius)).
type Turn = (usize, (f64, f64), (f64, f64));
fn chains(path: &Toolpath) -> Vec<(f64, Vec<Turn>)> {
    let f = frame();
    let mut out: Vec<(f64, Vec<Turn>)> = Vec::new();
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        if m.arc.is_none() {
            out.push((f.sr(m.to).1, Vec::new()));
        } else if let Some(c) = out.last_mut() {
            c.1.push((i, f.sr(m.from), f.sr(m.to)));
        }
    }
    out
}

fn is_dip(t: &Turn) -> bool {
    (t.2 .1 - t.1 .1).abs() > 1e-9
}

#[test]
fn dips_off_lay_the_plain_path_bit_for_bit() {
    let plain = run(&ThreadInfeed::default());
    // Off is interval 0, whatever the other numbers say.
    let off = run(&plan_with(ThreadDips { interval: 0.0, height: 0.2, length: 3.0 }));
    assert_eq!(plain.moves, off.moves);
    assert_eq!(plain.threads, off.threads);
    assert!(!plain.threads.iter().any(|t| t.chip_break_dips));
}

#[test]
fn a_dip_never_stands_deeper_than_its_pass() {
    let path = run(&plan_with(dips()));
    let cs = chains(&path);
    assert_eq!(cs.len(), 3, "three passes: 0.4, 0.8, 1.2");
    let mut lifted = 0;
    for (tip, turns) in &cs {
        for t in turns {
            for r in [t.1 .1, t.2 .1] {
                assert!(r >= tip - 1e-9, "a half-turn at {r} under its pass's tip {tip}");
                assert!(r <= tip + 0.3 + 1e-9, "a half-turn at {r} lifted past the dip height over {tip}");
            }
            if is_dip(t) {
                lifted += 1;
            }
        }
    }
    // Two roughing passes, three dips each, an out- and an in-ramp per dip.
    assert_eq!(lifted, 2 * 3 * 2, "{cs:?}");
}

#[test]
fn the_phase_is_unchanged_across_the_chain() {
    let plain = run(&ThreadInfeed::default());
    let dipped = run(&plan_with(dips()));
    assert_eq!(plain.moves.len(), dipped.moves.len(), "a dip adds no block");
    let f = frame();
    for (a, b) in plain.moves.iter().zip(&dipped.moves) {
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.arc.is_some(), b.arc.is_some());
        let (sa, sb) = (f.sr(a.to).0, f.sr(b.to).0);
        assert!((sa - sb).abs() < 1e-9, "a dip moved a station: {sa} vs {sb}");
        if let (Some(x), Some(y)) = (a.arc, b.arc) {
            assert_eq!(x.center, y.center, "the half-turns turn about the same axis point");
            assert_eq!(x.ccw, y.ccw);
        }
    }
    for (_, turns) in chains(&dipped) {
        let Some(first) = turns.first() else { continue };
        let s0 = first.1 .0;
        for (k, t) in turns.iter().enumerate() {
            // Every pass travels toward the chuck, -Z (#629).
            let along = s0 - t.2 .0;
            let want = (k + 1) as f64 * PITCH * 0.5;
            assert!((along - want).abs() < 1e-9, "half-turn {k} ends {along} along the chain, not {want}");
            let phase = along.rem_euclid(PITCH);
            assert!(phase.abs() < 1e-9 || (phase - PITCH * 0.5).abs() < 1e-9, "phase {phase}");
        }
    }
}

#[test]
fn dips_stay_under_45_degrees_outside_the_first_and_last_lead_on_roughing_passes() {
    let path = run(&plan_with(dips()));
    let cs = chains(&path);
    for (tip, turns) in &cs {
        let n = turns.len();
        for (k, t) in turns.iter().enumerate() {
            if !is_dip(t) {
                continue;
            }
            let k = k + 1;
            assert!(k > 2 && k + 2 <= n, "a dip at half-turn {k} of {n} - inside the first or last lead");
            let (dr, ds) = ((t.2 .1 - t.1 .1).abs(), (t.2 .0 - t.1 .0).abs());
            assert!(2.0 * dr < ds, "a dip segment's diameter change {} not under its axial travel {ds}", 2.0 * dr);
            assert!(*tip > ROOT + 1e-6, "a dip on the finishing pass at {tip}");
        }
    }
    // The finishing pass at the root cuts whole.
    let root = cs.iter().find(|(t, _)| (t - ROOT).abs() < 1e-6).expect("a pass at the root");
    assert!(!root.1.iter().any(is_dip));
    // A high dip spreads each ramp over two half-turns, still under 45:
    // 0.6 high (a 1.2 diameter change) against 1.0 of travel each.
    let p = ThreadParams { infeed: 0.6, ..th(24.0) };
    let plan = plan_with(ThreadDips { interval: 8.0, height: 0.6, length: 0.0 });
    let high = thread_planned_in(&long_bar(), &p, &plan, None).expect("the thread cuts");
    let ramps: Vec<f64> = chains(&high)
        .iter()
        .flat_map(|(_, turns)| turns.iter().filter(|t| is_dip(t)).map(|t| (t.2 .1 - t.1 .1).abs()).collect::<Vec<_>>())
        .collect();
    assert_eq!(ramps.len(), 4, "one dip, two half-turns a ramp: {ramps:?}");
    assert!(ramps.iter().all(|d| (d - 0.3).abs() < 1e-9), "{ramps:?}");
    let w = thread_warnings(&long_bar(), &high, width(), 60.0, TOL);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_repeated_depth_cuts_whole() {
    let plan = ThreadInfeed { repeat_each_depth: true, dips: dips(), ..ThreadInfeed::default() };
    let path = run(&plan);
    let cs = chains(&path);
    assert_eq!(cs.len(), 5, "0.4, 0.4, 0.8, 0.8, 1.2");
    let dipped: Vec<bool> = cs.iter().map(|(_, t)| t.iter().any(is_dip)).collect();
    assert_eq!(dipped, [true, false, true, false, false], "the first cut of each depth dips, its repeat does not");
}

#[test]
fn the_verdict_is_silent_on_laid_dips() {
    let path = run(&plan_with(dips()));
    let w = thread_warnings(&long_bar(), &path, width(), 60.0, TOL);
    assert!(w.is_empty(), "{w:?}");
    assert!(path.threads.iter().all(|t| t.chip_break_dips), "the record marks the dips");
}

#[test]
fn dips_keep_the_hand_and_the_chuck_ward_travel() {
    // #629: every pass travels toward the chuck, and every half-turn,
    // a dip's ramps included, winds the asked hand.
    for hand in [ThreadHand::Right, ThreadHand::Left] {
        let p = ThreadParams { hand, ..th(24.0) };
        let path = thread_planned_in(&long_bar(), &p, &plan_with(dips()), None).expect("the thread cuts");
        assert!(thread_hand_warnings(&path, hand).is_empty(), "{hand:?}: a half-turn of the other hand");
        assert!(thread_warnings(&long_bar(), &path, width(), 60.0, TOL).is_empty(), "{hand:?}");
        for (_, turns) in chains(&path) {
            for t in &turns {
                assert!(t.2 .0 < t.1 .0, "{hand:?}: a half-turn travelling away from the chuck");
            }
        }
        assert!(path.threads.iter().all(|t| t.hand == hand && t.chip_break_dips && t.toward_minus_z()));
    }
}

/// Rewrite the end radius of half-turn `k` (1-based) of chain `c` and
/// the start of the half-turn after it: the shape a buggy layout would
/// lay.
fn bend(path: &mut Toolpath, c: usize, k: usize, radius: f64, station_shift: f64) -> usize {
    let f = frame();
    let cs = chains(path);
    let at = cs[c].1[k - 1].0;
    let to = path.moves[at].to;
    let (s, _) = f.sr(to);
    let (o, m) = (f.point(0.0, s), f.point(1.0, s));
    let even = (to.x - o.x) * (m.x - o.x) + (to.y - o.y) * (m.y - o.y) + (to.z - o.z) * (m.z - o.z) > 0.0;
    let s = s + station_shift;
    let p = if even { f.point(radius, s) } else { antipode(&f, radius, s) };
    path.moves[at].to = p;
    if let Some(next) = path.moves.get_mut(at + 1) {
        next.from = p;
    }
    at
}

fn antipode(f: &Frame, r: f64, s: f64) -> Point3 {
    let a = f.point(r, s);
    let o = f.point(0.0, s);
    Point3::new(2.0 * o.x - a.x, 2.0 * o.y - a.y, 2.0 * o.z - a.z)
}

fn convicted(path: &Toolpath, at: usize) -> bool {
    thread_warnings(&long_bar(), path, width(), 60.0, TOL)
        .iter()
        .any(|w| matches!(w, camcraft_motion::verify::Warning::Gouge { at: a } if *a == at || *a == at + 1))
}

#[test]
fn the_verdict_convicts_a_dip_that_breaks_a_law() {
    let base = run(&plan_with(dips()));
    let tip0 = chains(&base)[0].0;
    // A dip in the first lead: half-turn 1 of the first pass lifted.
    let mut p = base.clone();
    let at = bend(&mut p, 0, 1, tip0 + 0.2, 0.0);
    assert!(convicted(&p, at), "a dip in the first lead passed the verdict");
    // A dip in the last lead.
    let mut p = base.clone();
    let n = chains(&p)[0].1.len();
    let at = bend(&mut p, 0, n - 1, tip0 + 0.2, 0.0);
    assert!(convicted(&p, at), "a dip in the last lead passed the verdict");
    // A dip on the finishing pass (the deepest chain).
    let mut p = base.clone();
    let at = bend(&mut p, 2, 6, ROOT + 0.2, 0.0);
    assert!(convicted(&p, at), "a dip on the finishing pass passed the verdict");
    // A dip deeper than its pass.
    let mut p = base.clone();
    let at = bend(&mut p, 0, 6, tip0 - 0.1, 0.0);
    assert!(convicted(&p, at), "a half-turn under its pass passed the verdict");
    // A moved station: the helix phase shifted.
    let mut p = base.clone();
    let at = bend(&mut p, 0, 6, tip0, 0.1);
    assert!(convicted(&p, at), "a shifted phase passed the verdict");
    // A dip steeper than 45 degrees.
    let mut p = base;
    let at = bend(&mut p, 0, 7, tip0 + 0.6, 0.0);
    assert!(convicted(&p, at), "a steep dip passed the verdict");
}

fn refused(plan: &ThreadInfeed, end: f64, why: &str) {
    match thread_planned_in(&threaded(end), &th(end), plan, None) {
        Ok(_) => panic!("{why}: the thread cut"),
        Err(e) => assert!(format!("{e:?}").contains(why), "{why}: refused for another reason: {e:?}"),
    }
}

#[test]
fn dips_refuse_by_name() {
    // Higher than the first pass's depth of cut: the door's own refusal.
    let high = plan_with(ThreadDips { height: 0.5, ..dips() });
    assert!(high.refusal(DEPTH, 0.4, 60.0).is_some_and(|w| w.contains("first pass")));
    refused(&high, 24.0, "lifts past the first pass");
    // Higher than a later roughing pass's own (an allowance shortens it).
    let short_rung = ThreadInfeed { finish_allowance: 0.1, dips: ThreadDips { height: 0.35, ..dips() }, ..ThreadInfeed::default() };
    refused(&short_rung, 24.0, "lifts past a roughing pass's own 0.300");
    // An interval that leaves no whole lead between dips.
    refused(&plan_with(ThreadDips { interval: 2.0, ..dips() }), 24.0, "leave less than a lead");
    // A thread too short for a dip outside its first and last lead.
    refused(&plan_with(dips()), 14.0, "too short for a chip-break dip");
    // A one-pass thread has no roughing pass to dip.
    let p = ThreadParams { infeed: DEPTH, ..th(24.0) };
    let e = thread_planned_in(&long_bar(), &p, &plan_with(ThreadDips { height: 0.3, ..dips() }), None);
    assert!(e.is_err_and(|e| format!("{e:?}").contains("none above the root")));
    // A dip too small to tell from the micron grid.
    refused(&plan_with(ThreadDips { height: 0.001, ..dips() }), 24.0, "too small");
}
