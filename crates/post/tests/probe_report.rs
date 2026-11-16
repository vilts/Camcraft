//! #575 acceptance: a probe program reports the deviation it measured on
//! the control before it writes the work offset, in the dialect's own
//! non-stopping message, or writes nothing where the dialect has none.
//! - Tormach PathPilot / LinuxCNC: one `(DEBUG, PROBE <axis> ERROR #19n
//!   <unit>)` per probed axis, after that axis's tolerance test and before
//!   the `G10 L20` (or the measure-only MSG/M0 stop); the unit word is
//!   `IN` in a G20 program and `MM` in a G21 one.
//! - Fanuc (custom macro B): no non-stopping display exists (#3000 alarms,
//!   #3006 stops), so nothing is added before the `G10 L2`.
//!
//! Provenance: the LinuxCNC G-code reference ("Messages" / "Comments":
//! DEBUG shows like MSG and substitutes #n), Fanuc's published custom
//! macro B documentation (#3000 / #3006); our own fixtures.
//!
//! The streams are recorded (#926): `recorded/inputs/probe_t99_*`,
//! written and guarded by app's `recorded_post_streams` from
//! `motion::probing::probe_cycle` (PathPilot's probe tool 99).
//!
//! Gate map (M4.4): covers assets/posts (tormach + fanuc probe result),
//! post::probe_rec.

mod recorded;

use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine};

const TORMACH: &str = include_str!("../../../assets/posts/tormach.cps");
const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const MX770: &str = include_str!("../../../assets/machines/tormach-770mx.mch");

/// A recorded probe stream: the face -Z at (10, 5, 20) or the corner +X+Y
/// at the origin, on tool 99, setting `set_wcs` (0 = G54, 1 = G55, none =
/// measure only).
fn stream(mode: &str, set_wcs: Option<u8>) -> String {
    let set = match set_wcs {
        Some(0) => "g54",
        Some(1) => "g55",
        None => "none",
        Some(n) => panic!("no recorded {mode} setting offset {n}"),
    };
    recorded::input(&format!("probe_t99_{mode}_{set}.cnc"))
}

const FACE: &str = "face";
const CORNER: &str = "corner";

fn tormach(p: &str, set_wcs: Option<u8>) -> String {
    let m = machine::mirror(&parse_mch(MX770).expect("770MX parses"));
    engine::run_with(TORMACH, &m, &stream(p, set_wcs), &doc_units(&m)).expect("post").program
}

/// A machine with no machine file posts in mm (G21).
fn tormach_mm(p: &str, set_wcs: Option<u8>) -> String {
    engine::run(TORMACH, &serde_json::Value::Null, &stream(p, set_wcs)).expect("post").program
}

fn line_of(prog: &str, text: &str) -> usize {
    prog.lines().position(|l| l == text).unwrap_or_else(|| panic!("no line {text:?}:\n{prog}"))
}

fn line_starting(prog: &str, prefix: &str) -> usize {
    prog.lines().position(|l| l.starts_with(prefix)).unwrap_or_else(|| panic!("no line {prefix:?}..:\n{prog}"))
}

#[test]
fn tormach_face_reports_z_after_its_tolerance_test_and_before_g10() {
    let prog = tormach(FACE, Some(1));
    assert!(prog.starts_with("(Tormach 770MX)\nG20 "), "the 770MX is an inch machine:\n{prog}");
    let report = line_of(&prog, "(DEBUG, PROBE Z ERROR #190 IN)");
    assert_eq!(prog.matches("(DEBUG,").count(), 1, "one report per probed axis:\n{prog}");
    assert!(report > line_of(&prog, "(MSG, PROBE Z OUT OF TOLERANCE)"), "after the tolerance test:\n{prog}");
    assert!(prog.contains("o901 endif\n(DEBUG, PROBE Z ERROR #190 IN)\nG10 L20 P2 Z[#5422-#190]\n"), "{prog}");
}

/// The report names the program's unit: a G21 program says MM, so the
/// operator never reads a mm deviation as inches (or the other way round).
#[test]
fn tormach_mm_program_reports_in_mm() {
    let prog = tormach_mm(FACE, Some(1));
    assert!(prog.contains("\nG21 "), "a metric program:\n{prog}");
    assert!(prog.contains("o901 endif\n(DEBUG, PROBE Z ERROR #190 MM)\nG10 L20 P2 Z[#5422-#190]\n"), "{prog}");
    assert!(!prog.contains(" IN)"), "no inch word in a G21 program:\n{prog}");
    let both = tormach_mm(CORNER, Some(0));
    assert!(both.contains("(DEBUG, PROBE X ERROR #190 MM)\n"), "{both}");
    assert!(both.contains("(DEBUG, PROBE Y ERROR #191 MM)\n"), "{both}");
}

#[test]
fn tormach_corner_reports_each_axis_before_g10() {
    let prog = tormach(CORNER, Some(0));
    let g10 = line_starting(&prog, "G10 L20 ");
    let x = line_of(&prog, "(DEBUG, PROBE X ERROR #190 IN)");
    let y = line_of(&prog, "(DEBUG, PROBE Y ERROR #191 IN)");
    assert!(x > line_of(&prog, "(MSG, PROBE X OUT OF TOLERANCE)") && x < g10, "{prog}");
    assert!(y > line_of(&prog, "(MSG, PROBE Y OUT OF TOLERANCE)") && y < g10, "{prog}");
    assert_eq!(prog.matches("(DEBUG,").count(), 2, "{prog}");
}

#[test]
fn tormach_measure_only_reports_before_its_stop() {
    let prog = tormach(FACE, None);
    assert!(!prog.contains("G10"), "nothing is set:\n{prog}");
    assert!(prog.contains("(DEBUG, PROBE Z ERROR #190 IN)\n(MSG, CHECK PROBE RESULT #190..)\nM0\n"), "{prog}");
}

#[test]
fn fanuc_has_no_non_stopping_message_so_writes_no_report() {
    let out = engine::run(FANUC, &serde_json::Value::Null, &stream(CORNER, Some(0))).expect("post");
    let prog = out.program;
    let g10 = line_starting(&prog, "G10 L2 ");
    let y_test = line_starting(&prog, "IF[ABS[#191]GT");
    assert_eq!(g10, y_test + 1, "the offset write follows the last tolerance test directly:\n{prog}");
    assert!(!prog.contains("DEBUG") && !prog.contains("#3006"), "no report and no stop on a good probe:\n{prog}");
}

/// #902: the program's unit is the document's, never the machine file's.
/// These runs on an inch machine file stand for the inch document they
/// posted before #902 (the file's own unit, through either door).
fn doc_units(m: &serde_json::Value) -> serde_json::Value {
    let word = m
        .get("units")
        .and_then(serde_json::Value::as_str)
        .or_else(|| m.pointer("/kinematics/default/units/length").and_then(serde_json::Value::as_str));
    let inch = matches!(word, Some("inch" | "inches"));
    serde_json::json!({ "Units": if inch { "inch" } else { "mm" } })
}
