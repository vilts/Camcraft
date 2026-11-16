//! The grblHAL post (#710): the program shape a grbl-family controller
//! reads (no % fence, no O word, no N numbers, no G64, M30), no tool
//! length word at all (the controller's tool-change handling owns the
//! offset), `T<n> M6` alone on its line after a Z lift, dwell in
//! seconds, and the words grblHAL's core does not list - G84 - refused
//! by name. The Z lift never trusts machine Z0 unless the machine file
//! says it is homed (states `safe_retract`): without homing grblHAL's
//! machine zero is the power-on height, so the lift stays in the work
//! offset at the clearance height, refused by name when that height is
//! unknown or not above the stock. The generic machine file mounts and
//! posts clean.
//! Provenance: our own tests over our own post and machine file.
//!
//! Gate map (#710): covers camcraft_post::post (grblhal.cps), camcraft_post::machine
//! (safeRetractZ, units), app::posting (registry, machine library).

use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use camcraft_app::posting::{BUILTINS, MACHINES};
use serde_json::{Value, json};
use std::path::Path;

const STREAM: &str = "PARAM ClearancePlane 15\nPARAM StockZTop 0\nPARAM SpindleSpeed 8000\nTOOL 3 EM6\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 10000 Y 0 Z -1000 F 500000\nENDSECTION\n\
                      PARAM ClearancePlane 15\nPARAM SpindleSpeed 6000\nTOOL 4 DRILL5\nSECTION\nCYCLE DRILL X 20000 Y 5000 Z -3000 R 2000 P 1500 F 100000\nENDSECTION\n";

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn cps() -> String {
    read("assets/posts/grblhal.cps")
}

/// The shipped generic machine, mounted the way the app hands it on.
fn generic() -> Value {
    let text = MACHINES
        .iter()
        .find(|(n, _)| *n == "grblHAL 3-axis")
        .map(|(_, t)| *t)
        .expect("the library ships the grblHAL machine");
    machine::mirror(&parse_mch(text).unwrap_or_else(|e| panic!("parses: {e}")))
}

fn post(m: &Value, stream: &str) -> String {
    let out = engine::run(&cps(), m, stream).unwrap_or_else(|e| panic!("posts: {e}"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out.program
}

/// #902: the program's unit is the document's (`postOptions.Units`),
/// never the machine file's.
fn post_with(m: &Value, stream: &str, options: &Value) -> String {
    let out = engine::run_with(&cps(), m, stream, options).unwrap_or_else(|e| panic!("posts: {e}"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out.program
}

#[test]
fn the_registry_and_the_library_ship_it() {
    assert!(BUILTINS.iter().any(|(n, s)| *n == "grblHAL" && *s == cps()), "the post is built in");
    let m = generic();
    assert!(m.get("safeRetractZ").is_none_or(Value::is_null), "the generic file assumes no homing: {m}");
}

#[test]
fn the_program_has_grbl_shape_and_never_touches_the_length_offset() {
    let nc = post(&generic(), STREAM);
    let lines: Vec<&str> = nc.lines().collect();
    assert_eq!(lines.first(), Some(&"(grblHAL 3-axis)"), "{nc}");
    assert_eq!(lines.get(1), Some(&"G21 G17 G90 G91.1 G94 G40 G80"), "{nc}");
    assert_eq!(lines.last(), Some(&"M30"), "{nc}");
    for l in &lines {
        let first = l.split_whitespace().next().unwrap_or("");
        assert!(!first.starts_with('%') && !first.starts_with('O') && !first.starts_with('N'), "{l:?}");
        for w in l.split_whitespace() {
            assert!(!["G43", "G43.1", "G49", "G64", "G28", "G30", "G84"].contains(&w), "{w} in {l:?}\n{nc}");
            assert!(!w.starts_with('H'), "no length register: {l:?}");
        }
    }
}

#[test]
fn each_tool_change_lifts_z_then_stands_alone() {
    let nc = post(&generic(), STREAM);
    let lines: Vec<&str> = nc.lines().collect();
    for call in ["T3 M6", "T4 M6"] {
        let at = lines.iter().position(|l| *l == call).unwrap_or_else(|| panic!("no {call:?}\n{nc}"));
        assert_eq!(lines[at - 1], "G0 Z15", "Z goes up first, in the work offset\n{nc}");
    }
    // #1616: two changes and the end, plus each section's stated first
    // traverse height.
    assert_eq!(nc.matches("G0 Z15\n").count(), 5, "two changes, two traverses and the end\n{nc}");
    assert!(!nc.contains("G53"), "no machine-coordinate move without homing\n{nc}");
    // X Y move first after a change, at the stated clearance; Z comes down after.
    let t4 = lines.iter().position(|l| *l == "T4 M6").expect("T4");
    let xy = t4 + lines[t4..].iter().position(|l| *l == "G0 X20 Y5").unwrap_or_else(|| panic!("{nc}"));
    assert_eq!(lines[xy - 1], "G0 Z15", "the clearance stated before the traverse\n{nc}");
}

#[test]
fn a_dwell_is_written_in_seconds() {
    let nc = post(&generic(), STREAM);
    assert!(nc.lines().any(|l| l == "G82 X20 Y5 Z-3 R2 P1.5 F100"), "1500 ms is P1.5\n{nc}");
}

#[test]
fn a_tap_is_refused_by_name() {
    let stream = read("crates/app/tests/golden/tap_plate.cl");
    let out = engine::run(&cps(), &generic(), &stream).expect("the script runs");
    let taps: Vec<&String> = out.warnings.iter().filter(|w| w.contains("G84")).collect();
    assert_eq!(taps.len(), 1, "one named refusal per program: {:?}", out.warnings);
}

#[test]
fn a_homed_machine_lifts_in_machine_coordinates() {
    // Stating safe_retract is the machine file saying it homes.
    for z in [0.0, -5.0] {
        let mut m = generic();
        m["safeRetractZ"] = json!(z);
        let nc = post(&m, STREAM);
        let want = format!("G53 G0 Z{z}\nT3 M6\n");
        assert!(nc.contains(&want), "{want:?}\n{nc}");
        assert_eq!(nc.matches("G53 G0 Z").count(), 3, "{nc}");
    }
}

#[test]
fn without_homing_the_lift_stays_in_the_work_offset() {
    // No machine at all is grbl's default too: no homing assumed.
    let nc = post(&Value::Null, STREAM);
    assert!(nc.contains("G0 Z15\nT3 M6\n") && !nc.contains("G53"), "{nc}");
    // A higher Z the program already commanded is clear as well.
    let high = STREAM.replacen("Rapid X 0 Y 0 Z 5000", "Rapid X 0 Y 0 Z 40000", 1);
    let nc = post(&generic(), &high);
    assert!(nc.contains("G0 Z40\nT4 M6\n") && nc.ends_with("G0 Z40\nM30\n"), "{nc}");
    // The lift is written from the work offset, like every Z word.
    let shifted = format!("PARAM OriginZ 10\n{STREAM}");
    assert!(post(&generic(), &shifted).contains("G0 Z5\nT3 M6\n"));
}

#[test]
fn an_unknown_or_buried_lift_height_is_refused_by_name() {
    let refused = |stream: &str| {
        let out = engine::run(&cps(), &generic(), stream).expect("the script runs");
        assert!(!out.program.contains("G53"), "nothing guessed:\n{}", out.program);
        out.warnings
    };
    // No clearance stated and nothing commanded yet at the first change.
    let w = refused(&STREAM.replacen("PARAM ClearancePlane 15\n", "", 1));
    assert!(w.iter().any(|w| w.contains("no clearance height is known")), "{w:?}");
    // A clearance at or below the stock top is no clearance.
    let w = refused(&STREAM.replacen("PARAM StockZTop 0", "PARAM StockZTop 20", 1));
    assert!(w.iter().any(|w| w.contains("is not above the stock top")), "{w:?}");
    assert_eq!(w.iter().filter(|w| w.contains("Z lift") || w.contains("stock top")).count(), 1, "once: {w:?}");
}
#[test]
fn an_inch_document_posts_g20_and_inches() {
    let nc = post_with(&generic(), STREAM, &json!({"Units": "inch"}));
    assert!(nc.lines().nth(1).is_some_and(|l| l.starts_with("G20 ")), "{nc}");
    assert!(nc.contains("G1 X0.3937 Y0 Z-0.0394 F19.69"), "{nc}");
    // An inch machine file alone no longer makes an inch program.
    let mut m = generic();
    m["units"] = json!("inch");
    assert_eq!(post(&m, STREAM), post(&generic(), STREAM), "the machine file's unit decides nothing");
}
