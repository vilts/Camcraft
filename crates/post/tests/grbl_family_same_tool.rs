//! Provenance: #3445, original tests of our stream and the grbl-family post contract.
//! Gate map: covers assets/posts/grblhal.cps, assets/posts/grbl.cps,
//! assets/posts/tinyg.cps, camcraft_post::post::engine.
//!
//! A section that keeps the tool this program already loaded writes no
//! tool change (grblHAL / TinyG `T M6`, the Grbl 1.1 `M0` stop). A new
//! tool, a return to an earlier tool, and a tool after a program stop
//! or optional stop each change again.

use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine};
use std::path::Path;

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path))
        .expect("fixture")
}

/// One section per entry: the tool number and its after-operation ask.
fn post(script: &str, mch: &str, sections: &[(u32, &str)]) -> String {
    let mut stream = String::new();
    for (tool, after) in sections {
        if !after.is_empty() {
            stream.push_str(&format!("PARAM AfterOperation {after}\n"));
        }
        stream.push_str(&format!(
            "PARAM SpindleSpeed 1500\nTOOL {tool} FLAT\nPARAM ClearancePlane 25.4\nPARAM RetractPlane 12.7\nSECTION\nRapid 20000 10000 2000\nPlunge 20000 10000 -10000 F120000\nRapid 20000 10000 2000\nENDSECTION\n"
        ));
    }
    let mch = parse_mch(&read(mch)).expect("machine");
    let out = engine::run(&read(script), &machine::mirror(&mch), &stream).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out.program
}

fn count(program: &str, line: &str) -> usize {
    program.lines().filter(|l| *l == line).count()
}

/// The first X/Y traverse of each section still comes after a Z-only rapid.
fn each_traverse_states_its_height(program: &str, sections: usize) {
    let lines: Vec<_> = program.lines().collect();
    let traverses: Vec<_> = (1..lines.len()).filter(|i| lines[*i] == "G0 X20 Y10").collect();
    assert_eq!(traverses.len(), sections, "{program}");
    for i in traverses {
        assert!(lines[i - 1].starts_with("G0 Z"), "{program}");
    }
}

fn changes(program: &str) -> Vec<&str> {
    program.lines().filter(|l| l.ends_with(" M6")).collect()
}

const GRBLHAL: (&str, &str) = ("assets/posts/grblhal.cps", "assets/machines/sienci-altmill-4x4.mch");
const GRBL: (&str, &str) = ("assets/posts/grbl.cps", "assets/machines/grbl-3axis.mch");
const TINYG: (&str, &str) = ("assets/posts/tinyg.cps", "assets/machines/tinyg-3axis.mch");

#[test]
fn grblhal_repeated_tool_writes_one_tool_change() {
    let (s, m) = GRBLHAL;
    let program = post(s, m, &[(1, ""), (1, ""), (1, "")]);
    assert_eq!(changes(&program), ["T1 M6"], "{program}");
    each_traverse_states_its_height(&program, 3);
}

#[test]
fn grblhal_new_tool_return_and_stop_each_change_again() {
    let (s, m) = GRBLHAL;
    let program = post(s, m, &[(1, ""), (1, ""), (7, ""), (1, "")]);
    assert_eq!(changes(&program), ["T1 M6", "T7 M6", "T1 M6"], "{program}");
    // The operator may take the tool out at a program stop or optional stop.
    let program = post(s, m, &[(1, "stop"), (1, "")]);
    assert_eq!(changes(&program), ["T1 M6", "T1 M6"], "{program}");
    let program = post(s, m, &[(1, "optional-stop"), (1, "")]);
    assert_eq!(count(&program, "M1"), 1, "{program}");
    assert_eq!(changes(&program), ["T1 M6", "T1 M6"], "{program}");
}

#[test]
fn grbl_repeated_tool_writes_no_tool_change_stop() {
    let (s, m) = GRBL;
    let program = post(s, m, &[(1, ""), (1, ""), (1, "")]);
    assert_eq!(count(&program, "M0"), 0, "{program}");
    assert!(!program.contains("TOOL CHANGE"), "{program}");
    each_traverse_states_its_height(&program, 3);
}

#[test]
fn grbl_new_tool_return_and_stop_each_change_again() {
    let (s, m) = GRBL;
    let program = post(s, m, &[(1, ""), (1, ""), (7, ""), (1, "")]);
    assert_eq!(count(&program, "M0"), 2, "{program}");
    assert_eq!(program.matches("TOOL CHANGE - LOAD").count(), 2, "{program}");
    // The program stop itself is one M0; the tool after it is asked for again.
    let program = post(s, m, &[(1, "stop"), (1, "")]);
    assert_eq!(count(&program, "M0"), 2, "{program}");
    // An optional stop (M1) also makes the tool unknown; the next section
    // writes one TOOL CHANGE - LOAD T1 note and its M0 after the M1.
    let program = post(s, m, &[(1, "optional-stop"), (1, "")]);
    assert_eq!(count(&program, "M1"), 1, "{program}");
    assert_eq!(count(&program, "M0"), 1, "{program}");
    assert_eq!(program.matches("TOOL CHANGE - LOAD T1").count(), 1, "{program}");
    let lines: Vec<_> = program.lines().collect();
    let m1 = lines.iter().position(|l| *l == "M1").expect("M1");
    let reload = lines
        .iter()
        .position(|l| l.contains("TOOL CHANGE - LOAD T1"))
        .expect("TOOL CHANGE - LOAD T1");
    let m0 = lines.iter().position(|l| *l == "M0").expect("M0");
    assert!(m1 < reload && reload < m0, "{program}");
}

#[test]
fn tinyg_repeated_tool_writes_one_tool_change() {
    let (s, m) = TINYG;
    let program = post(s, m, &[(1, ""), (1, ""), (1, "")]);
    assert_eq!(changes(&program), ["T1 M6"], "{program}");
    assert_eq!(count(&program, "M0"), 1, "{program}");
    each_traverse_states_its_height(&program, 3);
}

#[test]
fn tinyg_new_tool_return_and_stop_each_change_again() {
    let (s, m) = TINYG;
    let program = post(s, m, &[(1, ""), (1, ""), (7, ""), (1, "")]);
    assert_eq!(changes(&program), ["T1 M6", "T7 M6", "T1 M6"], "{program}");
    let program = post(s, m, &[(1, "stop"), (1, "")]);
    assert_eq!(changes(&program), ["T1 M6", "T1 M6"], "{program}");
    let program = post(s, m, &[(1, "optional-stop"), (1, "")]);
    assert_eq!(count(&program, "M1"), 1, "{program}");
    assert_eq!(changes(&program), ["T1 M6", "T1 M6"], "{program}");
}
