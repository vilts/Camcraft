//! Provenance: synthetic CNC intermediate streams for the #1498 offset decision.
//! Gate map: covers assets/posts/sodick-wire.cps, camcraft_post::post::engine,
//! camcraft_post::post::cnc.

use camcraft_post::cl::CompSide;
use camcraft_post::post::{cnc, engine};
use serde_json::json;

const SCRIPT: &str = include_str!("../../../assets/posts/sodick-wire.cps");
const STREAM: &str = "; HSMW-CNC v1
PARAM OperationName Punch1
PARAM WorkOffset G54
PARAM EdmCondition C110
SECTION
Rapid X -30000 Y 0 Z 0
Feed X -20000 Y 0 Z 0 F 2000
Feed X -10000 Y 0 Z 0 F 2000
ENDSECTION
";

fn run(stream: &str) -> engine::PostOutput {
    let machine = json!({"kind": "wire_edm", "name": "Synthetic wire", "edm": {"awt": true}});
    engine::run(SCRIPT, &machine, stream).expect("synthetic stream runs")
}

fn comp(side: Option<CompSide>, wear: bool) -> String {
    cnc::serialize(&[cnc::Record::Comp {
        side,
        radius_mm: if side.is_some() { 0.165 } else { 0.0 },
        wear,
    }])
    .lines()
    .filter(|line| line.starts_with("COMP "))
    .map(|line| format!("{line}\n"))
    .collect()
}

fn assert_refused(out: &engine::PostOutput) {
    assert!(
        out.warnings.iter().any(|w| {
            w.contains("Punch1") && w.contains("Sodick") && w.contains("control-side wire offset")
        }),
        "{:?}\n{}",
        out.warnings,
        out.program
    );
}

fn has_word(out: &engine::PostOutput, word: &str) -> bool {
    out.program
        .lines()
        .filter(|line| !line.starts_with('('))
        .flat_map(str::split_whitespace)
        .any(|actual| actual == word)
}

#[test]
fn sodick_control_and_wear_offsets_before_section_refuse_without_cutting() {
    for side in [CompSide::Left, CompSide::Right] {
        for wear in [false, true] {
            let stream = STREAM.replacen(
                "SECTION\n",
                &format!("{}SECTION\n", comp(Some(side), wear)),
                1,
            );
            let out = run(&stream);
            assert!(
                !has_word(&out, "G01") && !has_word(&out, "T91"),
                "{}",
                out.program
            );
            assert_refused(&out);
        }
    }
}

#[test]
fn sodick_control_and_wear_offsets_inside_section_stop_subsequent_cuts() {
    for side in [CompSide::Left, CompSide::Right] {
        for wear in [false, true] {
            let stream = STREAM.replace(
                "Feed X -10000",
                &format!("{}Feed X -10000", comp(Some(side), wear)),
            );
            let out = run(&stream);
            assert!(
                out.program.lines().any(|line| line == "G01 X-20. Y0."),
                "{}",
                out.program
            );
            assert!(
                !out.program.lines().any(|line| line == "G01 X-10. Y0."),
                "{}",
                out.program
            );
            assert_refused(&out);
            // A wire already threaded must still be cut at the section's end.
            assert!(has_word(&out, "T90"), "{}", out.program);
        }
    }
}

#[test]
fn sodick_comp_cancel_does_not_reenable_a_refused_section() {
    let request = format!(
        "{}{}",
        comp(Some(CompSide::Right), false),
        comp(None, false)
    );
    let stream = STREAM.replace("Feed X -20000", &format!("{request}Feed X -20000"));
    let out = run(&stream);
    assert!(
        !has_word(&out, "G01") && !has_word(&out, "T91"),
        "{}",
        out.program
    );
    assert_refused(&out);
}

#[test]
fn sodick_comp_request_stops_a_following_arc() {
    let stream = STREAM.replace(
        "Feed X -10000 Y 0 Z 0 F 2000",
        &format!(
            "{}Arc CCW X -10000 Y 0 Z 0 F 2000 I -15000 J 0",
            comp(Some(CompSide::Left), false)
        ),
    );
    let out = run(&stream);
    assert!(
        !has_word(&out, "G02") && !has_word(&out, "G03"),
        "{}",
        out.program
    );
    assert_refused(&out);
}

#[test]
fn sodick_standalone_comp_off_is_refused_as_an_unsupported_record() {
    let stream = STREAM.replace(
        "Feed X -20000",
        &format!("{}Feed X -20000", comp(None, false)),
    );
    let out = run(&stream);
    assert!(
        !has_word(&out, "G01") && !has_word(&out, "T91"),
        "{}",
        out.program
    );
    assert_refused(&out);
}

#[test]
fn sodick_comp_refusal_resets_for_the_next_plain_section() {
    let refused = STREAM.replacen(
        "SECTION\n",
        &format!("{}SECTION\n", comp(Some(CompSide::Left), false)),
        1,
    );
    let plain = STREAM.replace("Punch1", "Plain2");
    let out = run(&format!("{refused}{plain}"));
    assert_refused(&out);
    for line in ["G01 X-20. Y0.", "G01 X-10. Y0.", "T91", "T90"] {
        assert_eq!(
            out.program.lines().filter(|actual| *actual == line).count(),
            1,
            "{}",
            out.program
        );
    }
    assert!(
        !out.warnings.iter().any(|w| w.contains("Plain2")),
        "{:?}",
        out.warnings
    );
}

#[test]
fn sodick_plain_wire_centre_stream_still_posts() {
    let out = run(STREAM);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(
        has_word(&out, "G01") && has_word(&out, "T91") && has_word(&out, "T90"),
        "{}",
        out.program
    );
    assert!(
        !has_word(&out, "G41") && !has_word(&out, "G42"),
        "{}",
        out.program
    );
}
