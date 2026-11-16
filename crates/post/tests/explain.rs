//! Explain G-code (#1386): the PRELUDE's `explain()` helper, the
//! manifest's `explains` / `commentMax`, and the refusal of a post that
//! states no comment form. The comments are inert: stripped of them, an
//! Explain-on program is byte-identical to the Explain-off one.
//!
//! Gate map (M4.4): covers camcraft_post::post::explain, ::post::engine,
//! ::post::js.

use camcraft_post::cl::MoveKind;
use camcraft_post::post::cnc::{self, MoveRec, Record};
use camcraft_post::post::{engine, PostError};
use serde_json::{json, Value};

/// A test post in the given comment form: every block's words, then
/// `explain()` of them after the words - the way a dialect's `block()`
/// is meant to call it.
fn script(manifest: &str) -> String {
    format!(
        r#"{manifest}
description = "Explain Test (3-axis mill)";
function block(words) {{
  var c = explain(words);
  writeBlock.apply(null, c === "" ? words : words.concat([c]));
}}
function onOpen() {{ block(["G21", "G17", "G90", "G94", "G40", "G49", "G80"]); }}
function onToolChange(n, name) {{ block(["T" + n, "M6"]); block(["S8000", "M3"]); block(["M8"]); }}
function onRapid() {{ block(["G0", "X" + formatUnit(properties.X), "Y" + formatUnit(properties.Y), "Z" + formatUnit(properties.Z)]); }}
function onLinear() {{
  var w = ["X" + formatUnit(properties.X), "Y" + formatUnit(properties.Y), "Z" + formatUnit(properties.Z)];
  if (properties.FeedRate) w = ["G1"].concat(w).concat(["F" + formatFeed(properties.FeedRate)]);
  block(w);
}}
function onClose() {{ block(["M9"]); block(["M5"]); block(["G53", "G0", "Z0"]); block(["M30"]); }}
"#
    )
}

fn mv(kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>) -> Record {
    Record::Move(MoveRec { kind, x, y, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

fn stream() -> String {
    cnc::serialize(&[
        Record::Tool { number: 3, name: "6MM FLAT".into() },
        Record::Section { begin: true },
        mv(MoveKind::Rapid, 1.5, -2.0, 5.0, None),
        mv(MoveKind::Feed, 1.5, -2.0, -1.0, Some(300.0)),
        mv(MoveKind::Feed, 8.0, -2.0, -1.0, None),
        Record::Section { begin: false },
    ])
}

fn post(cps: &str, options: &Value) -> Result<String, PostError> {
    engine::run_with(cps, &Value::Null, &stream(), options).map(|o| o.program)
}

/// The program with every trailing explanation taken off its block.
fn strip(program: &str, open: &str) -> String {
    program
        .lines()
        .map(|l| match l.find(&format!(" {open}")) {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn explain_on_comments_blocks_in_both_forms_and_strips_back_to_the_plain_program() {
    let on = json!({ "Explain": true });
    for (manifest, open) in [("explains = \"paren\";", "("), ("explains = \"semi\";", ";")] {
        let cps = script(manifest);
        let plain = post(&cps, &json!({})).expect("explain off posts");
        let explained = post(&cps, &on).expect("explain on posts");
        assert!(!plain.contains(open), "off writes no comment:\n{plain}");
        assert_ne!(plain, explained, "on adds comments");
        assert_eq!(strip(&explained, open), plain, "comments only, never a changed word:\n{explained}");
        let say = |line: &str| explained.lines().find(|l| l.starts_with(line)).map(str::to_owned).unwrap_or_default();
        if open == "(" {
            assert!(say("G21 G17").ends_with(" (MM UNITS, XY PLANE, ABSOLUTE POSITIONS, FEED PER MINUTE)"), "{explained}");
            assert_eq!(say("T3 M6"), "T3 M6 (TOOL 3, TOOL CHANGE)");
            assert_eq!(say("G0 X1.5"), "G0 X1.5 Y-2 Z5 (RAPID MOVE)");
            assert_eq!(say("G1 X1.5"), "G1 X1.5 Y-2 Z-1 F300 (FEED MOVE, FEED RATE 300)");
            assert_eq!(say("M30"), "M30 (PROGRAM END, BACK TO START)");
        } else {
            assert_eq!(say("T3 M6"), "T3 M6 ; TOOL 3, TOOL CHANGE");
            assert_eq!(say("M9"), "M9 ; COOLANT OFF");
        }
        // A bare continuation block carries no G / M / T / S word.
        assert!(explained.lines().any(|l| l == "X8 Y-2 Z-1"), "{explained}");
        // Every comment fits the default cap of 60, delimiters included.
        for l in explained.lines() {
            if let Some(i) = l.find(&format!(" {open}")) {
                assert!(l[i + 1..].len() <= 60, "too long: {l}");
            }
        }
    }
}

#[test]
fn a_post_without_explains_is_refused_by_name_before_any_hook() {
    let cps = "description = \"Quiet Mill (3-axis)\";\nfunction onOpen() { writeln(\"%\"); }\n";
    match post(cps, &json!({ "Explain": true })) {
        Err(PostError::Refused(why)) => {
            assert!(why.contains("the Quiet Mill post does not declare explains"), "{why}");
        }
        other => panic!("expected a refusal by name, got {other:?}"),
    }
    // Off, the same post posts as it always did.
    assert_eq!(post(cps, &json!({})).expect("posts"), "%\n");
    assert_eq!(post(cps, &json!({ "Explain": false })).expect("posts"), "%\n");
}

#[test]
fn explain_guards_the_comment_text_and_its_length() {
    // A script that probes the guards directly through the helper.
    let probe = |manifest: &str, text: &str| -> String {
        let cps = format!(
            "{manifest}\nfunction onOpen() {{ writeln(\"[\" + _explainComment({}) + \"]\"); }}\n",
            json!(text)
        );
        post(&cps, &json!({ "Explain": true })).expect("posts")
    };
    // Delimiters and control characters never reach the comment.
    assert_eq!(probe("explains = \"paren\";", "A (B) ; C % D\u{7}E"), "[(A B C D E)]\n");
    // A keyword a control would execute is led by "- " in every form.
    assert_eq!(probe("explains = \"paren\";", "msg, hello"), "[(- msg, hello)]\n");
    assert_eq!(probe("explains = \"semi\";", "PRINT 1"), "[; - PRINT 1]\n");
    // The cap cuts the comment, delimiters included, never past it -
    // back to a whole meaning where one fits.
    assert_eq!(probe("explains = \"paren\"; commentMax = 10;", "ABCDEFGHIJKLMNOP"), "[(ABCDEFGH)]\n");
    assert_eq!(probe("explains = \"semi\"; commentMax = 10;", "ABC, DEFGHIJ"), "[; ABC]\n");
}

#[test]
fn explain_says_nothing_until_the_engine_arms_it() {
    // A post that declares a form but runs with the option off: no
    // comment at all, so every shipped post that calls explain() posts
    // byte-identical programs while the option is off.
    let cps = "explains = \"paren\";\nfunction onOpen() { writeln(\"[\" + explain([\"G0\", \"M3\"]) + \"]\"); }\n";
    assert_eq!(post(cps, &json!({})).expect("posts"), "[]\n");
    assert_eq!(post(cps, &json!({ "Explain": true })).expect("posts"), "[(RAPID MOVE, SPINDLE ON CLOCKWISE)]\n");
    // An unknown word is skipped, never guessed; a comment ends the read.
    let cps = "explains = \"paren\";\nfunction onOpen() { writeln(\"[\" + explain(\"G987 M3 (G0)\") + \"]\"); }\n";
    assert_eq!(post(cps, &json!({ "Explain": true })).expect("posts"), "[(SPINDLE ON CLOCKWISE)]\n");
}
