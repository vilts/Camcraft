//! Provenance: #1626, original tests of our stream and Tormach post contract.
//! Gate map: covers assets/posts/tormach.cps, camcraft_post::post::engine.

use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine};
use std::path::Path;

fn read(path: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path),
    )
    .expect("fixture")
}

fn post(tools: &[u32]) -> String {
    let mut stream = String::new();
    for tool in tools {
        stream.push_str(&format!(
            "PARAM SpindleSpeed 1500\nTOOL {tool} DRILL\nPARAM ClearancePlane 25.4\nPARAM RetractPlane 12.7\nSECTION\nRapid 20000 10000 2000\nPlunge 20000 10000 -10000 F120000\nRapid 20000 10000 2000\nENDSECTION\n"
        ));
    }
    let mch = parse_mch(&read("assets/machines/tormach-770mx.mch")).expect("machine");
    let out = engine::run(
        &read("assets/posts/tormach.cps"),
        &machine::mirror(&mch),
        &stream,
    )
    .expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out.program
}

#[test]
fn repeated_tool_skips_g30_and_m6_but_keeps_compensation_lift_and_clearance() {
    let program = post(&[4, 4]);
    let lines: Vec<_> = program.lines().collect();
    assert_eq!(
        lines.iter().filter(|line| **line == "G30").count(),
        1,
        "{program}"
    );
    assert_eq!(
        lines.iter().filter(|line| **line == "T4 M6").count(),
        1,
        "{program}"
    );
    let compensation: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (*line == "G43 H4").then_some(index))
        .collect();
    assert_eq!(compensation.len(), 2, "{program}");
    for index in compensation {
        assert_eq!(lines.get(index + 1), Some(&"G53 G0 Z0"), "{program}");
        let xy = lines[index + 1..]
            .iter()
            .position(|line| *line == "G0 X20 Y10")
            .map(|offset| index + 1 + offset)
            .expect("section traverse");
        assert_eq!(lines.get(xy - 1), Some(&"G0 Z25.4"), "{program}");
    }
}

#[test]
fn a_different_tool_and_a_return_to_an_earlier_tool_each_change_again() {
    let program = post(&[4, 4, 7, 4]);
    let changes: Vec<_> = program
        .lines()
        .filter(|line| line.ends_with(" M6"))
        .collect();
    assert_eq!(changes, ["T4 M6", "T7 M6", "T4 M6"], "{program}");
    assert_eq!(
        program.lines().filter(|line| *line == "G30").count(),
        3,
        "{program}"
    );
    let first_change = program.find("T4 M6").expect("first tool change");
    assert!(program[..first_change].contains("G30\n"), "{program}");
}
