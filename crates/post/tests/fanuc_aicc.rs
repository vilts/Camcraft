//! #2603: the Fanuc post's AI contour control option. Off by default;
//! on, each operation writes `G05.1 Q1 R<level>` after its preamble and
//! `G05.1 Q0` at its end, before the next tool change.
//!
//! Provenance: Fanuc G05.1 Q1 R / Q0 form from the machine owner's own
//! programs (31i-A5 Robodrill, issue #2603) and the Doosan DNM manual
//! cited in assets/posts/doosan-dnm.cps (Q0 before canned cycles).
//!
//! Gate map: covers assets/posts/fanuc.cps, camcraft_post::post::engine.

use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn read(rel: &str) -> String {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn vmc() -> Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("vmc machine json")
}

/// Two milling operations with their own tools, then a drill cycle.
const STREAM: &str = r#"; HSMW-CL v1 scale=0.001mm moves=14
PARAM WorkOffset 1
TOOL 12 "FACE D10"
PARAM SpindleSpeed 5800
PARAM OperationName Face
PARAM ClearancePlane 50
PARAM RetractPlane 10
SECTION
Rapid 0 0 10000
Feed 0 0 0 F300000
Feed 50000 0 0 F1200000
Rapid 50000 0 10000
ENDSECTION
TOOL 13 "EM D6"
PARAM SpindleSpeed 9500
PARAM OperationName Edge
PARAM ClearancePlane 50
PARAM RetractPlane 10
SECTION
Rapid 0 0 10000
Feed 0 0 -2000 F300000
Feed 40000 0 -2000 F1200000
Rapid 40000 0 10000
ENDSECTION
TOOL 14 "DRILL D6"
PARAM SpindleSpeed 7500
PARAM OperationName Peck
PARAM ClearancePlane 50
PARAM RetractPlane 10
SECTION
Rapid 20000 10000 10000
Rapid 20000 10000 2000
CYCLE DRILL X20000 Y10000 Z-5000 R2000 F200000
Plunge 20000 10000 -5000 F200000
Rapid 20000 10000 2000
Rapid 20000 10000 10000
ENDSECTION
"#;

fn post(options: Value) -> (String, Vec<String>) {
    let out = engine::run_with(&read("assets/posts/fanuc.cps"), &vmc(), STREAM, &options).expect("posts");
    (out.program, out.warnings)
}

fn idx(lines: &[&str], from: usize, pat: &str) -> usize {
    lines[from..].iter().position(|l| l.contains(pat)).map(|i| i + from).unwrap_or(usize::MAX)
}

#[test]
fn fanuc_aicc_is_off_by_default() {
    let (p, w) = post(json!({}));
    assert!(w.is_empty(), "{w:?}");
    assert!(!p.contains("G05.1"), "no G05.1 unless asked:\n{p}");
}

#[test]
fn fanuc_aicc_on_brackets_each_operation() {
    let (p, w) = post(json!({ "aiContourControl": true }));
    assert!(w.is_empty(), "{w:?}\n{p}");
    let lines: Vec<&str> = p.lines().collect();
    // Two milling ops: one on and one off each, the default level R5.
    assert_eq!(lines.iter().filter(|l| l.trim() == "G05.1 Q1 R5").count(), 3, "{p}");
    for tool in ["T12 M6", "T13 M6"] {
        let t = idx(&lines, 0, tool);
        let on = idx(&lines, t, "G05.1 Q1");
        let first_move = idx(&lines, t, "G00 ");
        let off = idx(&lines, on, "G05.1 Q0");
        let stop = idx(&lines, on, "M5");
        assert!(t < on && on < first_move, "{tool}: on after the preamble, before the first move:\n{p}");
        assert!(off < stop, "{tool}: off before the spindle stop and the next tool:\n{p}");
    }
    // The drill op: AICC is off before the canned cycle.
    let t = idx(&lines, 0, "T14 M6");
    let cycle = idx(&lines, t, "G81");
    let off = idx(&lines, t, "G05.1 Q0");
    assert!(off < cycle, "G05.1 Q0 before the canned cycle:\n{p}");
    assert_eq!(lines.iter().filter(|l| l.contains("G05.1 Q0")).count(), 3, "one off per on:\n{p}");
}

#[test]
fn fanuc_aicc_level_is_the_option() {
    let (p, w) = post(json!({ "aiContourControl": true, "aiContourLevel": 1 }));
    assert!(w.is_empty(), "{w:?}");
    assert!(p.contains("G05.1 Q1 R1\n") && !p.contains("R5\n"), "{p}");
}

/// A touch probe section (spindle stopped, G31 skip) never arms AICC.
const PROBE: &str = "; HSMW-CNC v1
PARAM StartX 10.000000
PARAM StartY 10.000000
PARAM StartZ 10.000000
PARAM OperationName Touch
PARAM ToolKind touch probe
PARAM SpindleSpeed 0
PARAM WorkOffset 1
PARAM ClearancePlane 10.000000
PARAM RetractPlane 3.000000
TOOL 1 PROBE
SECTION
GUARD 10000 10000 5000 F 300000
TOUCH 10000 10000 -5000 EXPECT 10000 10000 0 F 30000 M -
RETREAT 0 0 1000 EXPECT 10000 10000 1000 F 300000
ENDSECTION
";

#[test]
fn fanuc_aicc_skips_a_probe_section() {
    let opts = json!({ "aiContourControl": true });
    let out = engine::run_with(&read("assets/posts/fanuc.cps"), &vmc(), PROBE, &opts).expect("posts");
    assert!(out.program.contains("G31"), "the probe posts:\n{}", out.program);
    assert!(!out.program.contains("G05.1"), "no AICC around G31:\n{}", out.program);
}

#[test]
fn fanuc_aicc_level_out_of_range_is_refused_by_name() {
    let (p, w) = post(json!({ "aiContourControl": true, "aiContourLevel": 11 }));
    assert_eq!(w.len(), 1, "one refusal, not one per operation: {w:?}");
    assert!(w[0].contains("AI contour control level 11"), "{w:?}");
    assert!(!p.contains("G05.1"), "{p}");
}

#[test]
fn fanuc_aicc_on_replaces_the_smoothing_not_written_note() {
    let smoothed = STREAM.replace("PARAM OperationName Edge", "PARAM OperationName Edge\nPARAM Smoothing 0.01");
    let run = |o: Value| engine::run_with(&read("assets/posts/fanuc.cps"), &vmc(), &smoothed, &o).expect("posts").program;
    assert!(run(json!({})).contains("SMOOTHING NOT WRITTEN"), "off: the ask is named");
    let on = run(json!({ "aiContourControl": true }));
    assert!(!on.contains("SMOOTHING NOT WRITTEN") && on.contains("G05.1 Q1 R5"), "{on}");
}

#[test]
fn fanuc_aicc_options_are_declared_off_and_bounded() {
    use camcraft_post::post::options::{declared_options, validate_overrides, OptionValue};
    use std::collections::BTreeMap;
    let schema = declared_options(&read("assets/posts/fanuc.cps")).expect("schema");
    let get = |k: &str| schema.iter().find(|o| o.key == k).unwrap_or_else(|| panic!("{k} declared"));
    assert_eq!(get("aiContourControl").default, OptionValue::Bool(false));
    assert_eq!(get("aiContourLevel").default, OptionValue::Int(5));
    let mut ov = BTreeMap::new();
    ov.insert("aiContourLevel".to_string(), OptionValue::Int(11));
    assert!(validate_overrides(&schema, &ov).is_err(), "the host refuses R11 too");
}
