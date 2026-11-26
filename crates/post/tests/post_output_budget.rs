//! Provenance: issue #3002; original large-program and runaway-script fixtures.
//! Gate map: covers camcraft_post::post::engine, camcraft_post::post::js, camcraft_post::post::js_host.

use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine};
use serde_json::Value;
use std::path::Path;

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path))
        .expect("fixture")
}

fn operation(name: &str, moves: usize) -> String {
    let mut stream = format!(
        "PARAM OperationName {name}\nPARAM SpindleSpeed 1500\nTOOL 4 FLAT\n\
         PARAM ClearancePlane 25.4\nPARAM RetractPlane 12.7\nSECTION\nRapid 20000 10000 2000\n"
    );
    for i in 0..moves {
        stream.push_str(if i % 2 == 0 {
            "Feed 21000 10000 1000 F120000\n"
        } else {
            "Feed 20000 10000 1000 F120000\n"
        });
    }
    stream.push_str("Rapid 20000 10000 2000\nENDSECTION\n");
    stream
}

fn check_program(out: engine::PostOutput, moves: usize) {
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(out.swallowed, 0);
    assert_eq!(out.program.lines().filter(|line| line.starts_with("G1 ")).count(), moves);
    assert!(out.program.ends_with("M30\n"), "the complete program must end with M30");
    if moves > 100_000 {
        assert!(out.program.lines().count() > 100_000);
    }
}

#[test]
fn tormach_posts_over_100k_moves_in_a_whole_program() {
    let cps = read("assets/posts/tormach.cps");
    let mch = machine::mirror(&parse_mch(&read("assets/machines/tormach-770mx.mch")).unwrap());
    let large = operation("Large pocket", 100_001);
    let small = operation("Finish pocket", 3);
    check_program(engine::run(&cps, &mch, &format!("{large}{small}")).expect("whole program"), 100_004);
}

#[test]
fn tormach_posts_over_100k_moves_per_operation() {
    let cps = read("assets/posts/tormach.cps");
    let mch = machine::mirror(&parse_mch(&read("assets/machines/tormach-770mx.mch")).unwrap());
    let large = operation("Large pocket", 100_001);
    let small = operation("Finish pocket", 3);
    check_program(engine::run_numbered(&cps, &mch, &large, 1).expect("large operation"), 100_001);
    check_program(engine::run_numbered(&cps, &mch, &small, 2).expect("small operation"), 3);
}

fn assert_named_limit(error: &str) {
    assert!(error.contains("output") && error.contains("limit"), "{error}");
    assert!(error.contains("100000"), "the refusal must state the limit: {error}");
    assert!(error.contains("100001"), "the refusal must state the attempted count: {error}");
    assert!(error.contains("Runaway pocket"), "the refusal must name the operation: {error}");
}

#[test]
fn runaway_writers_report_limit_count_and_operation() {
    let stream = "PARAM OperationName Runaway pocket\nSECTION\nENDSECTION\n";
    for write in ["writeln('x');", "OUT.push('x');"] {
        let cps = format!("function onSection() {{ for (var i = 0; i < 100001; i++) {{ {write} }} }}");
        let error = engine::run(&cps, &Value::Null, stream).unwrap_err().to_string();
        assert_named_limit(&error);
    }
}

#[test]
fn caught_runaway_refusal_keeps_limit_count_and_operation_after_reset() {
    let cps = "function onSection() {\n\
        try { for (var i = 0; i < 100001; i++) writeln('x'); }\n\
        catch (e) { _outputLimitExceeded = false; _outputLines = 0; OUT.length = 0; }\n\
    }";
    let stream = "PARAM OperationName Runaway pocket\nSECTION\nENDSECTION\n";
    let error = engine::run(cps, &Value::Null, stream).unwrap_err().to_string();
    assert_named_limit(&error);
}

#[test]
fn large_input_still_refuses_a_runaway_script_at_its_scaled_limit() {
    // Three structural records and 7,000 moves permit 113,072 output lines.
    let mut stream = String::from("PARAM OperationName Large runaway pocket\nSECTION\n");
    stream.push_str(&"Feed 1000 0 0 F120000\n".repeat(7_000));
    stream.push_str("ENDSECTION\n");
    let cps = "function onSection() { for (var i = 0; i < 200000; i++) writeln('x'); }";
    let error = engine::run(cps, &Value::Null, &stream).unwrap_err().to_string();
    assert!(error.contains("output") && error.contains("limit"), "{error}");
    assert!(error.contains("113072"), "the refusal must state the scaled limit: {error}");
    assert!(error.contains("113073"), "the refusal must state the attempted count: {error}");
    assert!(error.contains("Large runaway pocket"), "{error}");
}

#[test]
fn top_level_runaway_names_the_first_operation() {
    let cps = "for (var i = 0; i < 100001; i++) writeln('x');";
    let stream = "PARAM OperationName Runaway pocket\nSECTION\nENDSECTION\n";
    let error = engine::run(cps, &Value::Null, stream).unwrap_err().to_string();
    assert_named_limit(&error);
}

#[test]
fn runaway_in_the_second_operation_names_that_operation() {
    let cps = "function onSection() {\n\
        if (properties.OperationName !== 'Runaway pocket') return;\n\
        for (var i = 0; i < 100001; i++) writeln('x');\n\
    }";
    let stream = "PARAM OperationName Quiet pocket\nSECTION\nENDSECTION\n\
                  PARAM OperationName Runaway pocket\nSECTION\nENDSECTION\n";
    let error = engine::run(cps, &Value::Null, stream).unwrap_err().to_string();
    assert_named_limit(&error);
    assert!(!error.contains("Quiet pocket"), "the refusal must identify the current operation: {error}");
}

#[test]
fn resetting_public_output_counters_cannot_bypass_the_guard() {
    let cps = "function onSection() {\n\
        for (var i = 0; i < 100001; i++) {\n\
            _outputLines = 0; OUT.length = 0; writeln('x');\n\
        }\n\
    }";
    let stream = "PARAM OperationName Runaway pocket\nSECTION\nENDSECTION\n";
    let error = engine::run(cps, &Value::Null, stream).unwrap_err().to_string();
    assert_named_limit(&error);
}
