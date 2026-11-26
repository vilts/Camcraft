//! Provenance: original standalone post reservation regressions from issue #2604.
//! Gate map (M4.4): covers camcraft_post::post::engine,
//! camcraft_post::machine::reservations, the post binary.

use std::process::Command;

const SCRIPT: &str = "function onOpen() { writeln('ok'); }\n";

fn run(tag: &str, stream: &str, existing: Option<&str>) -> (bool, String, Option<String>) {
    let dir = std::env::temp_dir().join(format!("camcraft_reservation_cli_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temporary directory");
    let machine = dir.join("reserved.mch");
    let script = dir.join("minimal.cps");
    let input = dir.join("input.cnc");
    let output = dir.join("output.nc");
    std::fs::write(&machine, r#"{
        "general": {"model": "Reserved probe machine"},
        "tooling": {"reserved": [
            {"number":1,"use":"probe"},
            {"name":"PROBE","use":"probe"}
        ]}
    }"#).expect("machine file");
    std::fs::write(&script, SCRIPT).expect("script");
    std::fs::write(&input, stream).expect("stream");
    let _ = std::fs::remove_file(&output);
    if let Some(old) = existing {
        std::fs::write(&output, old).expect("existing output");
    }
    let result = Command::new(env!("CARGO_BIN_EXE_post"))
        .arg("--machine").arg(&machine).arg(&script).arg(&input).arg(&output)
        .output().expect("run post");
    let written = std::fs::read_to_string(&output).ok();
    let _ = std::fs::remove_dir_all(&dir);
    (result.status.success(), String::from_utf8_lossy(&result.stderr).into_owned(), written)
}

#[test]
fn standalone_post_refuses_cutter_in_reserved_probe_pocket_before_writing() {
    let stream = "PARAM ToolKind flat end mill\nTOOL 1 End mill\nSECTION\nFeed 0 0 0 F100000\nENDSECTION\n";
    let (ok, error, written) = run("cutter_number", stream, None);
    assert!(!ok, "a cutter in reserved T1 must refuse; stderr: {error}");
    assert!(error.contains("End mill") && error.contains("probe") && error.contains('1'), "{error}");
    assert!(written.is_none(), "a refused tool must not create an output file");
}

#[test]
fn standalone_post_keeps_existing_output_when_probe_pocket_is_used_for_cutting() {
    let stream = "PARAM ToolKind flat end mill\nTOOL 1 End mill\nSECTION\nENDSECTION\n";
    let (ok, error, written) = run("existing_output", stream, Some("previous verified program\n"));
    assert!(!ok, "reserved pocket must refuse; stderr: {error}");
    assert_eq!(written.as_deref(), Some("previous verified program\n"), "refusal preserves the old program");
}

#[test]
fn standalone_post_refuses_cutter_with_reserved_probe_name() {
    let stream = "PARAM ToolKind flat end mill\nTOOL 12 PROBE\nSECTION\nENDSECTION\n";
    let (ok, error, written) = run("cutter_name", stream, None);
    assert!(!ok, "a cutter named PROBE must refuse; stderr: {error}");
    assert!(error.contains("PROBE") && error.to_ascii_lowercase().contains("reserved"), "{error}");
    assert!(written.is_none());
}

#[test]
fn standalone_post_refuses_reserved_tool_without_stated_kind() {
    let stream = "TOOL 1 Unknown tool\nSECTION\nENDSECTION\n";
    let (ok, error, written) = run("missing_kind", stream, None);
    assert!(!ok, "an unknown kind cannot claim the probe pocket; stderr: {error}");
    assert!(error.contains("Unknown tool") && error.contains("probe"), "{error}");
    assert!(written.is_none());
}

#[test]
fn standalone_post_allows_stated_touch_probe_in_reserved_pocket() {
    // App dumps ToolKind::Probe through its Display implementation: "touch probe".
    let stream = "PARAM ToolKind touch probe\nTOOL 1 PROBE\nSECTION\nENDSECTION\n";
    let (ok, error, written) = run("probe", stream, None);
    assert!(ok, "{error}");
    assert_eq!(written.as_deref().map(str::trim), Some("ok"));
}

#[test]
fn standalone_post_does_not_reuse_previous_probe_kind_for_unknown_tool() {
    let stream = "PARAM ToolKind touch probe\nTOOL 1 PROBE\nSECTION\nENDSECTION\nTOOL 1 Unknown tool\nSECTION\nENDSECTION\n";
    let (ok, error, written) = run("stale_probe_kind", stream, None);
    assert!(!ok, "the next tool needs its own kind; stderr: {error}");
    assert!(error.contains("Unknown tool") && error.contains("probe"), "{error}");
    assert!(written.is_none());
}

#[test]
fn standalone_post_allows_probe_kind_stated_after_tool() {
    let stream = "TOOL 1 PROBE\nPARAM ToolKind touch probe\nSECTION\nENDSECTION\n";
    let (ok, error, written) = run("probe_kind_after_tool", stream, None);
    assert!(ok, "the kind may follow the tool before its section; {error}");
    assert_eq!(written.as_deref().map(str::trim), Some("ok"));
}

#[test]
fn standalone_post_refuses_probe_changed_to_cutter_inside_section() {
    let stream = "PARAM ToolKind touch probe\nTOOL 1 PROBE\nSECTION\nPARAM ToolKind flat end mill\nENDSECTION\n";
    let (ok, error, written) = run("probe_retyped_inside_section", stream, None);
    assert!(!ok, "a section cannot change the reserved probe into a cutter; stderr: {error}");
    assert!(error.contains("PROBE") && error.contains("probe"), "{error}");
    assert!(written.is_none(), "the entire stream must be checked before output creation");
}

#[test]
fn standalone_post_refuses_cutting_with_reserved_probe_reused_without_new_tool() {
    let stream = "PARAM ToolKind touch probe\nTOOL 1 PROBE\nSECTION\nENDSECTION\nPARAM ToolKind flat end mill\nSECTION\nFeed 0 0 0 F100000\nENDSECTION\n";
    let (ok, error, written) = run("probe_reused_for_cutting", stream, None);
    assert!(!ok, "the selected probe remains in the spindle; stderr: {error}");
    assert!(error.contains("PROBE") && error.contains("probe"), "{error}");
    assert!(written.is_none());
}

#[test]
fn standalone_post_refuses_cutting_without_any_tool_selector_on_reserved_machine() {
    let stream = "PARAM ToolKind flat end mill\nSECTION\nFeed 0 0 0 F100000\nENDSECTION\n";
    let (ok, error, written) = run("missing_tool_selector", stream, None);
    assert!(!ok, "the post must not guess a selector for cutting; stderr: {error}");
    let lower = error.to_ascii_lowercase();
    assert!(lower.contains("tool") && (lower.contains("missing") || lower.contains("selector") || lower.contains("no tool")), "{error}");
    assert!(written.is_none());
}

#[test]
fn standalone_post_allows_probe_reused_across_sections_without_repeated_tool() {
    let stream = "PARAM ToolKind touch probe\nTOOL 1 PROBE\nSECTION\nENDSECTION\nPARAM ToolKind touch probe\nSECTION\nRapid 1000 0 0\nENDSECTION\n";
    let (ok, error, written) = run("probe_reused_for_probing", stream, None);
    assert!(ok, "a fresh probe kind may reuse the selected probe; {error}");
    assert_eq!(written.as_deref().map(str::trim), Some("ok"));
}
