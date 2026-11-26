//! Side Burn posting through the shipped sinker script (#3001).
//! Gate map (M4.4): covers assets/posts/sinker-iso.cps, post::engine.
//! Provenance: recorded CamCraft CL fixtures and our synthetic EDM dialect; no real control codes.

use camcraft_post::post::cnc::{self, Record};
use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::Path;

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path)).expect("fixture")
}

fn dialect() -> Value {
    let mut machine: Value = serde_json::from_str(&read("crates/app/tests/edm/sinker-iso-dialect.mch")).expect("machine fixture");
    // Invented test words. A real machine must state its own lateral servo code.
    machine["edm"]["codes"]["side"] = json!("M102 X{x} Y{y} Z{z} F{f}");
    machine
}

fn section(kind: &str, cl_path: &str, extra: &[(&str, Value)]) -> String {
    let mut records = vec![
        Record::Param { name: "OperationName".into(), value: json!("Burn1") },
        Record::Param { name: "OperationKind".into(), value: json!(kind) },
        Record::Param { name: "ToolKind".into(), value: json!("sinker electrode") },
        Record::Param { name: "SpindleSpeed".into(), value: json!(0.0) },
        Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        Record::Param { name: "SinkerCondition".into(), value: json!("S1") },
        Record::Param { name: "ElectrodePickup".into(), value: json!(15.0) },
    ];
    for (name, value) in extra {
        records.push(Record::Param { name: (*name).into(), value: value.clone() });
    }
    records.push(Record::Tool { number: 1, name: "E-10x6".into() });
    records.push(Record::Section { begin: true });
    records.extend(cnc::parse(&read(cl_path)).expect("CL fixture"));
    records.push(Record::Section { begin: false });
    cnc::serialize(&records)
}

fn side(extra: &[(&str, Value)]) -> String {
    section("Sinker Side Burn", "crates/app/tests/golden/sinker_side_burn.cl", extra)
}

fn post(machine: &Value, stream: &str, units: &str) -> engine::PostOutput {
    engine::run_with(&read("assets/posts/sinker-iso.cps"), machine, stream, &json!({ "Units": units })).expect("post runs")
}

#[test]
fn recorded_side_burn_posts_both_lateral_legs() {
    assert!(camcraft_post::machine::edm_codes::SINKER_CODES.contains(&("side", "{x} {y} {z} {f}")));
    let output = post(&dialect(), &side(&[]), "mm");
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    let burns: Vec<_> = output.program.lines().filter(|line| line.starts_with("M10")).collect();
    assert_eq!(burns, ["M100 H15.", "M101 Z14.05", "M102 X12.5 Y-7.65 Z14.05 F40.", "M102 X12.5 Y-7.5 Z14.05 F40."], "{}", output.program);
}

#[test]
fn lateral_direction_and_amount_follow_each_recorded_axis() {
    for (axis, sign, expected) in [
        ("X", -1.0, "M102 X12.65 Y-7.5 Z14.05 F40."),
        ("X", 1.0, "M102 X12.35 Y-7.5 Z14.05 F40."),
        ("Y", -1.0, "M102 X12.5 Y-7.35 Z14.05 F40."),
        ("Y", 1.0, "M102 X12.5 Y-7.65 Z14.05 F40."),
    ] {
        let mut records = cnc::parse(&side(&[])).expect("recorded stream");
        for record in &mut records {
            if let Record::Move(m) = record {
                let delta = (m.y + 7.5) * sign;
                if axis == "X" {
                    m.x = 12.5 + delta;
                    m.y = -7.5;
                } else {
                    m.y = -7.5 + delta;
                }
            }
        }
        let output = post(&dialect(), &cnc::serialize(&records), "mm");
        assert!(output.warnings.is_empty(), "{axis} {sign}: {:?}", output.warnings);
        assert!(output.program.lines().any(|line| line == expected), "{}", output.program);
        assert_eq!(output.program.lines().filter(|line| line.starts_with("M102")).count(), 2);
    }
}

#[test]
fn lateral_endpoint_feed_and_electrode_reference_use_document_units_and_wcs() {
    let mut machine = dialect();
    machine["edm"]["conditions"][0]["id"] = json!("S2");
    let stream = side(&[
        ("OriginX", json!(10.0)),
        ("OriginY", json!(-5.0)),
        ("OriginZ", json!(10.0)),
        ("SinkerCondition", json!("S2")),
        ("ElectrodePickup", json!(25.4)),
    ]);
    for (units, expected) in [
        ("mm", ["G21", "M100 H25.4", "ES2", "M101 Z4.05", "M102 X2.5 Y-2.65 Z4.05 F40."]),
        ("inch", ["G20", "M100 H1.", "ES2", "M101 Z0.1594", "M102 X0.0984 Y-0.1043 Z0.1594 F1.57"]),
    ] {
        let output = post(&machine, &stream, units);
        assert!(output.warnings.is_empty(), "{units}: {:?}", output.warnings);
        for line in expected {
            assert!(output.program.lines().any(|actual| actual == line), "{units}: {line}\n{}", output.program);
        }
        assert!(output.program.contains("T1 M06"), "{}", output.program);
    }
}

#[test]
fn straight_and_side_burns_post_together_to_recorded_nc() {
    let stream = section("Sinker Burn", "crates/app/tests/golden/sinker_burn_cavity.cl", &[]) + &side(&[]);
    let output = post(&dialect(), &stream, "mm");
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    let path = "crates/post/tests/recorded/golden/sinker-iso/sinker_side_burn.nc";
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path), &output.program).expect("write golden");
    }
    assert_eq!(output.program, read(path));
    assert_eq!(output.program.lines().filter(|line| *line == "M101 Z14.05").count(), 2);
    assert_eq!(output.program.lines().filter(|line| line.starts_with("M102")).count(), 2);
}

#[test]
fn side_burn_requires_explicit_complete_machine_dialect() {
    let mut templates = vec![String::new(), "M102 X{x} Y{y} Z{z} F{f} Q{power}".into()];
    for key in ["x", "y", "z", "f"] {
        templates.push("M102 X{x} Y{y} Z{z} F{f}".replace(&format!("{{{key}}}"), ""));
    }
    for template in templates {
        let mut machine = dialect();
        machine["edm"]["codes"]["side"] = json!(template);
        let output = post(&machine, &side(&[]), "mm");
        assert!(output.warnings.iter().any(|warning| warning.contains("edm.codes.side")), "{template}: {:?}", output.warnings);
        assert!(!output.program.contains("M101") && !output.program.contains("M102"), "{template}: {}", output.program);
    }
}

/// Inject a malformed callback property to cover the script's boundary even
/// when the ordinary CNC parser and driver always supply that property.
fn with_lateral_properties(change: &str, units: &str) -> engine::PostOutput {
    let source = read("assets/posts/sinker-iso.cps")
        + "\nvar originalLinear = onLinear; onLinear = function () { if (properties.Y !== properties.StartY) { "
        + change
        + " } originalLinear(); };\n";
    engine::run_with(&source, &dialect(), &side(&[]), &json!({ "Units": units })).expect("post runs")
}

fn refuses_lateral(change: &str, expected: &str) {
    let output = with_lateral_properties(change, "mm");
    assert!(output.warnings.iter().any(|warning| warning.contains(expected)), "{change}: {:?}", output.warnings);
    assert!(!output.program.contains("M102"), "{change}: {}", output.program);
}

#[test]
fn side_burn_refuses_diagonal_and_combined_vertical_lateral_moves() {
    for change in ["properties.X += 0.1;", "properties.Z -= 0.1;", "properties.Z += 0.1;", "properties.X += 0.00001;"] {
        refuses_lateral(change, "axis-aligned lateral feed at constant Z");
    }
}

#[test]
fn side_burn_refuses_missing_or_non_numeric_start_and_end_coordinates() {
    for key in ["X", "Y", "Z", "StartX", "StartY", "StartZ"] {
        refuses_lateral(&format!("delete properties.{key};"), "complete XYZ start and end coordinates");
        refuses_lateral(&format!("properties.{key} = NaN;"), "complete XYZ start and end coordinates");
    }
}

#[test]
fn side_burn_refuses_missing_zero_or_negative_feed() {
    for change in ["delete properties.FeedRate;", "properties.FeedRate = 0;", "properties.FeedRate = -1;", "properties.FeedRate = NaN;", "properties.FeedRate = 0.00001;"] {
        refuses_lateral(change, "positive feed");
    }
    let output = with_lateral_properties("properties.FeedRate = 0.001;", "inch");
    assert!(output.warnings.iter().any(|warning| warning.contains("positive feed")), "{:?}", output.warnings);
    assert!(!output.program.contains("M102"), "{}", output.program);
}

#[test]
fn side_burn_refuses_motion_below_the_program_resolution() {
    for units in ["mm", "inch"] {
        let output = with_lateral_properties("properties.Y = properties.StartY + 0.00001;", units);
        assert!(output.warnings.iter().any(|warning| warning.contains("below the program resolution")), "{units}: {:?}", output.warnings);
        assert!(!output.program.contains("M102"), "{}", output.program);
    }
}

#[test]
fn side_dialect_does_not_enable_lateral_moves_in_a_straight_burn() {
    let stream = side(&[]).replace("PARAM OperationKind Sinker Side Burn", "PARAM OperationKind Sinker Burn");
    assert!(stream.contains("PARAM OperationKind Sinker Burn"));
    let output = post(&dialect(), &stream, "mm");
    assert!(output.warnings.iter().any(|warning| warning.contains("X/Y travel")), "{:?}", output.warnings);
    assert!(!output.program.contains("M102"), "{}", output.program);
}
