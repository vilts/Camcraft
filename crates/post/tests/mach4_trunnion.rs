//! Provenance: clean-room 3+2 XYZAC trunnion, rollover, clamp, RTCP guard,
//! and simultaneous 5-axis refusal tests for Mach4 Mill (issue #1968).
//!
//! Gate map (M4.4): covers assets/posts/mach4.cps, post::kinematics, post::machine.

use camcraft_post::kinematics::{
    forward_part, AcTrunnionConfig, MachinePose, RotaryAxisConfig, RotarySignConvention,
};
use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine};
use serde_json::{json, Value};
use std::f64::consts::PI;
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

fn default_vmc() -> Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("vmc machine json")
}

struct TrunnionSpec<'a> {
    unit: &'a str,
    rtcp: Option<bool>,
    sign_conv: &'a str,
    a_zero: f64,
    c_zero: f64,
    c_rollover: bool,
    pivot: [f64; 3],
    offset: [f64; 3],
}

fn trunnion_mch_json(spec: &TrunnionSpec<'_>) -> String {
    let mut controller = json!({
        "synced_configuration": { "parts": {
            "X": { "coordinate": 0, "max_normal_speed": 4000.0, "max_rapid_speed": 8000.0 },
            "Y": { "coordinate": 1, "max_normal_speed": 4000.0, "max_rapid_speed": 8000.0 },
            "Z": { "coordinate": 2, "max_normal_speed": 3000.0, "max_rapid_speed": 6000.0 },
            "A": { "coordinate": 3, "max_normal_speed": 1500.0, "max_rapid_speed": 3000.0 },
            "C": { "coordinate": 4, "max_normal_speed": 2000.0, "max_rapid_speed": 4000.0 }
        } }
    });
    if let Some(flag) = spec.rtcp {
        controller["rtcp"] = json!(flag);
    }
    json!({
        "general": { "model": "MACH4 AC TRUNNION", "vendor": "Camcraft", "kind": "mill" },
        "kinematics": { "default": {
            "units": { "length": spec.unit, "angle": "degrees" },
            "sign_convention": spec.sign_conv,
            "parts": [{ "id": "base", "name": "table", "type": "table", "parts": [
                {
                    "id": "a_axis", "name": "A", "type": "rotary", "axis": [1.0, 0.0, 0.0],
                    "pivot": spec.pivot, "travel": { "min": -110.0, "max": 30.0 },
                    "zero_offset": spec.a_zero, "rollover": false,
                    "parts": [{
                        "id": "c_axis", "name": "C", "type": "rotary", "axis": [0.0, 0.0, 1.0],
                        "offset": spec.offset, "travel": { "min": -720.0, "max": 720.0 },
                        "zero_offset": spec.c_zero, "rollover": spec.c_rollover
                    }]
                },
                { "id": "x", "name": "X", "type": "linear", "travel": { "min": -300.0, "max": 300.0 },
                  "parts": [{ "id": "y", "name": "Y", "type": "linear", "travel": { "min": -200.0, "max": 200.0 } }] },
                { "id": "z", "name": "Z", "type": "linear", "travel": { "min": -250.0, "max": 0.0 }, "safe_retract": 0.0,
                  "parts": [{ "id": "head", "name": "head", "type": "head",
                    "spindle": { "min_speed": 1000.0, "max_speed": 18000.0 },
                    "tool_station": { "coolants": [{ "name": "Flood", "mCode": "M8" }, { "name": "Mist", "mCode": "M7" }] }
                  }] }
            ] }]
        } },
        "controller": controller,
        "tooling": { "number_of_tools": 16 }
    })
    .to_string()
}

fn static_rotaries_mch(name: &str, rotaries: Value) -> Value {
    json!({
        "name": name,
        "safeRetractZ": 0.0,
        "rtcp": false,
        "kinematics": { "static": { "rotaries": rotaries } }
    })
}

fn stream_axis_from_abc(a_deg: f64, b_deg: f64, c_deg: f64) -> [f64; 3] {
    let (sa, ca) = (a_deg * PI / 180.0).sin_cos();
    let (sb, cb) = (b_deg * PI / 180.0).sin_cos();
    let (sc, cc) = (c_deg * PI / 180.0).sin_cos();
    [ca * sb * cc + sa * sc, ca * sb * sc - sa * cc, ca * cb]
}

fn norm3(v: [f64; 3]) -> f64 { (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() }
fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }

fn word_value(line: &str, letter: char) -> Option<f64> {
    let line = line.split('(').next().unwrap_or("");
    for tok in line.split_whitespace() {
        if let Some(rest) = tok.strip_prefix(letter)
            && let Ok(v) = rest.parse::<f64>()
        {
            return Some(v);
        }
    }
    None
}

#[test]
fn mach4_3plus2_indexed_ac_trunnion_round_trips_kinematics() {
    let cps = mach4_post();
    let pivot = [120.0, -40.0, -150.0];
    let offset = [0.0, 12.5, 28.0];
    let a_zero = 10.0;
    let c_zero = -45.0;

    let stream = "\
PARAM WorkOffset G54
PARAM ClearancePlane 50
PARAM RetractPlane 10
PARAM OriginX 0
PARAM OriginY 0
PARAM OriginZ 0
TOOL 2 \"8MM BALL\"
PARAM SpindleSpeed 9000
SECTION VerticalTop
Rapid X 10000 Y 5000 Z 15000
Feed X 10000 Y 5000 Z 0 F 400000
Feed X 20000 Y 5000 Z 0 F 400000
ENDSECTION
SECTION IndexedSide45
PARAM ClearancePlane 50
PARAM RetractPlane 10
Rapid X 0 Y 20000 Z 30000 A -45000 B 0 C 0
Feed X 10000 Y 20000 Z 25000 F 500000 A -45000 B 0 C 0
Feed X 20000 Y 20000 Z 25000 F 500000 A -45000 B 0 C 0
ENDSECTION
SECTION IndexedRight90Arc
PARAM ClearancePlane 50
PARAM RetractPlane 10
Rapid X 15000 Y 10000 Z 5000 A 0 B -90000 C 0
Feed X 15000 Y 0 Z 5000 F 500000 A 0 B -90000 C 0
Arc CW X 15000 Y 0 Z 15000 F 500000 A 0 B -90000 C 0 J 0 K 10000
ENDSECTION
SECTION BackToVertical
PARAM ClearancePlane 50
PARAM RetractPlane 10
Rapid X 0 Y 0 Z 20000
Feed X 5000 Y 5000 Z 0 F 300000
ENDSECTION
";

    for (sign, unit) in [("table_rh", "mm"), ("tool_rh", "mm"), ("table_rh", "inch")] {
        let scale = if unit == "inch" { 25.4 } else { 1.0 };
        let spec = TrunnionSpec {
            unit,
            rtcp: Some(false),
            sign_conv: sign,
            a_zero,
            c_zero,
            c_rollover: false,
            pivot: [pivot[0] / scale, pivot[1] / scale, pivot[2] / scale],
            offset: [offset[0] / scale, offset[1] / scale, offset[2] / scale],
        };
        let parsed = parse_mch(&trunnion_mch_json(&spec)).expect("trunnion parses");
        let mch = machine::mirror(&parsed);
        let opts = if unit == "inch" { json!({ "Units": "inch" }) } else { Value::Null };
        let out = engine::run_with(&cps, &mch, stream, &opts).expect("3+2 runs");
        assert!(out.warnings.is_empty(), "{sign}/{unit} warnings: {:?}", out.warnings);
        let p = &out.program;
        assert!(p.contains(if unit == "inch" { "G20" } else { "G21" }), "unit code missing:\n{p}");
        assert!(!p.contains("G43.4") && !p.contains("G93"), "3+2 Mach4 must use G94 without G43.4/G93:\n{p}");

        let cfg = AcTrunnionConfig {
            a: RotaryAxisConfig {
                name: "A".into(),
                axis: [1.0, 0.0, 0.0],
                travel: Some((-110.0, 30.0)),
                zero_offset_deg: a_zero,
                rollover: false,
            },
            c: RotaryAxisConfig {
                name: "C".into(),
                axis: [0.0, 0.0, 1.0],
                travel: Some((-720.0, 720.0)),
                zero_offset_deg: c_zero,
                rollover: false,
            },
            spindle_axis: [0.0, 0.0, 1.0],
            a_pivot: pivot,
            c_offset: offset,
            wcs_offset: [0.0, 0.0, 0.0],
            work_datum: [pivot[0] + offset[0], pivot[1] + offset[1], pivot[2] + offset[2]],
            sign: if sign == "table_rh" { RotarySignConvention::TableRightHand } else { RotarySignConvention::ToolRightHand },
            pole_sin_tol: 1e-6,
            weight_a: 1.0,
            weight_c: 1.0,
            initial_c_deg: 0.0,
        };

        let (mut cur_a, mut cur_c) = (a_zero, c_zero);
        let (mut cur_x, mut cur_y, mut cur_z) = (0.0, 0.0, 0.0);
        let mut feed_points = Vec::new();
        for line in p.lines() {
            if let Some(a) = word_value(line, 'A') { cur_a = a; }
            if let Some(c) = word_value(line, 'C') { cur_c = c; }
            let has_xyz = word_value(line, 'X').is_some() || word_value(line, 'Y').is_some() || word_value(line, 'Z').is_some();
            if let Some(x) = word_value(line, 'X') { cur_x = x * scale; }
            if let Some(y) = word_value(line, 'Y') { cur_y = y * scale; }
            if let Some(z) = word_value(line, 'Z') { cur_z = z * scale; }
            if has_xyz && (line.starts_with("G01 ") || line.starts_with("G02 ") || line.starts_with("G03 ")) {
                feed_points.push(([cur_x, cur_y, cur_z], cur_a, cur_c));
            }
        }

        let expected = [
            ([10.0, 5.0, 0.0], [0.0, 0.0, 1.0]),
            ([20.0, 5.0, 0.0], [0.0, 0.0, 1.0]),
            ([10.0, 20.0, 25.0], stream_axis_from_abc(-45.0, 0.0, 0.0)),
            ([20.0, 20.0, 25.0], stream_axis_from_abc(-45.0, 0.0, 0.0)),
            ([15.0, 0.0, 5.0], stream_axis_from_abc(0.0, -90.0, 0.0)),
            ([15.0, 0.0, 15.0], stream_axis_from_abc(0.0, -90.0, 0.0)),
            ([5.0, 5.0, 0.0], [0.0, 0.0, 1.0]),
        ];
        assert_eq!(feed_points.len(), expected.len(), "feed count in {sign}/{unit}:\n{p}");
        let tip_tol = if unit == "inch" { 5e-3 } else { 2e-3 };
        for (i, ((xyz, a, c), (want_tip, want_axis))) in feed_points.iter().zip(expected.iter()).enumerate() {
            let rec = forward_part(&cfg, MachinePose { xyz: *xyz, a_deg: *a, c_deg: *c }).expect("forward_part");
            assert!(norm3(sub3(rec.tip, *want_tip)) <= tip_tol, "{sign}/{unit} pt {i}: {:?} vs {:?}", rec.tip, want_tip);
            assert!(dot3(rec.axis, *want_axis) > 1.0 - 1e-6, "{sign}/{unit} axis {i}: {:?} vs {:?}", rec.axis, want_axis);
        }
        assert!(p.contains("G02 ") || p.contains("G03 "), "rotated YZ arc missing:\n{p}");
    }
}

#[test]
fn mach4_rotary_rollover_unwrapped_vs_360_and_clamp_order() {
    let cps = mach4_post();
    let spec = TrunnionSpec {
        unit: "mm",
        rtcp: Some(false),
        sign_conv: "table_rh",
        a_zero: 0.0,
        c_zero: 0.0,
        c_rollover: true,
        pivot: [0.0, 0.0, 0.0],
        offset: [0.0, 0.0, 0.0],
    };
    let mch = machine::mirror(&parse_mch(&trunnion_mch_json(&spec)).expect("parses"));
    let stream = "\
PARAM WorkOffset G54
PARAM ClearancePlane 30
TOOL 1 \"6MM EM\"
PARAM SpindleSpeed 8000
SECTION S1
Rapid X 10000 Y 0 Z 10000 A -30000 B 0 C 20000
Feed X 10000 Y 0 Z 0 F 400000 A -30000 B 0 C 20000
ENDSECTION
SECTION S2
PARAM ClearancePlane 30
Rapid X 10000 Y 0 Z 10000 A -30000 B 0 C 140000
Feed X 10000 Y 0 Z 0 F 400000 A -30000 B 0 C 140000
ENDSECTION
SECTION S3
PARAM ClearancePlane 30
Rapid X 10000 Y 0 Z 10000 A -30000 B 0 C 260000
Feed X 10000 Y 0 Z 0 F 400000 A -30000 B 0 C 260000
ENDSECTION
SECTION S4
PARAM ClearancePlane 30
Rapid X 10000 Y 0 Z 10000 A -30000 B 0 C 20000
Feed X 10000 Y 0 Z 0 F 400000 A -30000 B 0 C 20000
ENDSECTION
";
    let out_360 = engine::run_with(&cps, &mch, stream, &json!({ "rolloverMode": "rollover_360" })).expect("rollover_360");
    assert!(out_360.warnings.is_empty(), "{:?}", out_360.warnings);
    let c_vals: Vec<f64> = out_360.program.lines().filter_map(|l| word_value(l, 'C')).collect();
    assert!(!c_vals.is_empty() && c_vals.iter().all(|c| (0.0..360.0).contains(c)), "C out of [0, 360): {c_vals:?}");

    let lines: Vec<&str> = out_360.program.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        if line.contains("G00 ") && (word_value(line, 'A').is_some() || word_value(line, 'C').is_some()) {
            let prev = lines[idx.saturating_sub(1)];
            assert!(prev.contains("G28") || prev.contains("G53") || prev.contains("G30") || prev == "G90");
        }
    }

    let out_unwrap = engine::run_with(&cps, &mch, stream, &json!({ "rolloverMode": "unwrapped" })).expect("unwrapped");
    assert!(out_unwrap.warnings.is_empty(), "{:?}", out_unwrap.warnings);

    let out_clamp = engine::run_with(&cps, &mch, stream, &json!({ "rotaryUnclamp": "M11", "rotaryClamp": "M10" })).expect("clamp");
    assert!(out_clamp.warnings.is_empty(), "{:?}", out_clamp.warnings);
    let cl: Vec<&str> = out_clamp.program.lines().collect();
    let i_u = cl.iter().position(|l| *l == "M11").expect("M11");
    let i_m = cl.iter().position(|l| l.starts_with("G00 ") && l.contains('A')).expect("G00 A");
    let i_c = cl.iter().position(|l| *l == "M10").expect("M10");
    assert!(i_u < i_m && i_m < i_c, "clamp order wrong:\n{}", out_clamp.program);

    let out_partial = engine::run_with(&cps, &mch, stream, &json!({ "rotaryUnclamp": "M11" })).expect("partial");
    assert!(out_partial.warnings.iter().any(|w| w.contains("rotaryClamp") || w.contains("clamp")));
}

#[test]
fn mach4_refuses_rtcp_bad_trunnion_and_simultaneous_by_name() {
    let cps = mach4_post();
    let tilted = "\
PARAM WorkOffset G54
PARAM ClearancePlane 25
TOOL 1 \"6MM EM\"
PARAM SpindleSpeed 8000
SECTION Tilted
Rapid X 0 Y 0 Z 25000 A -30000 B 0 C 0
Feed X 10000 Y 0 Z 0 F 400000 A -30000 B 0 C 0
ENDSECTION
";

    let rtcp_spec = TrunnionSpec {
        unit: "mm", rtcp: Some(true), sign_conv: "table_rh", a_zero: 0.0, c_zero: 0.0,
        c_rollover: false, pivot: [0.0, 0.0, 0.0], offset: [0.0, 0.0, 0.0],
    };
    let mch_rtcp = machine::mirror(&parse_mch(&trunnion_mch_json(&rtcp_spec)).expect("parses"));
    let out_rtcp = engine::run(&cps, &mch_rtcp, tilted).expect("runs");
    assert!(out_rtcp.warnings.iter().any(|w| w.contains("rtcp") || w.contains("RTCP")));
    assert!(!out_rtcp.program.contains("G43.4"));

    assert!(!engine::run(&cps, &default_vmc(), tilted).expect("runs").warnings.is_empty());

    let bad_cases: [(&str, Value, &str); 6] = [
        ("Single4th", json!([{ "letter": "A", "direction": [1, 0, 0], "travel": [-110, 30], "pivot": [0, 0, 0], "turnsPart": true }]), "C"),
        ("InvertedCA", json!([
            { "letter": "C", "direction": [0, 0, 1], "offset": [0, 0, 0], "turnsPart": true },
            { "letter": "A", "direction": [1, 0, 0], "parent": "C", "pivot": [0, 0, 0], "turnsPart": true }
        ]), "parent"),
        ("NoPivot", json!([
            { "letter": "A", "direction": [1, 0, 0], "turnsPart": true },
            { "letter": "C", "parent": "A", "direction": [0, 0, 1], "offset": [0, 0, 0], "turnsPart": true }
        ]), "pivot"),
        ("NoOffset", json!([
            { "letter": "A", "direction": [1, 0, 0], "pivot": [0, 0, 0], "turnsPart": true },
            { "letter": "C", "parent": "A", "direction": [0, 0, 1], "turnsPart": true }
        ]), "offset"),
        ("ParallelAxes", json!([
            { "letter": "A", "direction": [0, 0, 1], "pivot": [0, 0, 0], "turnsPart": true },
            { "letter": "C", "parent": "A", "direction": [0, 0, 1], "offset": [0, 0, 0], "turnsPart": true }
        ]), "parallel"),
        ("HeadRotary", json!([
            { "letter": "A", "direction": [1, 0, 0], "pivot": [0, 0, 0], "turnsPart": false },
            { "letter": "C", "parent": "A", "direction": [0, 0, 1], "offset": [0, 0, 0], "turnsPart": true }
        ]), "turnsPart"),
    ];
    for (name, rots, expect_substr) in bad_cases {
        let out = engine::run(&cps, &static_rotaries_mch(name, rots), tilted).expect("runs");
        assert!(
            out.warnings.iter().any(|w| w.contains(expect_substr)),
            "{name} expected `{expect_substr}` in {:?}",
            out.warnings
        );
    }

    let ok_spec = TrunnionSpec { rtcp: Some(false), ..rtcp_spec };
    let mch_ok = machine::mirror(&parse_mch(&trunnion_mch_json(&ok_spec)).expect("parses"));
    let over_travel = "\
PARAM WorkOffset G54
PARAM ClearancePlane 25
TOOL 1 \"6MM EM\"
PARAM SpindleSpeed 8000
SECTION OverTravel
Rapid X 0 Y 0 Z 25000 A -125000 B 0 C 0
Feed X 10000 Y 0 Z 0 F 400000 A -125000 B 0 C 0
ENDSECTION
";
    let out_ot = engine::run(&cps, &mch_ok, over_travel).expect("runs");
    assert!(out_ot.warnings.iter().any(|w| w.contains("travel") || w.contains("limit")));

    let simultaneous = "\
PARAM WorkOffset G54
PARAM ClearancePlane 25
TOOL 1 \"6MM EM\"
PARAM SpindleSpeed 8000
SECTION Sim5x
Rapid X 0 Y 0 Z 25000 A -30000 B 0 C 0
Feed X 10000 Y 0 Z 0 F 400000 A -30000 B 0 C 0
Feed X 20000 Y 0 Z 0 F 400000 A -45000 B 0 C 30000
ENDSECTION
";
    let out_sim = engine::run(&cps, &mch_ok, simultaneous).expect("runs");
    assert!(out_sim.warnings.iter().any(|w| w.contains("needs #1966: inverse-time feed")));
    assert!(!out_sim.program.contains("A-45") && !out_sim.program.contains("C30"));
}

fn review_trunnion_with_zeros(a_zero: f64, c_zero: f64) -> Value {
    let spec = TrunnionSpec {
        unit: "mm", rtcp: Some(false), sign_conv: "table_rh", a_zero, c_zero,
        c_rollover: false, pivot: [0.0; 3], offset: [0.0; 3],
    };
    machine::mirror(&parse_mch(&trunnion_mch_json(&spec)).expect("review trunnion parses"))
}

fn review_trunnion() -> Value { review_trunnion_with_zeros(0.0, 0.0) }

const VERTICAL_THEN_TILTED: &str = "\
PARAM WorkOffset G54
PARAM ClearancePlane 25
TOOL 1 \"6MM EM\"
PARAM SpindleSpeed 8000
SECTION Vertical
Rapid X 10000 Y 5000 Z 10000
Feed X 10000 Y 5000 Z 0 F 400000
ENDSECTION
SECTION Tilted
PARAM ClearancePlane 25
Rapid X 10000 Y 5000 Z 10000 A -45000 B 0 C 0
Feed X 10000 Y 5000 Z 0 F 400000 A -45000 B 0 C 0
ENDSECTION
";

#[test]
fn mach4_repeated_program_indexes_vertical_before_first_cut() {
    for (a_zero, c_zero) in [(0.0, 0.0), (10.0, -45.0)] {
    let out = engine::run(&mach4_post(), &review_trunnion_with_zeros(a_zero, c_zero), VERTICAL_THEN_TILTED).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<_> = out.program.lines().collect();
    let mut a = 0.0;
    let mut c = 0.0;
    // Carry the first run's final table position into a repeated run.
    for line in &lines {
        if let Some(v) = word_value(line, 'A') { a = v; }
        if let Some(v) = word_value(line, 'C') { c = v; }
    }
    assert!((a - a_zero).abs() > 1.0, "fixture must finish tilted:\n{}", out.program);
    let mut retracted = false;
    let mut indexed = false;
    for line in &lines {
        if line.contains("G28") || line.starts_with("G53 ") || line.contains("G30") { retracted = true; }
        if let Some(v) = word_value(line, 'A') {
            assert!(retracted, "rotary move before retract:\n{}", out.program);
            a = v;
            indexed = true;
        }
        if let Some(v) = word_value(line, 'C') { c = v; }
        if line.starts_with("G01 ") {
            assert!(indexed, "repeat must position rotaries before initial cut:\n{}", out.program);
            assert!((a - a_zero).abs() < 1e-6 && (c - c_zero).abs() < 1e-6, "repeat begins cutting tilted: A{a} C{c}");
            break;
        }
    }
}

}

#[test]
fn mach4_indexed_cut_restores_vertical_before_probe_and_offset_update() {
    for tool_change in [false, true] {
        for guard in [false, true] {
            let cutting = VERTICAL_THEN_TILTED.replace(
                "Feed X 10000 Y 5000 Z 0 F 400000 A -45000",
                "Feed X 10000 Y 5000 Z -10000 F 400000 A -45000",
            );
            let tool = if tool_change { "TOOL 99 \"3D TOUCH PROBE\"\n" } else { "" };
            let approach = if guard { "GUARD 10000 5000 10000 F 1200000\n" } else { "" };
            let stream = format!("{cutting}{tool}\
PARAM ToolKind Touch Probe
PARAM SpindleSpeed 0
PARAM ClearancePlane 25
PARAM RetractPlane 10
SECTION ProbeZ
{approach}TOUCH 10000 5000 -5000 EXPECT 10000 5000 0 F 100000 M 0
RETREAT 10000 5000 5000 EXPECT 10000 5000 5000 F 1200000
FOUND SET 0 TOL 500 AXIS Z 0 0
ENDSECTION
");
            let out = engine::run(&mach4_post(), &review_trunnion(), &stream).expect("posts");
            assert!(out.warnings.is_empty(), "tool_change={tool_change}, guard={guard}: {:?}", out.warnings);
            let mut a: f64 = 0.0;
            let mut c: f64 = 0.0;
            let mut tilted = false;
            let mut restored = false;
            let mut retracted = false;
            let mut probes = 0;
            let mut seated_xy = false;
            let mut seated_z = false;
            for line in out.program.lines() {
                if line.contains("G28") || line.contains("G30") || line.starts_with("G53 ") { retracted = true; }
                if let Some(v) = word_value(line, 'A') {
                    a = v;
                    if a.abs() > 1.0 { tilted = true; retracted = false; }
                    else if tilted {
                        assert!(retracted, "restore must retract first:\n{}", out.program);
                        restored = true;
                    }
                }
                if let Some(v) = word_value(line, 'C') { c = v; }
                if restored && probes == 0 && line.starts_with("G00 ") {
                    if let Some(z) = word_value(line, 'Z') {
                        assert!(z >= 25.0, "unguarded probe descent to Z{z}:\n{}", out.program);
                        seated_z = true;
                    }
                    if word_value(line, 'X').is_some() || word_value(line, 'Y').is_some() {
                        assert!(seated_z, "probe X/Y traverse before known clearance:\n{}", out.program);
                        assert_eq!(word_value(line, 'X'), Some(10.0));
                        assert_eq!(word_value(line, 'Y'), Some(5.0));
                        seated_xy = true;
                    }
                }
                if line.starts_with("G31 ") || line.starts_with("G10 L2 P1") {
                    assert!(restored && a.abs() < 1e-6 && c.abs() < 1e-6,
                        "probe or offset write while table is tilted:\n{}", out.program);
                    if line.starts_with("G31 ") {
                        assert!(seated_xy && seated_z, "probe must start from known clearance seat:\n{}", out.program);
                        probes += 1;
                    }
                }
            }
            assert_eq!(probes, if guard { 2 } else { 1 }, "guard and touch must post:\n{}", out.program);
            assert!(out.program.contains("G10 L2 P1 Z[#5223+#190]"), "offset update must post:\n{}", out.program);
        }
    }
}

#[test]
fn mach4_cli_guard_checks_clearance_descent_when_stream_start_equals_target() {
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!("camcraft-mach4-guard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let input = dir.join("input.cnc");
    let output = dir.join("output.nc");
    let trunnion = dir.join("trunnion.mch");
    let spec = TrunnionSpec {
        unit: "mm", rtcp: Some(false), sign_conv: "table_rh", a_zero: 0.0, c_zero: 0.0,
        c_rollover: false, pivot: [0.0; 3], offset: [0.0; 3],
    };
    std::fs::write(&trunnion, trunnion_mch_json(&spec)).expect("write machine");
    for indexed in [false, true] {
        let cutting = if indexed {
            VERTICAL_THEN_TILTED.replace(
                "Feed X 10000 Y 5000 Z 0 F 400000 A -45000",
                "Feed X 10000 Y 5000 Z 10000 F 400000 A -45000",
            )
        } else {
            "PARAM WorkOffset G54\nPARAM ClearancePlane 25\nTOOL 1 \"6MM EM\"\n\
PARAM SpindleSpeed 8000\nSECTION Vertical\nRapid X 10000 Y 5000 Z 25000\n\
Feed X 10000 Y 5000 Z 10000 F 400000\nENDSECTION\n".to_owned()
        };
        let stream = format!("{cutting}TOOL 99 \"3D TOUCH PROBE\"
PARAM ToolKind Touch Probe
PARAM SpindleSpeed 0
PARAM ClearancePlane 25
PARAM RetractPlane 10
SECTION ProbeZ
GUARD 10000 5000 10000 F 1200000
TOUCH 10000 5000 -5000 EXPECT 10000 5000 0 F 100000 M 0
RETREAT 10000 5000 5000 EXPECT 10000 5000 5000 F 1200000
FOUND SET 0 TOL 500 AXIS Z 0 0
ENDSECTION
");
        std::fs::write(&input, stream).expect("write stream");
        let machine = if indexed { trunnion.clone() } else { repo("assets/machines/3axis-vmc.mch") };
        let result = Command::new(env!("CARGO_BIN_EXE_post"))
            .arg("--machine").arg(machine).arg(repo("assets/posts/mach4.cps"))
            .arg(&input).arg(&output).output().expect("run CLI");
        assert!(result.status.success(), "indexed={indexed}: {}", String::from_utf8_lossy(&result.stderr));
        let nc = std::fs::read_to_string(&output).expect("output");
        let guard = nc.find("G31 X10. Y5. Z10. F1200").expect("guard descent");
        let touch = nc.find("G31 X10. Y5. Z-5. F100").expect("touch");
        assert!(guard < touch, "guard must precede touch:\n{nc}");
        assert!(nc[..guard].ends_with("G00 Z25.\nG00 X10. Y5.\n"), "probe must approach at clearance:\n{nc}");
        assert!(nc[guard..touch].contains("IF[ABS[[#5063-10]]GT0.005]THEN#3000=3(PROBE HIT ON POSITIONING)"),
            "clearance descent must check Z contact before TOUCH, even when stream start equals target (indexed={indexed}):\n{nc}");
        assert!(nc[touch..].contains("G10 L2 P1 Z[#5223+#190]"), "offset update must post:\n{nc}");
    }
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
}

#[test]
fn mach4_cli_validates_declared_options_and_posts_overrides() {
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!("camcraft-mach4-options-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let input = dir.join("input.cnc");
    let mch = dir.join("machine.mch");
    let output = dir.join("output.nc");
    let spec = TrunnionSpec {
        unit: "mm", rtcp: Some(false), sign_conv: "table_rh", a_zero: 0.0, c_zero: 0.0,
        c_rollover: false, pivot: [0.0; 3], offset: [0.0; 3],
    };
    std::fs::write(&input, VERTICAL_THEN_TILTED).expect("write stream");
    std::fs::write(&mch, trunnion_mch_json(&spec)).expect("write machine");
    let run = |options: &[&str]| {
        let _ = std::fs::remove_file(&output);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_post"));
        cmd.arg("--machine").arg(&mch);
        for option in options { cmd.arg("--option").arg(option); }
        cmd.arg(repo("assets/posts/mach4.cps")).arg(&input).arg(&output)
            .output().expect("run CLI")
    };
    let schema = camcraft_post::post::options::declared_options(&mach4_post()).expect("schema");
    let number = schema.iter().find(|o| o.key == "programNumber").expect("programNumber declared");
    assert_eq!(number.default.to_json(), json!(1001));
    assert!(matches!(number.kind, camcraft_post::post::options::OptionKind::Integer { min: Some(1), .. }));
    for name in ["rotaryClamp", "rotaryUnclamp"] {
        let option = schema.iter().find(|o| o.key == name).expect("clamp option");
        assert_eq!(option.default.to_json(), json!(""));
        assert!(matches!(option.kind, camcraft_post::post::options::OptionKind::String));
    }
    let defaults = run(&[]);
    assert!(defaults.status.success(), "{}", String::from_utf8_lossy(&defaults.stderr));
    let nc = std::fs::read_to_string(&output).expect("default output");
    assert!(nc.contains("O0001 ") && nc.contains("G91 G28 Z0."), "{nc}");
    let overrides = run(&[
        "safeRetract=G53", "rolloverMode=unwrapped", "rotaryUnclamp=M11", "rotaryClamp=M10",
        "programNumber=4321",
    ]);
    assert!(overrides.status.success(), "{}", String::from_utf8_lossy(&overrides.stderr));
    let nc = std::fs::read_to_string(&output).expect("override output");
    assert!(nc.contains("O0001 ") && nc.contains("G53 G00 Z0."), "{nc}");
    assert!(nc.contains("M11\nG00 A0. C0.\nM10"), "{nc}");
    let g30 = run(&["safeRetract=G30", "rolloverMode=rollover_360"]);
    assert!(g30.status.success(), "{}", String::from_utf8_lossy(&g30.stderr));
    assert!(std::fs::read_to_string(&output).unwrap().contains("G91 G30 Z0."));
    for bad in ["safeRetract=G99", "rolloverMode=bogus", "programNumber=0", "programNumber=1.5", "programNumber=no"] {
        let result = run(&[bad]);
        let err = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success(), "CLI accepted {bad}");
        assert!(err.contains(bad.split('=').next().unwrap()), "{bad}: {err}");
        assert!(!output.exists(), "invalid {bad} wrote a program");
    }
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
}
