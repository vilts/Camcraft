//! Provenance: issue #2069's user-source diagnostic contract, using original scripts.
//! Gate map: covers camcraft_post::post::js.

use camcraft_post::post::js::Script;

#[test]
fn syntax_errors_report_exact_user_line_and_column() {
    let error = match Script::load("// user line one\n\nvar x = ;\n") {
        Ok(_) => panic!("invalid user source must be refused"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("line 3, col 9"), "{error}");
}

#[test]
fn runtime_errors_report_exact_user_line_and_column() {
    let mut script = Script::load("// user line one\nfunction onOpen() {\n  throw new Error('user failure');\n}\n")
        .expect("valid user source");
    let error = script.call("onOpen", "").expect_err("user exception must be refused").to_string();
    assert!(error.contains("user failure"), "{error}");
    assert!(error.contains("user.cps:3:9"), "{error}");
}

fn load_error(source: &str) -> String {
    match Script::load(source) {
        Ok(_) => panic!("script must exceed its execution budget"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn endless_top_level_loop_is_refused() {
    let error = load_error("while (true) {}");
    assert!(error.contains("loop iteration"), "{error}");
}

#[test]
fn endless_hook_loop_is_refused() {
    let mut script = Script::load("function onOpen() { while (true) {} }").unwrap();
    let error = script.call("onOpen", "").unwrap_err().to_string();
    assert!(error.contains("loop iteration"), "{error}");
}

#[test]
fn excessive_recursion_is_refused() {
    let error = load_error("function descend(n) { if (n) descend(n - 1); } descend(100);");
    assert!(error.contains("recursive calls"), "{error}");
}

#[test]
fn excessive_stack_is_refused() {
    let error = load_error("function f() {} f(...Array(5000).fill(0));");
    assert!(error.contains("stack"), "{error}");
}

#[test]
fn output_limit_covers_every_line_writer() {
    for write in ["writeln('x');", "writeBlock('x');", "writeBlankLine();", "write('x'); writeln();"] {
        let error = load_error(&format!("for (var i = 0; i < 100001; i++) {{ {write} }}"));
        assert!(error.contains("output") && error.contains("limit"), "{write}: {error}");
    }
}

#[test]
fn output_limit_counts_lines_across_hooks() {
    let mut script = Script::load(
        "function onOpen() { for (var i = 0; i < 60000; i++) writeln('x'); }\n\
         function onClose() { for (var i = 0; i < 60000; i++) writeln('y'); }",
    ).unwrap();
    script.call("onOpen", "").expect("first hook fits the budget");
    let error = script.call("onClose", "").unwrap_err().to_string();
    assert!(error.contains("output") && error.contains("limit"), "{error}");
}

#[test]
fn direct_output_buffer_writes_are_refused_at_run_boundary() {
    let cps = "function onOpen() { for (var i = 0; i < 100001; i++) OUT.push('x'); }";
    let error = camcraft_post::post::post(cps, &serde_json::json!({}), "")
        .unwrap_err().to_string();
    assert!(error.contains("output") && error.contains("limit"), "{error}");
}

#[test]
fn direct_output_buffer_writes_are_refused_before_drain() {
    let mut script = Script::load("for (var i = 0; i < 100001; i++) OUT.push('x');").unwrap();
    let error = script.drain().unwrap_err().to_string();
    assert!(error.contains("output") && error.contains("limit"), "{error}");
}

#[test]
fn caught_output_limit_error_is_still_refused() {
    let cps = "function onOpen() {\n  try {\n    for (var i = 0; i < 100001; i++) writeBlock('G01 X1');\n  } catch (e) {}\n}\n";
    let error = camcraft_post::post::post(cps, &serde_json::json!({}), "")
        .unwrap_err()
        .to_string();
    assert!(error.contains("output") && error.contains("limit"), "{error}");

    let mut script = Script::load(cps).unwrap();
    let call_err = script.call("onOpen", "").unwrap_err().to_string();
    assert!(call_err.contains("output") && call_err.contains("limit"), "{call_err}");
    let drain_err = script.drain().unwrap_err().to_string();
    assert!(drain_err.contains("output") && drain_err.contains("limit"), "{drain_err}");

    let top_level = "try { for (var i = 0; i < 100001; i++) writeBlock('G01 X1'); } catch (e) {}";
    let top_err = camcraft_post::post::post(top_level, &serde_json::json!({}), "")
        .unwrap_err()
        .to_string();
    assert!(top_err.contains("output") && top_err.contains("limit"), "{top_err}");

    let cleared = "function onOpen() {\n  try {\n    for (var i = 0; i < 100001; i++) writeBlock('G01 X1');\n  } catch (e) { _outputLimitExceeded = false; _outputLines = 0; OUT.length = 0; }\n}\n";
    let cleared_err = camcraft_post::post::post(cleared, &serde_json::json!({}), "")
        .unwrap_err()
        .to_string();
    assert!(cleared_err.contains("output") && cleared_err.contains("limit"), "{cleared_err}");
}
