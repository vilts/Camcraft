//! #174 acceptance: the positioning feeds of a probe cycle - the descent
//! from the rapid / safe plane to each touch's start, the travel between
//! touches, the rise - are protected moves. The dump marks them in the
//! stream (`GUARD`), and each probing post writes them as a stop-on-
//! contact move that ALARMS on an unexpected trip: the G31 skip read
//! the other way round (#5061-#5063 not at the target) on the Fanuc
//! dialect, G38.3 + a #5070 test on Tormach PathPilot (LinuxCNC). A
//! probing script that cannot voice the move is refused by name.
//!
//! Provenance: Fanuc's published operator documentation (G31 skip,
//! custom macro B system variables, #3000 alarms); the LinuxCNC G-code
//! reference (G38.3, #5070, O-code conditionals, MSG); our own fixtures.
//!
//! The probe cycles are recorded (#926): each mode's dumped stream
//! (`recorded/inputs/probe_t7_<mode>_g54.cnc`) and its path
//! (`recorded/inputs/probe_<mode>.cl`), both written and guarded by app's
//! `recorded_post_streams` from `motion::probing::probe_cycle`.
//!
//! Gate map (M4.4): covers post::probe_rec (GUARD), post::cnc, post::engine,
//! post::invert, cl::text, assets/posts fanuc.cps + tormach.cps
//! (onProbeGuard).

mod recorded;

use camcraft_post::cl::{text, MoveKind, Toolpath};
use camcraft_post::post::{self, cnc, engine};

const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const TORMACH: &str = include_str!("../../../assets/posts/tormach.cps");

/// The corner +X+Y at the origin, rim Z 20, setting G54.
fn corner_records() -> Vec<cnc::Record> {
    recorded::input_records("probe_t7_corner_g54.cnc")
}

/// A recorded probe path (`cl::text`): its moves, no touch metadata.
fn path(mode: &str) -> Toolpath {
    let name = format!("probe_{mode}.cl");
    text::from_text(&recorded::input(&name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Every motion of a probe path is a touch, a back-off, a protected
/// move or a rapid - never a plain feed - and each mode's positioning
/// feeds (descent, travel, rise) are exactly the GUARD records. Since
/// #570 every path opens with a protected descent from the safe plane to
/// the approach plane; a -Z face then starts its touch there, so its
/// other feed is its rise.
#[test]
fn the_positioning_feeds_of_a_probe_path_are_marked_protected() {
    let modes = [("face", 2), ("edge", 3), ("corner", 5), ("bore", 3), ("boss", 9)];
    for (name, want) in modes {
        let recs = recorded::input_records(&format!("probe_t7_{name}_g54.cnc"));
        let t = path(name);
        assert!(!recs.iter().any(|r| matches!(r, cnc::Record::Warn { .. })), "{name}: no WARN");
        let feeds = recs.iter().filter(|r| matches!(r, cnc::Record::Move(m) if m.kind != MoveKind::Rapid)).count();
        assert_eq!(feeds, 0, "{name}: no positioning feed posts as a plain G1");
        let guards: Vec<[f64; 3]> = recs
            .iter()
            .filter_map(|r| match r {
                cnc::Record::Guard(g) => Some(g.to),
                _ => None,
            })
            .collect();
        assert_eq!(guards.len(), want, "{name}: the descents, the travel and the rises");
        // The motion records in path order: one per move of the path.
        let motions: Vec<&cnc::Record> = recs
            .iter()
            .filter(|r| {
                matches!(r, cnc::Record::Move(_) | cnc::Record::Touch(_) | cnc::Record::Retreat(_) | cnc::Record::Guard(_))
            })
            .collect();
        // The motion count the viewer map relies on is unchanged.
        assert_eq!(motions.len(), t.moves.len(), "{name}: one record per move");
        // Each guard is one of the path's non-rapid moves, not a touch.
        let touch_ix: Vec<usize> =
            motions.iter().enumerate().filter(|(_, r)| matches!(r, cnc::Record::Touch(_))).map(|(i, _)| i).collect();
        assert!(!touch_ix.is_empty(), "{name}: a probe path touches");
        // The recorded path is on the micron lattice the stream speaks.
        for g in &guards {
            assert!(
                t.moves.iter().enumerate().any(|(i, m)| m.kind != MoveKind::Rapid
                    && !touch_ix.contains(&i)
                    && (m.to.x - g[0]).abs() < 1e-6
                    && (m.to.y - g[1]).abs() < 1e-6
                    && (m.to.z - g[2]).abs() < 1e-6),
                "{name}: guard {g:?} is a positioning feed of the path"
            );
        }
    }
    // A cutting path (no touches) carries no GUARD.
    let plain = path("corner");
    assert!(plain.probes.is_empty(), "a cl path carries no touches");
    let d = post::Dump { op_name: "Cut", tool_number: 1, tool_name: "EM", tool_diameter: 6.0, ..post::Dump::default() };
    assert!(!post::dump(&plain, &d).iter().any(|r| matches!(r, cnc::Record::Guard(_))));
}

/// GUARD round-trips through the stream text, and malformed lines are
/// refused by the parser, not guessed at.
#[test]
fn the_guard_record_round_trips_and_refuses_malformed_lines() {
    let recs = corner_records();
    let stream = cnc::serialize(&recs);
    assert!(stream.contains("\nGUARD "), "{stream}");
    assert_eq!(stream, recorded::input("probe_t7_corner_g54.cnc"), "serialize . parse is the identity");
    assert_eq!(cnc::parse(&stream).expect("parse"), recs);
    for bad in ["GUARD 1 2 3\n", "GUARD 1 2 F 300\n", "GUARD 1 2 3 F 0\n", "GUARD 1 2 x F 300\n"] {
        assert!(cnc::parse(bad).is_err(), "{bad:?} is refused");
    }
}

/// The Fanuc dialect: each guard is a G31 followed by one alarm test
/// per moving axis; the corner golden pins the travel between touches.
#[test]
fn fanuc_writes_protected_moves_as_g31_with_a_contact_alarm() {
    let stream = recorded::input("probe_t7_corner_g54.cnc");
    let out = engine::run(FANUC, &serde_json::Value::Null, &stream).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    assert!(
        !lines.iter().any(|l| l.starts_with("G01 ") || l.starts_with("G1 ")),
        "no absolute feed is left in a probe program:\n{}",
        out.program
    );
    let alarms = lines.iter().filter(|l| l.ends_with("THEN#3000=3(PROBE HIT ON POSITIONING)")).count();
    assert_eq!(alarms, 5, "the opening descent (#570), then a descent and a rise per touch, each a Z test:\n{}", out.program);
    // The first descent (#570): from the safe plane to the approach plane.
    let at = lines.iter().position(|l| l.starts_with("G31 ") && l.contains("F300")).expect("a G31");
    assert_eq!(lines[at + 1], "IF[ABS[#5063-25]GT0.001]THEN#3000=3(PROBE HIT ON POSITIONING)", "\n{}", out.program);
    recorded::app_golden("nc/fanuc/probe_corner.nc", &out.program);
}

/// Tormach PathPilot: G38.3 then the #5070 test that ends the program.
/// The touches themselves (G38.2) are #362's; this golden pins the
/// protected moves alone (the corner's guard records and rapids).
#[test]
fn tormach_writes_protected_moves_as_g38_3_with_a_trip_test() {
    let recs: Vec<cnc::Record> = corner_records()
        .into_iter()
        .filter(|r| !matches!(r, cnc::Record::Touch(_) | cnc::Record::Retreat(_) | cnc::Record::Found(_)))
        .collect();
    let out = engine::run(TORMACH, &serde_json::Value::Null, &cnc::serialize(&recs)).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(out.program.matches("G38.3 ").count(), 5, "\n{}", out.program);
    assert_eq!(out.program.matches("[#5070 EQ 1]").count(), 5, "\n{}", out.program);
    assert!(out.program.contains("o5001 if [#5070 EQ 1]\n(MSG, PROBE HIT ON POSITIONING)\nM2\no5001 endif\n"));
    assert!(!out.program.lines().any(|l| l.starts_with("G1 ")), "no plain feed:\n{}", out.program);
    recorded::app_golden("nc/tormach/probe_guard_moves.nc", &out.program);
}

/// A post that probes but cannot write a protected move is refused by
/// name, once per operation - never a plain G1 in its place.
#[test]
fn a_probing_post_without_on_probe_guard_is_refused_by_name() {
    let script = FANUC.replace("function onProbeGuard()", "function onProbeGuardNotHere()");
    let stream = recorded::input("probe_t7_corner_g54.cnc");
    let out = engine::run(&script, &serde_json::Value::Null, &stream).expect("post");
    let named: Vec<&String> = out.warnings.iter().filter(|w| w.contains("onProbeGuard")).collect();
    assert_eq!(named.len(), 1, "one refusal for the operation: {:?}", out.warnings);
    assert!(named[0].contains("Probe1"), "names the operation: {}", named[0]);
    assert!(!out.program.lines().any(|l| l.starts_with("G01 ")), "no plain feed stands in:\n{}", out.program);
}
