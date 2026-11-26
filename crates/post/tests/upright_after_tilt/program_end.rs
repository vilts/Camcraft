//! #3540: a program whose last section is tilted ends upright. The
//! rotary words are modal, so without a return the table or the head
//! stays at the last tilt after M30. The program end writes the same
//! blocks as the return of #3466: G49, the Z retract, then the zero angle
//! words with no X/Y word, then the X/Y home and M30. Doosan unclamps and
//! clamps a turning A around the return.
//!
//! Provenance: RS-274 modal rotary words and G28 / G53 reference returns;
//! FANUC G49 cancel of tool length and tool centre point control; our own
//! design notes only.

use super::{dump_of, tilted_path, upright_path, word, Toolpath};
use camcraft_post::post::{cnc, engine};
use serde_json::json;

/// The lines from the last line that states `letter` off zero to the end.
fn program_end<'a>(stem: &str, prog: &'a str, letter: char) -> Vec<&'a str> {
    let lines: Vec<&str> = prog.lines().collect();
    let last = lines
        .iter()
        .rposition(|l| word(l, letter).is_some_and(|v| v.abs() > 1.0))
        .unwrap_or_else(|| panic!("{stem}: the lean posts:\n{prog}"));
    lines[last + 1..].to_vec()
}

fn post(stem: &str, r: &[cnc::Record], opts: serde_json::Value) -> engine::PostOutput {
    let out = engine::run_with(&super::read(&format!("assets/posts/{stem}.cps")), &super::tilthead(), &cnc::serialize(r), &opts)
        .unwrap_or_else(|e| panic!("{stem}: the stream posts: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: no refusal: {:?}\n{}", out.warnings, out.program);
    out
}

/// After the last tilted block: G49, a retract, the zero word with no
/// X/Y word, then (G28 mode) the X/Y home, then M30.
fn assert_ends_upright(stem: &str, out: &engine::PostOutput, letter: char, retract: &str) {
    let prog = &out.program;
    let end = program_end(stem, prog, letter);
    let m30 = end.iter().position(|l| l.trim() == "M30").unwrap_or_else(|| panic!("{stem}: M30 ends the program:\n{prog}"));
    let zero = end[..m30]
        .iter()
        .position(|l| word(l, letter).is_some_and(|v| v.abs() < 1e-9))
        .unwrap_or_else(|| panic!("{stem}: the program commands {letter}0 before M30:\n{prog}"));
    let zl = end[zero];
    assert!(word(zl, 'X').is_none() && word(zl, 'Y').is_none(), "{stem}: the return moves no X/Y: {zl}\n{prog}");
    let up = end[..zero]
        .iter()
        .position(|l| l.contains(retract))
        .unwrap_or_else(|| panic!("{stem}: the tool retracts ({retract}) before the rotaries turn:\n{prog}"));
    assert!(end[..up].iter().any(|l| l.trim() == "G49"), "{stem}: G49 cancels TCPM before the retract:\n{prog}");
    if retract == "G28" {
        let xy = end[..m30]
            .iter()
            .position(|l| l.contains("G28") && word(l, 'X').is_some() && word(l, 'Y').is_some())
            .unwrap_or_else(|| panic!("{stem}: the program homes X/Y:\n{prog}"));
        assert!(zero < xy, "{stem}: X/Y home after the rotary return:\n{prog}");
        assert!(end[xy].contains("G91"), "{stem}: the X/Y home is incremental (no via point at work zero): {}\n{prog}", end[xy]);
    }
}

#[test]
fn fanuc_returns_rotaries_before_program_end() {
    let r = dump_of(&tilted_path(), "Lean1", 1);
    assert_ends_upright("fanuc", &post("fanuc", &r, json!({})), 'B', "G28");
    assert_ends_upright("fanuc", &post("fanuc", &r, json!({ "safeRetract": "G53" })), 'B', "G53");
}

#[test]
fn doosan_returns_rotaries_before_program_end() {
    let r = dump_of(&tilted_path(), "Lean1", 1);
    let brake = json!({ "rotaryBrake": true });
    assert_ends_upright("doosan-dnm", &post("doosan-dnm", &r, brake), 'B', "G28");
    let g53 = json!({ "rotaryBrake": true, "safeRetract": "G53" });
    assert_ends_upright("doosan-dnm", &post("doosan-dnm", &r, g53), 'B', "G53");
}

/// A turning A on the Doosan post is unclamped (M11) before the return
/// and clamped (M10) after it, as at the return of #3466.
#[test]
fn doosan_unclamps_and_clamps_a_at_program_end() {
    let (s, c) = (30f64.to_radians().sin(), 30f64.to_radians().cos());
    let lean = [0.0, -s, c];
    let tilted = Toolpath::new(upright_path().moves.iter().map(|m| m.with_axis(lean)).collect());
    let out = post("doosan-dnm", &dump_of(&tilted, "LeanA", 1), json!({ "rotaryBrake": true }));
    assert_ends_upright("doosan-dnm", &out, 'A', "G28");
    let prog = &out.program;
    let end = program_end("doosan-dnm", prog, 'A');
    let zero = end.iter().position(|l| word(l, 'A').is_some_and(|v| v.abs() < 1e-9)).unwrap_or_else(|| panic!("A0:\n{prog}"));
    assert!(end[..zero].iter().any(|l| l.trim() == "M11"), "M11 before the A return:\n{prog}");
    assert!(end[zero + 1..].iter().any(|l| l.trim() == "M10"), "M10 after the A return:\n{prog}");
}

/// An upright program ends byte for byte as before: no G49, no angle.
#[test]
fn upright_program_end_adds_no_rotary_return() {
    for stem in ["fanuc", "doosan-dnm"] {
        let out = post(stem, &dump_of(&upright_path(), "Flat1", 1), json!({ "rotaryBrake": true }));
        let tail: Vec<&str> = out.program.lines().rev().take(5).collect();
        assert_eq!(tail, ["%", "M30", "G28 X0 Y0", "G91 G28 Z0", "M5"], "{stem}:\n{}", out.program);
        assert!(!out.program.lines().any(|l| l.trim() == "G49"), "{stem}: no G49 line:\n{}", out.program);
    }
}
