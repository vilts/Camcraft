//! #950: doosan-dnm.cps writes the first A of a program and of each
//! section - the modal A memory is only ever an angle the post wrote,
//! never an assumed A0 - so an A0 operation after an A30 one turns the
//! table back instead of cutting at A30.
//! Provenance: our own tests over our own post; the modal-word reading
//! from the FANUC 0i-F operator's manual (B-64604EN), behaviour only.
//!
//! Gate map (M4.4): covers camcraft_post::post (doosan-dnm.cps rotary words).
//! Lane shape (M4.4): all tests here run in well under a second.

use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn dnm_post() -> String {
    read("assets/posts/doosan-dnm.cps")
}

/// The shipped DNM machine: its A states a direction, so G43.4 (TCP).
fn dnm_machine() -> serde_json::Value {
    machine::mirror(&parse_mch(&read("assets/machines/doosan-dnm.mch")).expect("dnm machine parses"))
}

/// A 4th axis stating no direction: no TCP (`machine.rtcp` false).
fn plain_fourth() -> serde_json::Value {
    serde_json::json!({ "name": "TEST 4TH", "kinematics": { "static": { "rotaries": [
        { "letter": "A", "type": "rotary" } ] } } })
}

/// #950: without TCP the post took the table to stand at A0 at program
/// start and after every tool change, so an A0 section after an A30 one
/// wrote no A word and cut at A30. The first A of a program and of each
/// section is written; the modal memory is only ever an angle written.
/// The TCP machine (G43.4) also unclamps for that turn now.
#[test]
fn doosan_dnm_writes_a0_after_an_a30_tool() {
    let stream = "\
PARAM ClearancePlane 5
TOOL 1 T1
PARAM SpindleSpeed 6000
SECTION
Rapid X 0 Y 0 Z 5000 A 30000
Feed X 0 Y 0 Z 0 F 100000 A 30000
ENDSECTION
PARAM ClearancePlane 5
TOOL 2 T2
PARAM SpindleSpeed 6000
SECTION
Rapid X 0 Y 0 Z 5000 A 0
Feed X 0 Y 0 Z 0 F 100000 A 0
ENDSECTION
";
    let options = serde_json::json!({ "rotaryBrake": true });
    for mch in [plain_fourth(), dnm_machine()] {
        let out = engine::run_with(&dnm_post(), &mch, stream, &options).expect("posts");
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        let lines: Vec<&str> = out.program.lines().collect();
        let t2 = lines.iter().position(|l| l.starts_with("T02")).expect("second tool change");
        let a30 = lines.iter().position(|l| l.contains("G00") && l.contains("A30")).expect("A30 written");
        assert!(a30 < t2, "{}", out.program);
        let a0 = lines[t2..].iter().position(|l| l.contains("G00") && l.split(' ').any(|w| w == "A0."));
        let a0 = t2 + a0.unwrap_or_else(|| panic!("second tool writes no A0:\n{}", out.program));
        assert_eq!(lines[a0 - 1].trim(), "M11", "{}", out.program);
        assert_eq!(lines[a0 + 1].trim(), "M10", "{}", out.program);
        // The feed at the same A holds it modal: no second A word.
        assert!(!lines[a0 + 1..].iter().any(|l| l.contains('A') && l.contains("G01")), "{}", out.program);
    }
}

/// #950: an opening A0 is written too - the post never knows where the
/// table stands before it places it.
#[test]
fn doosan_dnm_writes_the_opening_a0_without_tcp() {
    let stream = "\
PARAM ClearancePlane 5
TOOL 1 T1
PARAM SpindleSpeed 6000
SECTION
Rapid X 0 Y 0 Z 5000 A 0
Feed X 0 Y 0 Z 0 F 100000 A 0
ENDSECTION
";
    let options = serde_json::json!({ "rotaryBrake": true });
    let out = engine::run_with(&dnm_post(), &plain_fourth(), stream, &options).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(!out.program.contains("G43.4"), "{}", out.program);
    assert!(out.program.contains("M11\nG00 X0. Y0. Z5. A0.\nM10"), "{}", out.program);
}
