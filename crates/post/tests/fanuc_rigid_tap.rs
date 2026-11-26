//! #2603: the Fanuc post's rigid tap option. Off by default (a control
//! with parameter 5200 bit 0 set taps rigid on G84 alone); on, each G84
//! block that opens a cycle group has `M29 S<rpm>` on the block before it.
//! A bare X Y hole inside an open group needs none: rigid mode holds to G80.
//!
//! Provenance: Fanuc Series 30i/31i/32i-B operator's manual (B-64604EN-2),
//! rigid tapping: M29 S before G84, rigid mode ends at G80.
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

/// A tap section with three holes (the third at a new depth), then a
/// drill section (no M29).
const STREAM: &str = r#"; HSMW-CL v1 scale=0.001mm moves=20
PARAM WorkOffset 1
TOOL 15 "TAP M6"
PARAM SpindleSpeed 600
PARAM OperationName Tap
PARAM ClearancePlane 50
PARAM RetractPlane 10
SECTION
Rapid 14000 14000 10000
Rapid 14000 14000 2000
CYCLE TAP X14000 Y14000 Z-13000 R2000 F600000
Plunge 14000 14000 -13000 F600000
Rapid 14000 14000 2000
CYCLE TAP X30000 Y14000 Z-13000 R2000 F600000
Rapid 30000 14000 2000
Plunge 30000 14000 -13000 F600000
Rapid 30000 14000 2000
Rapid 30000 14000 10000
Rapid 50000 14000 10000
Rapid 50000 14000 2000
CYCLE TAP X50000 Y14000 Z-8000 R2000 F600000
Plunge 50000 14000 -8000 F600000
Rapid 50000 14000 2000
Rapid 50000 14000 10000
ENDSECTION
TOOL 14 "DRILL D6"
PARAM SpindleSpeed 7500
PARAM OperationName Drill
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

#[test]
fn fanuc_rigid_tap_m29_is_off_by_default() {
    let (p, w) = post(json!({}));
    assert!(w.is_empty(), "{w:?}");
    assert!(p.contains("G84 "), "the tap posts:\n{p}");
    assert!(!p.contains("M29"), "no M29 unless asked:\n{p}");
}

#[test]
fn fanuc_rigid_tap_m29_precedes_each_g84_group() {
    let (p, w) = post(json!({ "rigidTapM29": true }));
    assert!(w.is_empty(), "{w:?}\n{p}");
    let lines: Vec<&str> = p.lines().map(str::trim).collect();
    let g84: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.starts_with("G84 ")).map(|(i, _)| i).collect();
    // The hop law (#994) ends each group with G80, which ends rigid mode, so
    // each of the three holes opens its own G84 group.
    assert_eq!(g84.len(), 3, "three G84 groups:\n{p}");
    for &i in &g84 {
        assert_eq!(lines[i - 1], "M29 S600", "M29 S<rpm> on the block right before G84:\n{p}");
    }
    assert_eq!(lines.iter().filter(|l| l.starts_with("M29")).count(), 3, "one M29 per G84 group, none on the drill:\n{p}");
}

/// A section with no spindle speed (a touch probe's 0 rpm) that asks a
/// tap: the cycle is not posted with `M29 S0`.
const NO_RPM: &str = r#"; HSMW-CL v1 scale=0.001mm moves=4
PARAM WorkOffset 1
PARAM ToolKind touch probe
PARAM SpindleSpeed 0
PARAM OperationName Tap
PARAM ClearancePlane 50
PARAM RetractPlane 10
TOOL 1 "PROBE"
SECTION
Rapid 14000 14000 10000
Rapid 14000 14000 2000
CYCLE TAP X14000 Y14000 Z-13000 R2000 F600000
Rapid 14000 14000 10000
ENDSECTION
"#;

#[test]
fn fanuc_rigid_tap_m29_without_spindle_speed_is_refused_by_name() {
    let opts = json!({ "rigidTapM29": true });
    let out = engine::run_with(&read("assets/posts/fanuc.cps"), &vmc(), NO_RPM, &opts).expect("posts");
    assert!(out.warnings.iter().any(|w| w.contains("rigid tap M29 needs the section's spindle speed")), "{:?}\n{}", out.warnings, out.program);
    assert!(!out.program.contains("M29") && !out.program.contains("G84"), "{}", out.program);
}

#[test]
fn fanuc_rigid_tap_m29_option_is_declared_off() {
    use camcraft_post::post::options::{declared_options, OptionValue};
    let schema = declared_options(&read("assets/posts/fanuc.cps")).expect("schema");
    let o = schema.iter().find(|o| o.key == "rigidTapM29").expect("rigidTapM29 declared");
    assert_eq!(o.default, OptionValue::Bool(false));
}
