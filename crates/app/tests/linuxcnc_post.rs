//! The generic LinuxCNC post (#2303): the program shape stock LinuxCNC
//! reads (no O word, G91.1 stated, G64 P or G61, `T<n> M6` then `G43 H<n>`),
//! and the machine-specific parts as options: the retract (G53 G0 Z from
//! the machine file's safe_retract, G28 / G30 Z only, or none - never an
//! assumed G53 G0 Z0, #435 / #495), G43 left out when the M6 remap applies
//! the length, the program end and the % delimiters. Rigid tapping is
//! G33.1, written only on a machine that states a spindle encoder
//! (`rigid_tapping: true`); a floating-holder tap is G84. What the post
//! cannot write (a tilted move, control compensation) is refused by name.
//! Provenance: our own tests over our own post and machine file; the
//! dialect facts from the public LinuxCNC G-code reference (linuxcnc.org).
//!
//! Gate map: covers camcraft_post::post (linuxcnc.cps), camcraft_post::post::machine
//! (rigidTappingStated, safeRetractZ), app::posting (registry, machine library).

use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use camcraft_app::posting::{BUILTINS, MACHINES, builtin_machine_text, builtin_post_text};
use serde_json::{Value, json};
use std::path::Path;

/// A milling section (tool 3, 8000 rpm) and a drilling one (tool 4).
const STREAM: &str = "PARAM ClearancePlane 15\nPARAM RetractPlane 5\nPARAM StockZTop 0\nPARAM SpindleSpeed 8000\nTOOL 3 EM6\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 10000 Y 0 Z -1000 F 500000\nArc CW X 20000 Y 0 Z -2000 F 400000 I 15000 J 0\nENDSECTION\n\
                      PARAM ClearancePlane 15\nPARAM RetractPlane 5\nPARAM SpindleSpeed 6000\nTOOL 4 DRILL5\nSECTION\nCYCLE DRILL X 20000 Y 5000 Z -3000 R 2000 P 1500 F 100000\nRapid X 20000 Y 5000 Z 2000\nPlunge X 20000 Y 5000 Z -3000 F 100000\nRapid X 20000 Y 5000 Z 2000\nRapid X 20000 Y 5000 Z 5000\nENDSECTION\n";

/// A M10x1.5 tap at 500 rpm (feed 750 mm/min), rigid or floating.
fn tap_stream(kind: &str) -> String {
    format!(
        "PARAM ClearancePlane 15\nPARAM RetractPlane 5\nPARAM StockZTop 0\nPARAM SpindleSpeed 500\nTOOL 6 TAP10\nSECTION\n\
         Rapid X 30000 Y 10000 Z 5000\nRapid X 30000 Y 10000 Z 2000\n\
         CYCLE {kind} X 30000 Y 10000 Z -12000 R 2000 F 750000\n\
         Plunge X 30000 Y 10000 Z -12000 F 750000\nFeed X 30000 Y 10000 Z 2000 F 750000\nRapid X 30000 Y 10000 Z 5000\nENDSECTION\n"
    )
}

/// The G and M codes this post may write - each one in the LinuxCNC
/// G-code / M-code reference.
const CODES: &[&str] = &[
    "G0", "G1", "G2", "G3", "G10", "G17", "G18", "G19", "G20", "G21", "G28", "G30", "G33.1", "G38.2", "G38.3",
    "G40", "G43", "G49", "G53", "G54", "G55", "G56", "G57", "G58", "G59", "G59.1", "G59.2", "G59.3", "G61",
    "G64", "G73", "G80", "G81", "G82", "G83", "G84", "G85", "G86", "G89", "G90", "G91", "G91.1", "G94", "G99",
    "M0", "M1", "M2", "M3", "M5", "M6", "M7", "M8", "M9", "M30",
];

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn cps() -> String {
    read("assets/posts/linuxcnc.cps")
}

/// The shipped generic machine, mounted the way the app hands it on.
fn generic() -> Value {
    assert!(MACHINES.iter().any(|(n, _)| *n == "LinuxCNC 3-axis"), "the library ships the LinuxCNC machine");
    let text = builtin_machine_text("LinuxCNC 3-axis").unwrap_or_else(|e| panic!("reads: {e}"));
    machine::mirror(&parse_mch(text).unwrap_or_else(|e| panic!("parses: {e}")))
}

/// The generic machine with what the user states for a real one.
fn stated(safe_z: Option<f64>, encoder: bool) -> Value {
    let mut text: Value = serde_json::from_str(&read("assets/machines/linuxcnc-3axis.mch")).expect("json");
    let parts = &mut text["kinematics"]["default"]["parts"][0]["parts"];
    if let Some(z) = safe_z {
        parts[0]["safe_retract"] = json!(z);
    }
    if encoder {
        parts[1]["spindle"]["rigid_tapping"] = json!(true);
    }
    machine::mirror(&parse_mch(&text.to_string()).unwrap_or_else(|e| panic!("parses: {e}")))
}

fn run(m: &Value, stream: &str, options: &Value) -> (String, Vec<String>) {
    let out = engine::run_with(&cps(), m, stream, options).unwrap_or_else(|e| panic!("runs: {e}"));
    (out.program, out.warnings)
}

fn post(m: &Value, stream: &str, options: &Value) -> String {
    let (nc, warnings) = run(m, stream, options);
    assert!(warnings.is_empty(), "{warnings:?}\n{nc}");
    assert_linuxcnc_words(&nc);
    nc
}

/// Every G or M code of every non-comment line is one this post may write.
fn assert_linuxcnc_words(nc: &str) {
    for l in nc.lines() {
        if l.starts_with('(') || l == "%" {
            continue;
        }
        let first = l.split_whitespace().next().unwrap_or("");
        assert!(!first.starts_with('O') && !first.starts_with('N'), "no O word or N number by default: {l:?}");
        for w in l.split_whitespace() {
            if w.starts_with('G') || w.starts_with('M') {
                assert!(CODES.contains(&w), "{w} is not a word this post writes: {l:?}\n{nc}");
            }
        }
    }
}

#[test]
fn the_registry_and_the_library_ship_it() {
    assert!(BUILTINS.iter().any(|(n, _)| *n == "LinuxCNC"), "the post is built in");
    assert_eq!(builtin_post_text("LinuxCNC").ok(), Some(cps().as_str()), "the built-in reads the shipped file");
    let m = generic();
    assert!(m.get("safeRetractZ").is_none_or(Value::is_null), "the generic file assumes no Z0: {m}");
    // It states no spindle encoder either: rigid_tap_needs_a_stated_encoder.
    assert!(m.pointer("/kinematics/static/head/rigidTapping").is_none(), "{m}");
}

#[test]
fn an_unstated_safe_z_is_refused_never_g53_z0() {
    let (nc, warnings) = run(&generic(), STREAM, &Value::Null);
    assert!(
        warnings.iter().any(|w| w.contains("does not state its safe Z retract") && w.contains("safe_retract")),
        "{warnings:?}"
    );
    assert!(!nc.contains("G53 G0 Z0"), "never the PathPilot convention: {nc}");
}

#[test]
fn a_stated_safe_z_is_the_retract_before_m6_and_at_the_end() {
    let nc = post(&stated(Some(-2.0), false), STREAM, &Value::Null);
    let lines: Vec<&str> = nc.lines().collect();
    assert_eq!(lines.get(1), Some(&"G21 G17 G90 G91.1 G94 G40 G49 G80"), "{nc}");
    assert_eq!(lines.get(2), Some(&"G64 P0.025"), "{nc}");
    assert_eq!(lines.last(), Some(&"M30"), "{nc}");
    let t3 = lines.iter().position(|l| *l == "T3 M6").expect("tool 3");
    assert_eq!(lines[t3 - 1], "G53 G0 Z-2", "{nc}");
    assert_eq!(lines[t3 + 1], "G43 H3", "{nc}");
    assert_eq!(lines[t3 + 2], "G53 G0 Z-2", "Z up again after M6: {nc}");
    assert_eq!(lines[lines.len() - 2], "G53 G0 Z-2", "{nc}");
    assert!(!nc.contains("G30") && !nc.contains("G28"), "no stored-position move unasked: {nc}");
    // The first X/Y traverse states its height (#1614).
    let s = lines.iter().position(|l| *l == "S8000 M3").expect("spindle");
    assert_eq!(&lines[s + 1..s + 4], &["M8", "G0 Z15", "G0 X0 Y0"], "{nc}");
    assert!(nc.contains("G2 X20 Y0 Z-2 I5 J0 F400"), "a helix with incremental IJ: {nc}");
    assert!(nc.contains("G99\nG82 X20 Y5 Z-3 R2 P1.5 F100\nG80"), "{nc}");
}

#[test]
fn g28_g30_and_none_retracts_move_z_only_or_nothing() {
    let m = generic(); // no safe Z stated: these modes do not need one
    for (mode, word) in [("g28", "G28"), ("g30", "G30")] {
        let nc = post(&m, STREAM, &json!({ "retractMode": mode }));
        let lines: Vec<&str> = nc.lines().collect();
        let t3 = lines.iter().position(|l| *l == "T3 M6").expect("tool 3");
        assert_eq!(&lines[t3 - 2..t3], &[format!("G91 {word} Z0").as_str(), "G90"], "{nc}");
        assert_eq!(&lines[lines.len() - 3..], &[format!("G91 {word} Z0").as_str(), "G90", "M30"], "{nc}");
        assert!(!nc.contains("G53"), "{nc}");
    }
    let nc = post(&m, STREAM, &json!({ "retractMode": "none" }));
    assert!(!nc.contains("G53") && !nc.contains("G28") && !nc.contains("G30"), "{nc}");
    assert!(nc.contains("T3 M6\nG43 H3\nS8000 M3\nM8\nG0 Z15\nG0 X0 Y0"), "the traverse height still stated: {nc}");
}

#[test]
fn a_remap_that_applies_the_length_gets_no_g43_and_no_section_g49() {
    let nc = post(&stated(Some(0.0), false), STREAM, &json!({ "writeToolLength": false }));
    assert!(!nc.contains("G43"), "{nc}");
    // G49 only in the header, before the first M6.
    assert_eq!(nc.matches("G49").count(), 1, "{nc}");
    assert!(nc.contains("G17 G90 G54 G40 G80\n"), "{nc}");
}

#[test]
fn path_control_end_word_and_percent_are_options() {
    let m = stated(Some(0.0), false);
    let nc = post(&m, STREAM, &json!({ "pathControl": "exact", "programEnd": "M2", "percentDelimiters": true }));
    let lines: Vec<&str> = nc.lines().collect();
    assert_eq!(lines.first(), Some(&"%"), "{nc}");
    assert_eq!(lines.last(), Some(&"%"), "{nc}");
    assert_eq!(lines[lines.len() - 2], "M2", "{nc}");
    assert!(nc.contains("\nG61\n") && !nc.contains("G64"), "{nc}");
    let nc = post(&m, STREAM, &json!({ "Units": "inch", "blendTolerance": 0.05 }));
    assert!(nc.starts_with("(LinuxCNC 3-axis)\nG20 G17"), "{nc}");
    assert!(nc.contains("\nG64 P0.002\n"), "the tolerance in the program's unit: {nc}");
    // #2189: the end-unit word goes after the last retract, before the end.
    let nc = post(&m, STREAM, &json!({ "EndUnits": "inch" }));
    assert!(nc.ends_with("G53 G0 Z0\nG20\nM30\n"), "{nc}");
}

#[test]
fn rigid_tap_needs_a_stated_encoder() {
    let (_, warnings) = run(&stated(Some(0.0), false), &tap_stream("TAP"), &Value::Null);
    assert!(warnings.iter().any(|w| w.contains("rigid tapping (G33.1)") && w.contains("encoder")), "{warnings:?}");
    let nc = post(&stated(Some(0.0), true), &tap_stream("TAP"), &Value::Null);
    assert!(nc.contains("S500 M3"), "{nc}");
    // Seated on R over the hole by the stream, then one synchronized move:
    // pitch = 750 / 500 = 1.5 mm per revolution, Z only.
    assert!(nc.contains("G0 X30 Y10 Z2\nG33.1 Z-12 K1.5\nG80\nG0 X30 Y10 Z5"), "{nc}");
    let nc = post(&stated(Some(0.0), true), &tap_stream("TAP"), &json!({ "Units": "inch" }));
    assert!(nc.contains("G33.1 Z-0.4724 K0.059055"), "the pitch at full precision in inch: {nc}");
}

#[test]
fn a_floating_tap_is_g84() {
    let nc = post(&stated(Some(0.0), false), &tap_stream("FLOATINGTAP"), &Value::Null);
    assert!(nc.contains("G99\nG84 X30 Y10 Z-12 R2 F750\nG80"), "{nc}");
    assert!(!nc.contains("G33.1"), "{nc}");
}

#[test]
fn tilted_moves_and_control_comp_are_refused_by_name() {
    let m = stated(Some(0.0), false);
    let tilted = "PARAM ClearancePlane 15\nPARAM SpindleSpeed 8000\nTOOL 3 EM6\nSECTION\nRapid X 0 Y 0 Z 5000 A 30000 B 0 C 0\nENDSECTION\n";
    let (_, warnings) = run(&m, tilted, &Value::Null);
    assert!(warnings.iter().any(|w| w.contains("3-axis")), "{warnings:?}");
    let comp = "PARAM ClearancePlane 15\nPARAM SpindleSpeed 8000\nTOOL 3 EM6\nSECTION\nRapid X 0 Y 0 Z 5000\nCOMP LEFT 3000\nFeed X 10000 Y 0 Z -1000 F 500000\nCOMP OFF\nENDSECTION\n";
    let (_, warnings) = run(&m, comp, &Value::Null);
    assert!(warnings.iter().any(|w| w.contains("G41/G42")), "{warnings:?}");
}
