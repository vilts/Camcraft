//! Provenance: PR #1741 review of #1626; our stream fixtures.
//! Gate map: covers assets/posts/tormach.cps, camcraft_post::post::engine.

use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine};

const POST: &str = include_str!("../../../assets/posts/tormach.cps");
const MACHINE: &str = include_str!("../../../assets/machines/tormach-770mx.mch");
const SECTION: &str = "PARAM SpindleSpeed 1500\nTOOL 4 DRILL\nPARAM ClearancePlane 25.4\nPARAM RetractPlane 12.7\nSECTION\nRapid 20000 10000 2000\nENDSECTION\n";

fn post(stream: &str) -> String {
    let mch = parse_mch(MACHINE).expect("machine");
    let out = engine::run(POST, &machine::mirror(&mch), stream).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out.program
}

fn assert_reload_after_stop(program: &str, stop: &str, tool: &str) {
    let lines: Vec<_> = program.lines().collect();
    let at = lines
        .iter()
        .position(|line| *line == stop)
        .expect("stop emitted");
    assert_eq!(
        lines.iter().filter(|line| **line == tool).count(),
        2,
        "{program}"
    );
    let reload = lines[at + 1..]
        .iter()
        .position(|line| *line == tool)
        .map(|index| index + at + 1)
        .expect("tool reloaded after stop");
    assert_eq!(lines[reload - 1], "G30", "{program}");
    assert_eq!(lines[reload - 2], "G53 G0 Z0", "{program}");
    assert!(
        lines[at + 1..reload]
            .iter()
            .all(|line| !line.starts_with("G43")),
        "{program}"
    );
    assert_eq!(
        lines[reload + 1],
        tool.replace("T", "G43 H").replace(" M6", ""),
        "{program}"
    );
    assert_eq!(lines[reload + 2], "G53 G0 Z0", "{program}");
}

#[test]
fn same_tool_reloads_after_program_stop() {
    let program = post(&format!("PARAM AfterOperation stop\n{SECTION}{SECTION}"));
    assert_reload_after_stop(&program, "M0", "T4 M6");
}

#[test]
fn same_tool_reloads_after_optional_stop() {
    let program = post(&format!(
        "PARAM AfterOperation optional-stop\n{SECTION}{SECTION}"
    ));
    assert_reload_after_stop(&program, "M1", "T4 M6");
}

#[test]
fn same_tool_reloads_after_setup_change_stop() {
    let program = post(&format!(
        "PARAM AfterOperation stop\nPARAM SetupChange Back\n{SECTION}{SECTION}"
    ));
    assert!(
        program.contains("(MSG, SETUP CHANGE - NEXT: Back)\nM0\n"),
        "{program}"
    );
    assert_reload_after_stop(&program, "M0", "T4 M6");
}

#[test]
fn same_probe_reloads_after_probe_result_check() {
    let probe = include_str!("recorded/inputs/probe_t99_bore_none.cnc");
    let program = post(&format!("{probe}{probe}"));
    assert!(
        program.contains("(MSG, CHECK PROBE RESULT #190..)\nM0\n"),
        "{program}"
    );
    assert_reload_after_stop(&program, "M0", "T99 M6");
}
