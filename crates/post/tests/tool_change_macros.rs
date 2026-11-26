//! Provenance: issue #2407 generic ATC macro contract; no controller dialect is inferred.
//! Gate map: covers camcraft_post::post::engine, camcraft_post::post::tool_change.

use camcraft_post::post::{cnc, engine};
use serde_json::{json, Value};

const CPS: &str = r#"
function onOpen() { writeln("BEGIN"); }
function onToolChange(n, name) { writeBlock("T" + n, "M6"); }
function onClose() { writeln("END"); }
"#;

fn stream() -> String {
    cnc::serialize(&[
        cnc::Record::Tool {
            number: 1,
            name: "first".into(),
        },
        cnc::Record::Tool {
            number: 2,
            name: "second".into(),
        },
    ])
}

fn script(config: &Value) -> String {
    format!("var toolChangeMacros = {config};\n{CPS}")
}

#[test]
fn post_macros_surround_every_tool_change_and_map_to_preview() {
    let cps = script(&json!({"before": "M100\r\nM101", "after": "M102"}));
    let out = engine::run(&cps, &Value::Null, &stream()).expect("post macros");
    assert_eq!(
        out.program,
        "BEGIN\nM100\nM101\nT1 M6\nM102\nM100\nM101\nT2 M6\nM102\nEND\n"
    );
    for line in 1..9 {
        let record = if line < 5 { 0 } else { 1 };
        assert!(
            out.line_map
                .iter()
                .any(|m| m.line == line && m.record == Some(record)),
            "line {line} maps to TOOL {record}"
        );
    }
    for (record, first, last) in [(0, 1, 5), (1, 5, 9)] {
        assert!(
            out.blocks
                .iter()
                .any(|b| b.record == Some(record) && b.first == first && b.last == last),
            "preview includes macro lines for TOOL {record}"
        );
    }
}

#[test]
fn empty_and_absent_macros_preserve_output() {
    let baseline = engine::run(CPS, &Value::Null, &stream()).expect("no macros");
    assert_eq!(baseline.program, "BEGIN\nT1 M6\nT2 M6\nEND\n");
    for config in [json!({}), json!({"before": "", "after": ""})] {
        let out =
            engine::run(&script(&config), &Value::Null, &stream()).expect("empty post macros");
        assert_eq!(out.program, baseline.program);
        assert_eq!(out.line_map, baseline.line_map);
    }
}

fn refuses(config: Value, field: &str) {
    let err =
        engine::run(&script(&config), &Value::Null, &stream()).expect_err("bad post macro refuses");
    assert!(
        err.to_string().contains("toolChangeMacros") && err.to_string().contains(field),
        "{err}"
    );
}

#[test]
fn macro_object_type_refuses() {
    for config in [json!(null), json!("M100"), json!([])] {
        refuses(config, "toolChangeMacros");
    }
}

#[test]
fn macro_before_type_refuses() {
    refuses(json!({"before": 100}), "before");
}

#[test]
fn macro_after_type_refuses() {
    refuses(json!({"after": ["M100"]}), "after");
}

#[test]
fn macro_unknown_field_refuses() {
    refuses(json!({"befroe": "M100"}), "befroe");
}

#[test]
fn macro_before_controls_refuse() {
    for bad in ["M100\0M101", "M100\u{1b}M101", "M100\u{7f}M101"] {
        refuses(json!({"before": bad}), "before");
    }
}

#[test]
fn macro_after_controls_refuse() {
    for bad in ["M100\0M101", "M100\u{1b}M101", "M100\u{7f}M101"] {
        refuses(json!({"after": bad}), "after");
    }
}

#[test]
fn macro_lone_carriage_return_refuses() {
    for field in ["before", "after"] {
        refuses(json!({field: "M100\rM101"}), field);
    }
}

#[test]
fn macro_text_is_literal_and_not_javascript() {
    let text = r#"M100 ; quoted \"text\" and backslash \ path"#;
    let out = engine::run(&script(&json!({"before": text})), &Value::Null, &stream())
        .expect("literal text");
    assert_eq!(out.program.matches(&format!("{text}\n")).count(), 2);
}

#[test]
fn repeated_tool_records_preserve_hooks_without_repeating_change_macros() {
    let cps = r#"
var toolChangeMacros = {before: "M100", after: "M101"};
var currentTool = null;
function onToolChange(n) {
    if (currentTool !== n) writeBlock("T" + n, "M6");
    currentTool = n;
    writeBlock("G43", "H" + n);
}
"#;
    let records: Vec<_> = [1, 1, 2, 1]
        .into_iter()
        .map(|number| cnc::Record::Tool {
            number,
            name: "mill".into(),
        })
        .collect();
    let out = engine::run(cps, &Value::Null, &cnc::serialize(&records)).expect("tool transitions");
    assert_eq!(
        out.program,
        "M100\nT1 M6\nG43 H1\nM101\nG43 H1\nM100\nT2 M6\nG43 H2\nM101\nM100\nT1 M6\nG43 H1\nM101\n"
    );
}
