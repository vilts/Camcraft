//! Provenance: clean-room 3-axis tests for Mach4 Mill postprocessor (issue #1968),
//! specced from public Mach4 Mill GCode Programming / RS-274/NGC docs.
//!
//! Gate map (M4.4): covers assets/posts/mach4.cps, post::post (engine).

use camcraft_post::post::engine;
use serde_json::json;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn mach4_post() -> String {
    read("assets/posts/mach4.cps")
}

fn default_vmc() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("vmc machine json")
}

#[test]
fn mach4_3axis_mm_and_inch_dialect_words() {
    let cps = mach4_post();
    let mch = default_vmc();

    let stream = "\
PARAM WorkOffset 8
PARAM Coolant flood
PARAM AfterOperation optional-stop
PARAM ClearancePlane 25
PARAM RetractPlane 5
TOOL 3 \"10MM 4FL EM\"
PARAM ToolKind Flat Endmill
PARAM ToolDiameter 10
PARAM SpindleSpeed 10000
SECTION ContourAndPlanes
Rapid X 0 Y 0 Z 10000
Plunge X 0 Y 0 Z -2000 F 300000
COMP LEFT 5000
Feed X 10000 Y 0 Z -2000 F 800000
COMP OFF
Feed X 20000 Y 0 Z -2000 F 800000
Arc CW X 20000 Y 10000 Z -2000 F 600000 I 20000 J 5000
Arc CCW X 30000 Y 10000 Z -2000 F 600000 I 25000 K -2000
Arc CW X 30000 Y 20000 Z -2000 F 600000 J 15000 K -2000
Arc CW X 30000 Y 30000 Z -2000 F 600000 I 30000 J 25000
ENDSECTION
SECTION Cycles
PARAM WorkOffset G59
PARAM Coolant mist
PARAM ClearancePlane 25
PARAM RetractPlane 5
Rapid X 40000 Y 20000 Z 5000
CYCLE DRILL X 40000 Y 20000 R 2000 Z -8000 P 500 F 200000
Rapid X 40000 Y 20000 Z 2000
Plunge X 40000 Y 20000 Z -8000 F 200000
Rapid X 40000 Y 20000 Z 2000
CYCLE PECK X 50000 Y 20000 R 2000 Z -15000 Q 4000 F 240000
Rapid X 50000 Y 20000 Z 2000
Plunge X 50000 Y 20000 Z -15000 F 240000
Rapid X 50000 Y 20000 Z 2000
CYCLE DEEP X 60000 Y 20000 R 2000 Z -20000 Q 5000 F 180000
Rapid X 60000 Y 20000 Z 2000
Plunge X 60000 Y 20000 Z -20000 F 180000
Rapid X 60000 Y 20000 Z 2000
CYCLE REAM X 70000 Y 20000 R 2000 Z -10000 F 150000
Rapid X 70000 Y 20000 Z 2000
Plunge X 70000 Y 20000 Z -10000 F 150000
Rapid X 70000 Y 20000 Z 2000
CYCLE BORE X 80000 Y 20000 R 2000 Z -10000 F 120000
Rapid X 80000 Y 20000 Z 2000
Plunge X 80000 Y 20000 Z -10000 F 120000
Rapid X 80000 Y 20000 Z 2000
CYCLE BACKBORE X 90000 Y 20000 R 2000 Z -10000 P 250 F 100000
Rapid X 90000 Y 20000 Z 2000
Plunge X 90000 Y 20000 Z -10000 F 100000
Rapid X 90000 Y 20000 Z 2000
ENDSECTION
SECTION Tapping
PARAM WorkOffset 254
PARAM ClearancePlane 25
PARAM RetractPlane 5
TOOL 4 \"M6X1 TAP\"
PARAM SpindleSpeed 600
Rapid X 100000 Y 20000 Z 5000
CYCLE TAP X 100000 Y 20000 R 2000 Z -12000 F 600000
Rapid X 100000 Y 20000 Z 2000
Feed X 100000 Y 20000 Z -12000 F 600000
Feed X 100000 Y 20000 Z 2000 F 600000
ENDSECTION
";
    let out_mm = engine::run(&cps, &mch, stream).expect("mach4 mm runs");
    assert!(out_mm.warnings.is_empty(), "warnings: {:?}", out_mm.warnings);
    let p = &out_mm.program;

    assert!(p.contains("G21"), "expected G21 in mm program:\n{p}");
    assert!(p.contains("G90"), "expected G90:\n{p}");
    assert!(p.contains("G94"), "expected G94:\n{p}");
    assert!(p.contains("G91.1"), "expected G91.1 incremental IJK mode:\n{p}");
    assert!(p.contains("G64"), "expected G64 constant velocity mode:\n{p}");
    assert!(!p.contains("G43.4"), "3-axis must not emit G43.4:\n{p}");
    assert!(!p.contains("G93"), "3-axis must not emit G93:\n{p}");

    assert!(p.contains("G54.1 P2"), "expected G54.1 P2:\n{p}");
    assert!(p.contains("G59"), "expected G59:\n{p}");
    assert!(p.contains("G54.1 P248"), "expected G54.1 P248:\n{p}");
    assert!(p.contains("T3 M6"), "expected T3 M6:\n{p}");
    assert!(p.contains("G43 H3"), "expected G43 H3:\n{p}");
    assert!(p.contains("G41") && p.contains("D3") && p.contains("G40"), "expected G41 D3 / G40:\n{p}");
    assert!(p.contains("G02 X20. Y10.") && p.contains("I0.") && p.contains("J5."), "expected initial XY arc:\n{p}");
    assert!(p.contains("G18 G03") && p.contains("I5.") && p.contains("K0."), "expected G18 arc:\n{p}");
    assert!(p.contains("G19 G02") && p.contains("J5.") && p.contains("K0."), "expected G19 arc:\n{p}");
    assert!(p.contains("G17 G02 X30. Y30."), "expected G17 restore arc:\n{p}");

    assert!(p.contains("G82") && p.contains("P0.5"), "expected G82 with P0.5s dwell:\n{p}");
    assert!(p.contains("G73") && p.contains("Q4."), "expected G73:\n{p}");
    assert!(p.contains("G83") && p.contains("Q5."), "expected G83:\n{p}");
    assert!(p.contains("G85"), "expected G85:\n{p}");
    assert!(
        p.lines().any(|line| line.split_whitespace().any(|w| w == "G86")
            && !line.split_whitespace().any(|w| w.starts_with('P'))),
        "expected G86 without a dwell word:\n{p}"
    );
    assert!(p.contains("G89") && p.contains("P0.25"), "expected G89 with P0.25s dwell:\n{p}");
    assert!(p.contains("G84"), "expected G84:\n{p}");
    assert!(p.contains("G80"), "expected G80:\n{p}");
    assert!(p.contains("M8") && p.contains("M7") && p.contains("M9"), "expected flood/mist/off:\n{p}");
    assert!(p.contains("M1"), "expected optional stop M1:\n{p}");
    assert!(p.contains("M30"), "expected M30:\n{p}");

    let out_in = engine::run_with(&cps, &mch, stream, &json!({ "Units": "inch", "safeRetract": "G53" }))
        .expect("mach4 inch runs");
    assert!(out_in.warnings.is_empty(), "inch warnings: {:?}", out_in.warnings);
    assert!(out_in.program.contains("G20"), "expected G20 in inch program:\n{}", out_in.program);
    assert!(out_in.program.contains("G53 G00 Z0."), "expected G53 safe retract:\n{}", out_in.program);
}

#[test]
fn mach4_g31_probing_emits_g31_and_refuses_coolant() {
    let cps = mach4_post();
    let mch = default_vmc();
    let stream = "\
PARAM WorkOffset G54
PARAM Coolant flood
PARAM ClearancePlane 25
PARAM RetractPlane 10
TOOL 99 \"3D TOUCH PROBE\"
PARAM ToolKind Touch Probe
PARAM SpindleSpeed 0
SECTION ProbeZ
GUARD 0 0 10000 F 1200000
TOUCH 0 0 -5000 EXPECT 0 0 0 F 100000 M 0
RETREAT 0 0 5000 EXPECT 0 0 5000 F 1200000
FOUND SET 0 TOL 500 AXIS Z 0 0
ENDSECTION
";
    let out = engine::run(&cps, &mch, stream).expect("mach4 probing runs");
    assert!(out.warnings.is_empty(), "probe warnings: {:?}", out.warnings);
    let p = &out.program;
    assert!(p.contains("M5"), "probe must keep spindle stopped:\n{p}");
    assert!(!p.contains("M8") && !p.contains("M7"), "probe must not enable coolant:\n{p}");
    assert!(p.contains("G31"), "expected G31 probing:\n{p}");
    assert!(p.contains("#5063"), "expected #5063 Z trip register:\n{p}");
    assert!(p.contains("G10 L2 P1"), "expected G10 L2 P1 offset update:\n{p}");
}

#[test]
fn mach4_safe_retract_g53_requires_machine_z_and_supports_g30() {
    let cps = mach4_post();
    let mch_no_retract = json!({
        "name": "NoRetractBench",
        "axes": ["X", "Y", "Z"],
        "spindleMax": 12000,
        "coolants": [{ "name": "Flood", "mCode": "M8" }],
        "rigidTapping": true
    });
    let stream = "\
PARAM WorkOffset G54
PARAM ClearancePlane 20
TOOL 1 \"6MM EM\"
PARAM SpindleSpeed 8000
SECTION Cut
Rapid X 0 Y 0 Z 10000
Feed X 10000 Y 0 Z -1000 F 500000
ENDSECTION
";
    let out_g53 = engine::run_with(&cps, &mch_no_retract, stream, &json!({ "safeRetract": "G53" }))
        .expect("runs");
    assert!(
        out_g53.warnings.iter().any(|w| w.contains("NoRetractBench") && w.contains("safeRetractZ")),
        "missing safeRetractZ on named machine must be refused by name under G53, got: {:?}",
        out_g53.warnings
    );

    let out_g30 = engine::run_with(&cps, &mch_no_retract, stream, &json!({ "safeRetract": "G30" }))
        .expect("runs G30");
    assert!(out_g30.warnings.is_empty(), "G30 warnings: {:?}", out_g30.warnings);
    assert!(out_g30.program.contains("G91 G30 Z0."), "expected G91 G30 Z0.:\n{}", out_g30.program);
}

fn cycle_stream(cycle: &str, dwell: i32) -> String {
    format!(
        "PARAM WorkOffset G54\nPARAM ClearancePlane 25\nPARAM RetractPlane 5\n\
         TOOL 1 \"6MM TOOL\"\nPARAM SpindleSpeed 600\nSECTION Cycle\n\
         Rapid X 0 Y 0 Z 5000\n\
         CYCLE {cycle} X 0 Y 0 R 2000 Z -10000 Q 2000 P {dwell} F 120000\n\
         Rapid X 0 Y 0 Z 2000\nPlunge X 0 Y 0 Z -10000 F 120000\n\
         Rapid X 0 Y 0 Z 2000\nENDSECTION\n"
    )
}

#[test]
fn mach4_refuses_dwell_on_g73_and_g84_by_name() {
    assert_unsupported_cycle_dwells(&[("PECK", "G73"), ("TAP", "G84")]);
}

#[test]
fn mach4_refuses_dwell_on_g83_g85_and_g86_by_name() {
    assert_unsupported_cycle_dwells(&[("DEEP", "G83"), ("REAM", "G85"), ("BORE", "G86")]);
}

fn assert_unsupported_cycle_dwells(cycles: &[(&str, &str)]) {
    let cps = mach4_post();
    let mch = default_vmc();
    for &(cycle, g) in cycles {
        let out = engine::run(&cps, &mch, &cycle_stream(cycle, 250)).expect("cycle runs");
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains(&format!("canned cycle {g} cannot write a dwell on Mach4"))),
            "{cycle} must refuse unsupported dwell by name: {:?}",
            out.warnings
        );
        assert!(
            !out.program
                .lines()
                .any(|line| line.split_whitespace().any(|w| w == g)),
            "refused {g} must not be emitted:\n{}",
            out.program
        );
        let zero = engine::run(&cps, &mch, &cycle_stream(cycle, 0)).expect("zero dwell runs");
        assert!(
            zero.warnings.is_empty(),
            "{cycle} zero dwell: {:?}",
            zero.warnings
        );
        let line = zero
            .program
            .lines()
            .find(|line| line.split_whitespace().any(|w| w == g))
            .unwrap_or_else(|| panic!("missing {g} for zero dwell:\n{}", zero.program));
        assert!(
            !line.split_whitespace().any(|w| w.starts_with('P')),
            "{g} must not write P: {line}"
        );
    }
}

#[test]
fn mach4_supported_cycle_dwells_use_seconds_on_g82_and_g89_only() {
    let cps = mach4_post();
    let mch = default_vmc();
    for (cycle, g) in [("DRILL", "G82"), ("SPOT", "G82"), ("BACKBORE", "G89")] {
        let out = engine::run(&cps, &mch, &cycle_stream(cycle, 250)).expect("dwell cycle runs");
        assert!(out.warnings.is_empty(), "{cycle}: {:?}", out.warnings);
        let line = out
            .program
            .lines()
            .find(|line| line.split_whitespace().any(|w| w == g))
            .unwrap_or_else(|| panic!("missing {g}:\n{}", out.program));
        assert!(
            line.split_whitespace().any(|w| w == "P0.25"),
            "{g} needs seconds dwell: {line}"
        );
    }
}
