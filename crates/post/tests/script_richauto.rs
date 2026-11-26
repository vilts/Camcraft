//! Provenance: original #1914 pins for the RichAuto DSP A11 post contract
//! specced from OEM A11 manuals, RS-274/ISO 6983, CamCraft post laws, and
//! the block format of a program that a Laguna Swift A11 runs (#1907).
//! Gate map (M4.4): covers assets/posts/richauto.cps, camcraft_post::post::engine,
//! camcraft_post::post::end_units, camcraft_post::post::options.

use camcraft_post::post::options::{declared_options, resolve_options, OptionKind, OptionValue};
use camcraft_post::post::{end_units, engine, PostError};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

#[macro_use]
mod recorded;

post_suite!("richauto.cps");

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

const HEADER: &str = "PARAM OperationName A11\nPARAM SpindleSpeed 8000\nPARAM WorkOffset 1\nPARAM ClearancePlane 10\nPARAM RetractPlane 3\nPARAM StockZTop 0\nPARAM Coolant off\nTOOL 1 ROUTER\nSECTION\n";
const CUT: &str = "Rapid X 10000 Y 0 Z 5000\nPlunge X 10000 Y 0 Z 0 F 300000\nArc CCW X 0 Y 10000 Z 0 F 800000 I 0 J 0\nRapid X 0 Y 10000 Z 10000\n";

fn run(stream: &str, options: &Value) -> engine::PostOutput {
    engine::run_with(
        &read("assets/posts/richauto.cps"),
        &json!({"model": "RichAuto A11", "spindleMax": 24000, "coolants": []}),
        stream,
        options,
    )
    .expect("the post runs or refuses by name")
}

fn program(body: &str) -> String {
    format!("{HEADER}{body}ENDSECTION\n")
}

fn clean(out: &engine::PostOutput) {
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
}

fn refused(stream: &str, words: &[&str]) {
    let out = run(stream, &Value::Null);
    let warnings = out.warnings.join(" ").to_ascii_lowercase();
    assert!(words.iter().any(|word| warnings.contains(word)), "expected {words:?}: {warnings}");
}

/// The block text after its `N<seq>` word; comments and `%` stay as they are.
fn body(line: &str) -> &str {
    match line.strip_prefix('N') {
        Some(rest) => rest.trim_start_matches(|c: char| c.is_ascii_digit()),
        None => line,
    }
}

/// The program with every sequence number taken off, one block per line.
fn bodies(p: &str) -> Vec<&str> {
    p.lines().map(body).collect()
}

/// True when a block's words start with `word` and the next byte is not a digit.
fn starts_word(b: &str, word: &str) -> bool {
    b.strip_prefix(word).is_some_and(|r| !r.starts_with(|c: char| c.is_ascii_digit()))
}

#[test]
fn richauto_writes_the_a11_mmg_block_format() {
    let out = run(&program(CUT), &Value::Null);
    clean(&out);
    let p = &out.program;
    let lines: Vec<&str> = p.lines().collect();
    assert_eq!(lines.last(), Some(&"%"), "the program ends with %: {p}");
    assert!(lines[0].starts_with("(Filename: "), "{p}");
    let mut want = 10;
    for line in &lines[..lines.len() - 1] {
        if line.starts_with('(') {
            assert!(line.ends_with(')'), "{line}");
            continue;
        }
        let n = format!("N{want}");
        assert!(starts_word(line, &n), "N{want} expected (step 10): {line}\n{p}");
        assert!(!line.contains(' '), "no spaces between words: {line}");
        want += 10;
    }
    assert_eq!(body(lines[lines.len() - 2]), "M30", "{p}");
    let b = bodies(p);
    for want in ["G21G90G17", "M03S8000", "G00Z10.000", "G00X10.000Y0.000", "G00Z5.000", "G1Z0.000F300.0"] {
        assert!(b.contains(&want), "missing block {want}: {p}");
    }
    // The arc: three-decimal end point and incremental centre (AbsCntr Off).
    assert!(b.contains(&"G3X0.000Y10.000Z0.000I-10.000J0.000F800.0"), "{p}");
    let blocks: Vec<&str> = b.iter().copied().filter(|l| !l.starts_with('(')).collect();
    for word in ["G54", "G40", "G80", "G91", "G94", "G43", "G49", "G53", "M6", "T", ";"] {
        assert!(!blocks.iter().any(|l| l.contains(word)), "forbidden {word}: {p}");
    }
    assert!(!lines.iter().any(|l| l.starts_with('O')), "{p}");
    assert!(p.is_ascii(), "{p}");
}

#[test]
fn unchanged_axis_and_feed_words_are_left_out() {
    let out = run(
        &program("Rapid X 0 Y 0 Z 5000\nFeed X 10000 Y 0 Z 5000 F 300000\nFeed X 20000 Y 0 Z 5000 F 300000\nFeed X 20000 Y 0 Z 5000 F 300000\nFeed X 20000 Y 5000 Z 5000 F 200000\n"),
        &Value::Null,
    );
    clean(&out);
    let b = bodies(&out.program);
    let i = b.iter().position(|l| *l == "G1X10.000F300.0").expect("first cut");
    assert_eq!(b[i + 1], "G1X20.000", "{}", out.program);
    assert_eq!(b[i + 2], "G1Y5.000F200.0", "a zero-length move writes no block: {}", out.program);
}

#[test]
fn clearance_comes_before_the_first_xy_and_before_the_spindle_stop() {
    let out = run(&program(CUT), &Value::Null);
    clean(&out);
    let b = bodies(&out.program);
    let first_xy = b.iter().position(|l| !l.starts_with('(') && l.contains('X')).expect("first XY");
    assert!(b[..first_xy].contains(&"G00Z10.000"), "{}", out.program);
    let stop = b.iter().rposition(|l| *l == "M05").expect("spindle stop");
    assert_eq!(b[stop - 1], "G00Z10.000", "{}", out.program);
}

#[test]
fn world_origin_does_not_change_incremental_arc_centres() {
    let translated = program(CUT).replace("TOOL 1", "PARAM OriginX 5\nPARAM OriginY 7\nPARAM OriginZ 2\nTOOL 1");
    let out = run(&translated, &Value::Null);
    clean(&out);
    let b = bodies(&out.program);
    assert!(b.contains(&"G3X-5.000Y3.000Z-2.000I-10.000J0.000F800.0"), "{}", out.program);
    assert!(b.contains(&"G00Z8.000"), "{}", out.program);
}

#[test]
fn spindle_dwell_is_configurable_and_declared_in_schema() {
    let cps = read("assets/posts/richauto.cps");
    let schema = declared_options(&cps).expect("declaredOptions parses");
    let dwell = schema.iter().find(|o| o.key == "SpindleDwellMs").expect("SpindleDwellMs declared");
    assert_eq!(dwell.default, OptionValue::Float(0.0));
    assert!(matches!(dwell.kind, OptionKind::Number { min: Some(0.0), max: Some(2147483647.0) }));
    let defaults = resolve_options(&schema, &BTreeMap::new()).expect("defaults resolve");

    let base = run(&program(CUT), &Value::Null);
    clean(&base);
    assert!(!base.program.contains("G04"), "{}", base.program);
    let with_defaults = run(&program(CUT), &Value::Object(defaults));
    clean(&with_defaults);
    assert_eq!(with_defaults.program, base.program);

    for (ms, word) in [(json!(1250), "G04P1250"), (json!(0.1), "G04P1")] {
        let out = run(&program(CUT), &json!({"SpindleDwellMs": ms}));
        clean(&out);
        let b = bodies(&out.program);
        let on = b.iter().position(|l| *l == "M03S8000").expect("spindle on");
        assert_eq!(b[on + 1], word, "{}", out.program);
    }
}

#[test]
fn bad_spindle_dwell_is_refused_by_name() {
    for value in [json!(-1), json!(2147483648_u64), json!("bad"), json!("2")] {
        let out = run(&program(CUT), &json!({"SpindleDwellMs": value}));
        assert!(out.warnings.join(" ").to_ascii_lowercase().contains("dwell"), "{:?}", out.warnings);
    }
}

#[test]
fn end_units_declares_note_and_notes_inch_ask_without_g20() {
    let cps = read("assets/posts/richauto.cps");
    assert_eq!(end_units::words(&cps).expect("manifest"), None);
    let base = run(&program(CUT), &Value::Null);
    clean(&base);
    let mm_same = run(&program(CUT), &json!({"EndUnits": "mm"}));
    clean(&mm_same);
    assert_eq!(mm_same.program, base.program);

    let inch_end = run(&program(CUT), &json!({"EndUnits": "inch"}));
    clean(&inch_end);
    let noted = inch_end.program.lines().filter(|l| l.contains("END UNITS NOT WRITTEN")).count();
    assert_eq!(noted, 1, "{}", inch_end.program);
    assert!(!inch_end.program.contains("G20"), "{}", inch_end.program);
}

#[test]
fn peck_drilling_keeps_each_expanded_plunge_and_retract() {
    let out = run(&read("crates/post/tests/recorded/streams/drill_retract.cnc"), &Value::Null);
    clean(&out);
    for word in ["G81", "G82", "G83", "G73", "G85", "G86", "G89"] {
        assert!(!out.program.contains(word), "{}", out.program);
    }
    let b = bodies(&out.program);
    for z in [-3, -7, -11, -15, -19, -20] {
        let zw = format!("Z{z}.000");
        assert!(b.iter().any(|l| starts_word(l, "G1") && l.contains(&zw)), "missing plunge {z}: {}", out.program);
    }
    assert!(b.iter().any(|l| starts_word(l, "G00") && l.contains("Z-2.000")), "{}", out.program);
}

#[test]
fn tapping_is_refused_by_name() {
    for kind in ["TAP", "FLOATINGTAP"] {
        let stream = program(&format!(
            "Rapid X 0 Y 0 Z 3000\nCYCLE {kind} X 0 Y 0 Z -10000 R 3000 ENTRY 0 Q 0 P 0 F 120000 AT 1\nFeed X 0 Y 0 Z -10000 F 120000\nFeed X 0 Y 0 Z 3000 F 120000\n"
        ));
        let out = run(&stream, &Value::Null);
        let warnings = out.warnings.join(" ").to_ascii_lowercase();
        assert!(warnings.contains("tapping") || warnings.contains("g84"), "{kind}: {warnings}");
    }
}

#[test]
fn cutter_compensation_is_refused_by_name() {
    for side in ["LEFT", "RIGHT"] {
        refused(
            &program(&format!("Rapid X 10000 Y 0 Z 5000\nCOMP {side} 3000\nFeed X 20000 Y 0 Z 5000 F 300000\nCOMP OFF\n")),
            &["compensation", "g41", "g42"],
        );
    }
}

#[test]
fn probing_is_refused_by_name() {
    refused(&read("crates/post/tests/recorded/streams/probe.cnc"), &["probing"]);
}

#[test]
fn a_second_tool_is_refused_but_repeated_same_tool_is_allowed() {
    let first = program(CUT);
    clean(&run(&format!("{first}{}", program(CUT)), &Value::Null));
    let changed = format!("{first}{}", program(CUT).replace("TOOL 1", "TOOL 2"));
    refused(&changed, &["tool change", "single-tool"]);
}

#[test]
fn another_work_offset_is_refused_by_name() {
    refused(&program(CUT).replace("WorkOffset 1", "WorkOffset 2"), &["work offset"]);
}

#[test]
fn missing_or_substock_clearance_is_refused_by_name() {
    for stream in [
        program(CUT).replace("PARAM ClearancePlane 10\n", ""),
        program(CUT).replace("ClearancePlane 10", "ClearancePlane -1"),
    ] {
        refused(&stream, &["clearance"]);
    }
}

#[test]
fn inch_document_is_refused_by_name() {
    let err = engine::run_with(&read("assets/posts/richauto.cps"), &Value::Null, &program(CUT), &json!({"Units": "inch"}))
        .expect_err("metric-only post refuses inch");
    assert!(
        matches!(err, PostError::Refused(_)) && err.to_string().to_ascii_lowercase().contains("inch"),
        "{err}"
    );
}

fn hole(kind: &str, dwell: u32, feed: &str, floor: bool) -> String {
    let chain = if floor { "Plunge X 10000 Y 20000 Z -6000 F 100000\n" } else { "" };
    program(&format!(
        "Rapid X 10000 Y 20000 Z 2000\nCYCLE {kind} X 10000 Y 20000 Z -6000 R 2000 ENTRY 0 Q 0 P {dwell}{feed} AT 1\n{chain}Rapid X 10000 Y 20000 Z 2000\nRapid X 10000 Y 20000 Z 10000\n"
    ))
}

#[test]
fn bore_stops_at_floor_and_restarts_after_rapid_retract() {
    for dwell in [0, 500] {
        let out = run(&hole("BORE", dwell, " F 100000", true), &json!({"SpindleDwellMs": 100}));
        clean(&out);
        let b = bodies(&out.program);
        let floor = b.iter().position(|l| *l == "G1Z-6.000F100.0").expect("bore floor");
        let stop = floor + b[floor..].iter().position(|l| *l == "M05").expect("stop at floor");
        let restart = stop + b[stop..].iter().position(|l| *l == "M03S8000").expect("restart after retract");
        assert!(b[stop..restart].contains(&"G00Z2.000"), "{}", out.program);
        assert_eq!(b[restart + 1], "G04P100", "{}", out.program);
        if dwell > 0 {
            assert_eq!(b[floor + 1], "G04P500", "{}", out.program);
        }
        assert!(!out.program.contains("G86") && !out.program.contains("G89"), "{}", out.program);
    }
}

#[test]
fn drill_dwell_waits_for_floor_and_is_not_silently_lost() {
    let out = run(&hole("DRILL", 500, " F 100000", true), &Value::Null);
    clean(&out);
    let b = bodies(&out.program);
    let floor = b.iter().position(|l| *l == "G1Z-6.000F100.0").expect("drill floor");
    assert_eq!(b[floor + 1], "G04P500", "{}", out.program);
    refused(&hole("DRILL", 500, " F 100000", false), &["dwell", "floor"]);
}

#[test]
fn missing_cycle_feed_is_refused_by_name() {
    refused(&hole("DRILL", 0, "", true), &["feed"]);
}

#[test]
fn invalid_spindle_and_tilt_and_unknown_feed_start_are_refused() {
    for rpm in [0, 24001] {
        refused(&program(CUT).replace("SpindleSpeed 8000", &format!("SpindleSpeed {rpm}")), &["spindle", "rpm"]);
    }
    refused(
        &program(CUT).replace("Rapid X 10000 Y 0 Z 5000", "Rapid X 10000 Y 0 Z 5000 A 1000 B 0 C 0"),
        &["rotary", "tilt", "3-axis"],
    );
    refused(&program("Feed X 10000 Y 0 Z 0 F 300000\n"), &["start", "rapid", "position"]);
}

mod richauto_safety;
