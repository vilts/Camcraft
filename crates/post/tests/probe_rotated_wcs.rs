//! #2683: the LinuxCNC and Tormach probe posts stop by name before the
//! `G10 L20` write when a work offset has an XY rotation. The write
//! `G10 L20 P<n> X[#5420-#190]` is exact (target = active offset +
//! deviation, #2610) only while the frames of the active and the target
//! system are the machine's: with a rotation the deviation is measured
//! along rotated axes, and a target system with a different rotation gets
//! an origin that is right at the probe's position only. The post cannot
//! know the rotation - it is a register of the control (R of G10 L2) - so
//! the program tests it at run time, in an O-word block, and ends with M2.
//! A Z-only result (face) is not guarded: a rotation about Z leaves Z.
//!
//! Provenance: the LinuxCNC G-code reference (G10 L2 "R rotation about
//! the Z axis", G10 L20, numbered parameters 5221-5230 "Coordinate System
//! 1, G54 for X, Y, Z, A, B, C, U, V, W & R" and each next system 20
//! higher, O-code conditionals, binary operators NE / OR); our own fixtures.
//!
//! The streams are recorded (#926): `recorded/inputs/probe_t99_*` and
//! `probe_t7_*`, written and guarded by app's `recorded_post_streams`.
//! Every one probes in G54 (`PARAM WorkOffset 1`).
//!
//! Gate map: covers assets/posts/tormach.cps, assets/posts/linuxcnc.cps
//! (the probe result's rotation guard), camcraft_post::post::engine.

mod recorded;

use camcraft_post::post::engine;
use serde_json::{json, Value};

const TORMACH: &str = include_str!("../../../assets/posts/tormach.cps");
const LINUXCNC: &str = include_str!("../../../assets/posts/linuxcnc.cps");

/// Both posts, machine-less (mm). The LinuxCNC post needs its retract
/// form stated when no machine file states a safe Z.
fn posts() -> [(&'static str, &'static str, Value); 2] {
    [("tormach", TORMACH, Value::Null), ("linuxcnc", LINUXCNC, json!({ "retractMode": "g28" }))]
}

fn post(cps: &str, options: &Value, input: &str) -> String {
    post_stream(cps, options, &recorded::input(input))
}

fn post_stream(cps: &str, options: &Value, stream: &str) -> String {
    let out = engine::run_with(cps, &Value::Null, stream, options).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out.program
}

/// The guard block under O-number `o`, testing `cond`, naming `who`,
/// directly before the offset write `g10`.
fn guard(o: u32, cond: &str, who: &str, g10: &str) -> String {
    format!("o{o} if {cond}\n(MSG, PROBE: {who} XY ROTATION NOT ZERO - NO OFFSET WRITTEN)\nM2\no{o} endif\n{g10}\n")
}

/// Probing in G54 and setting G54: the one rotation register is G54's
/// (#5230), and the stop comes after the reports, directly before G10.
#[test]
fn setting_the_active_offset_stops_when_it_is_rotated() {
    for (name, cps, options) in posts() {
        let prog = post(cps, &options, "probe_t99_corner_g54.cnc");
        let want = guard(903, "[#5230 NE 0]", "G54", "G10 L20 P1 X[#5420-#190] Y[#5421-#191]");
        assert!(prog.contains(&format!("(DEBUG, PROBE Y ERROR #191 MM)\n{want}")), "{name}:\n{prog}");
    }
}

/// The tested register follows the section's own system: the same
/// stream probing in G55 and setting G55 tests #5250 only.
#[test]
fn the_guard_follows_the_sections_work_offset() {
    const IN_G54: &str = "PARAM WorkOffset 1.000000\n";
    for (name, cps, options) in posts() {
        let recorded = recorded::input("probe_t99_corner_g55.cnc");
        assert_eq!(recorded.matches(IN_G54).count(), 1, "the recorded stream probes in G54");
        let prog = post_stream(cps, &options, &recorded.replace(IN_G54, "PARAM WorkOffset 2.000000\n"));
        let want = guard(903, "[#5250 NE 0]", "G55", "G10 L20 P2 X[#5420-#190] Y[#5421-#191]");
        assert!(prog.contains(&want), "{name}:\n{prog}");
    }
}

/// Probing in G54 and setting another system: either rotation stops the
/// write (G55 is #5250, G59 is #5330: 5210 + 20 x the system number).
#[test]
fn setting_another_offset_stops_when_either_is_rotated() {
    for (name, cps, options) in posts() {
        let prog = post(cps, &options, "probe_t99_corner_g55.cnc");
        let want = guard(903, "[[#5230 NE 0] OR [#5250 NE 0]]", "G54 OR G55", "G10 L20 P2 X[#5420-#190] Y[#5421-#191]");
        assert!(prog.contains(&want), "{name}:\n{prog}");
        let prog = post(cps, &options, "probe_t7_bore_g59.cnc");
        let want = guard(903, "[[#5230 NE 0] OR [#5330 NE 0]]", "G54 OR G59", "G10 L20 P6 X[#5420-#190] Y[#5421-#191]");
        assert!(prog.contains(&want), "{name}:\n{prog}");
    }
}

/// One guard per offset write, under an O-number no tolerance test used,
/// and an edge (X only) is guarded like a corner.
#[test]
fn the_guard_is_written_once_under_its_own_o_number() {
    for (name, cps, options) in posts() {
        let prog = post(cps, &options, "probe_t99_edge_g55.cnc");
        assert_eq!(prog.matches("XY ROTATION NOT ZERO").count(), 1, "{name}:\n{prog}");
        let want = guard(902, "[[#5230 NE 0] OR [#5250 NE 0]]", "G54 OR G55", "G10 L20 P2 X[#5420-#190]");
        assert!(prog.contains(&want), "{name}:\n{prog}");
        assert_eq!(prog.lines().filter(|l| l.starts_with("o902 ")).count(), 2, "{name}: if and endif only:\n{prog}");
    }
}

/// A rotation about Z does not move Z: a face result is written without
/// the guard. A measure-only result writes no offset and needs none.
#[test]
fn a_z_only_result_and_a_measure_only_result_have_no_guard() {
    for (name, cps, options) in posts() {
        let face = post(cps, &options, "probe_t99_face_g55.cnc");
        assert!(face.contains("(DEBUG, PROBE Z ERROR #190 MM)\nG10 L20 P2 Z[#5422-#190]\n"), "{name}:\n{face}");
        assert!(!face.contains("ROTATION"), "{name}:\n{face}");
        let measure = post(cps, &options, "probe_t99_bore_none.cnc");
        assert!(!measure.contains("G10") && !measure.contains("ROTATION"), "{name}:\n{measure}");
    }
}
