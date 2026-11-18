//! #2189: end-unit post option (`postOptions.EndUnits`) across every shipped
//! mill dialect and undeclared scripts.
//!
//! Provenance: our own streams and fixtures over RS-274 / ISO 6983 (`G20` / `G21`)
//! and SINUMERIK 840D (`G700` / `G710`); Haas Setting 9, Carvera mm-only firmware,
//! and Heidenhain mm-only post behavior as documented in each post header.
//!
//! Gate map (M4.4): covers camcraft_post::post::end_units,
//! camcraft_post::post::engine, assets/posts/*.cps.

use camcraft_post::post::{engine, PostError};
use serde_json::{json, Value};
use std::path::Path;

fn script(stem: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../assets/posts/{stem}.cps")),
    )
    .unwrap_or_else(|e| panic!("missing {stem}.cps: {e}"))
}

fn stream() -> &'static str {
    "PARAM SpindleSpeed 6000\nPARAM WorkOffset 1\nPARAM ClearancePlane 10\nPARAM RetractPlane 5\nTOOL 3 END MILL\nSECTION\nRapid 10000 20000 5000\nFeed 10000 20000 -1000 F300000\nFeed 20000 20000 -1000 F300000\nRapid 20000 20000 10000\nENDSECTION\n"
}

fn run(stem: &str, opts: Value) -> Result<String, PostError> {
    let out = engine::run_with(&script(stem), &Value::Null, stream(), &opts)?;
    assert!(out.warnings.is_empty(), "{stem}: unexpected warnings: {:?}", out.warnings);
    Ok(out.program)
}

fn non_empty_lines(prog: &str) -> Vec<&str> {
    prog.lines().filter(|l| !l.is_empty()).collect()
}

fn end_index(lines: &[&str], end_word: &str) -> usize {
    lines
        .iter()
        .rposition(|l| {
            l.split_whitespace()
                .any(|w| w.trim_end_matches('*') == end_word)
        })
        .unwrap_or_else(|| panic!("no {end_word} in {lines:?}"))
}

#[test]
fn writing_mill_posts_emit_end_unit_word_before_program_end() {
    let cases = [
        ("tormach", "G20", "G21", "M30"),
        ("grblhal", "G20", "G21", "M30"),
        ("tinyg", "G20", "G21", "M30"),
        ("fanuc", "G20", "G21", "M30"),
        ("mazak", "G20", "G21", "M30"),
        ("doosan-dnm", "G20", "G21", "M30"),
        ("sinumerik", "G700", "G710", "M2"),
    ];
    for (stem, inch_word, mm_word, end_word) in cases {
        let mm_plain = run(stem, json!({ "Units": "mm" })).expect("mm plain");
        let mm_same = run(stem, json!({ "Units": "mm", "EndUnits": "mm" })).expect("mm same");
        assert_eq!(mm_same, mm_plain, "{stem}: matching mm adds nothing");

        let mm_to_inch =
            run(stem, json!({ "Units": "mm", "EndUnits": "inch" })).expect("mm -> inch");
        let lines = non_empty_lines(&mm_to_inch);
        let ei = end_index(&lines, end_word);
        assert!(ei > 0, "{stem}: missing line before {end_word}");
        assert!(
            lines[ei - 1].split_whitespace().any(|w| w == inch_word),
            "{stem}: expected {inch_word} right before {end_word}, got {:?}\n{mm_to_inch}",
            &lines[ei.saturating_sub(2)..=ei]
        );
        let stripped: Vec<&str> = lines
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != ei - 1)
            .map(|(_, l)| *l)
            .collect();
        if stem != "sinumerik" {
            assert_eq!(stripped, non_empty_lines(&mm_plain), "{stem}: only the end-unit line differs");
        } else {
            assert_eq!(stripped.len(), non_empty_lines(&mm_plain).len(), "{stem}: line count");
        }

        let inch_plain = run(stem, json!({ "Units": "inch" })).expect("inch plain");
        let inch_same =
            run(stem, json!({ "Units": "inch", "EndUnits": "inch" })).expect("inch same");
        assert_eq!(inch_same, inch_plain, "{stem}: matching inch adds nothing");

        let inch_to_mm =
            run(stem, json!({ "Units": "inch", "EndUnits": "mm" })).expect("inch -> mm");
        let lines = non_empty_lines(&inch_to_mm);
        let ei = end_index(&lines, end_word);
        assert!(ei > 0, "{stem}: missing line before {end_word}");
        assert!(
            lines[ei - 1].split_whitespace().any(|w| w == mm_word),
            "{stem}: expected {mm_word} right before {end_word}, got {:?}\n{inch_to_mm}",
            &lines[ei.saturating_sub(2)..=ei]
        );
    }
}

#[test]
fn non_switching_mill_posts_note_end_units_once() {
    for (stem, supports_inch) in [("haas", true), ("carvera", false), ("heidenhain", false)] {
        let mm_plain = run(stem, json!({ "Units": "mm" })).expect("mm plain");
        let mm_same = run(stem, json!({ "Units": "mm", "EndUnits": "mm" })).expect("mm same");
        assert_eq!(mm_same, mm_plain, "{stem}: matching mm adds nothing");

        let mm_to_inch =
            run(stem, json!({ "Units": "mm", "EndUnits": "inch" })).expect("mm -> inch notes");
        let noted: Vec<&str> = mm_to_inch
            .lines()
            .filter(|l| l.contains("END UNITS NOT WRITTEN"))
            .collect();
        assert_eq!(
            noted.len(),
            1,
            "{stem}: expected one END UNITS NOT WRITTEN note:\n{mm_to_inch}"
        );
        assert!(
            !mm_to_inch.lines().any(|l| l == "G20" || l.contains("G70")),
            "{stem}: must not write G20/G70:\n{mm_to_inch}"
        );
        let stripped: Vec<&str> = mm_to_inch
            .lines()
            .filter(|l| !l.contains("END UNITS NOT WRITTEN"))
            .collect();
        if stem != "heidenhain" {
            let plain_lines: Vec<&str> = mm_plain.lines().collect();
            assert_eq!(stripped, plain_lines, "{stem}: only the note differs");
        } else {
            assert_eq!(stripped.len(), mm_plain.lines().count(), "{stem}: line count");
        }

        if supports_inch {
            let inch_plain = run(stem, json!({ "Units": "inch" })).expect("inch plain");
            let inch_same =
                run(stem, json!({ "Units": "inch", "EndUnits": "inch" })).expect("inch same");
            assert_eq!(inch_same, inch_plain, "{stem}: matching inch adds nothing");

            let inch_to_mm =
                run(stem, json!({ "Units": "inch", "EndUnits": "mm" })).expect("inch -> mm notes");
            let noted: Vec<&str> = inch_to_mm
                .lines()
                .filter(|l| l.contains("END UNITS NOT WRITTEN"))
                .collect();
            assert_eq!(noted.len(), 1, "{stem}: inch -> mm note:\n{inch_to_mm}");
        }
    }
}

#[test]
fn undeclared_post_refuses_end_units_by_name() {
    let custom = r#"
description = "Custom Mill (3-axis)";
programUnits = "mm inch";
function onOpen() { writeln("G21"); }
function onClose() { writeln("M30"); }
"#;
    let err = engine::run_with(custom, &Value::Null, "", &json!({ "EndUnits": "inch" }))
        .expect_err("undeclared endUnits must refuse");
    match err {
        PostError::Refused(why) => {
            assert!(why.contains("endUnits"), "{why}");
            assert!(why.contains("Custom Mill"), "{why}");
        }
        other => panic!("expected Refused, got {other:?}"),
    }
    let lathe_err =
        engine::run_with(&script("fanuc-lathe"), &Value::Null, "", &json!({ "EndUnits": "inch" }))
            .expect_err("lathe without endUnits must refuse");
    assert!(matches!(lathe_err, PostError::Refused(_)), "{lathe_err:?}");
}
