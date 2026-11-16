//! Provenance: synthetic streams from our CNC intermediate contract (#1497):
//! `WireU` / `WireV` (the upper guide's offset from the lower one) and
//! `WireFourAxis`, as docs/CNC-INTERMEDIATE.md states them.
//! Gate map: covers assets/posts/sodick-wire.cps, camcraft_post::post::engine,
//! camcraft_post::post::cnc.

use camcraft_post::cl::{ArcPlane, MoveKind};
use camcraft_post::post::cnc::{self, MoveRec, Record};
use camcraft_post::post::engine;
use serde_json::json;

const SCRIPT: &str = include_str!("../../../assets/posts/sodick-wire.cps");

fn mv(kind: MoveKind, x: f64) -> Record {
    Record::Move(MoveRec {
        kind,
        x,
        y: 0.0,
        z: 0.0,
        feed: (kind == MoveKind::Feed).then_some(2.0),
        center: None,
        ccw: false,
        plane: ArcPlane::Xy,
        abc: None,
    })
}

fn param(name: &str, value: serde_json::Value) -> Record {
    Record::Param {
        name: name.into(),
        value,
    }
}

fn lean() -> [Record; 2] {
    [param("WireU", json!(1.5)), param("WireV", json!(-0.5))]
}

/// Where the stream states the upper guide offset.
#[derive(Clone, Copy, Debug)]
enum Lean {
    None,
    /// Before the section opens (the app writes operation PARAMs there).
    BeforeSection,
    /// On the section's first positioning move.
    OnRapid,
    /// On the first cut.
    OnCut,
}

fn section(op: &str, at: Lean, four_axis: bool) -> Vec<Record> {
    let mut r = vec![
        param("OperationName", json!(op)),
        param("WorkOffset", json!("G54")),
        param("EdmCondition", json!("C110")),
    ];
    if four_axis {
        r.push(param("WireFourAxis", json!("arc length")));
    }
    if matches!(at, Lean::BeforeSection) {
        r.extend(lean());
    }
    r.push(Record::Section { begin: true });
    if matches!(at, Lean::OnRapid) {
        r.extend(lean());
    }
    r.push(mv(MoveKind::Rapid, -30.0));
    if matches!(at, Lean::OnCut) {
        r.extend(lean());
    }
    r.extend([mv(MoveKind::Feed, -20.0), Record::Section { begin: false }]);
    r
}

fn run(records: &[Record]) -> engine::PostOutput {
    let machine = json!({"kind": "wire_edm", "name": "Synthetic wire", "edm": {"awt": true}});
    engine::run(SCRIPT, &machine, &cnc::serialize(records)).expect("synthetic stream runs")
}

fn count(out: &engine::PostOutput, word: &str) -> usize {
    out.program
        .lines()
        .filter(|line| !line.starts_with('('))
        .filter(|line| line.split_whitespace().any(|w| w == word))
        .count()
}

fn assert_nothing_cut(out: &engine::PostOutput, case: &str) {
    for word in ["T91", "C110", "G01", "G00", "T90"] {
        assert_eq!(count(out, word), 0, "{case}: {word}\n{}", out.program);
    }
}

#[test]
fn sodick_refuses_a_lean_before_the_first_rapid_and_threads_nothing() {
    for at in [Lean::OnRapid, Lean::BeforeSection] {
        let out = run(&section("Punch1", at, false));
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains("Punch1") && w.contains("leaning positioning move")),
            "{at:?}: {:?}\n{}",
            out.warnings,
            out.program
        );
        assert_nothing_cut(&out, &format!("{at:?}"));
    }
}

#[test]
fn sodick_still_refuses_a_leaning_cut() {
    let out = run(&section("Punch1", Lean::OnCut, false));
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("Punch1") && w.contains("leaning move")),
        "{:?}",
        out.warnings
    );
    assert_eq!(count(&out, "G01"), 0, "{}", out.program);
}

#[test]
fn sodick_still_refuses_a_four_axis_section_by_name() {
    for at in [Lean::None, Lean::OnRapid, Lean::OnCut] {
        let out = run(&section("Punch1", at, true));
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains("Punch1") && w.contains("4-axis cut")),
            "{at:?}: {:?}",
            out.warnings
        );
        assert_nothing_cut(&out, &format!("{at:?}"));
    }
}

#[test]
fn sodick_lean_refusal_resets_for_the_next_vertical_section() {
    let mut stream = section("Punch1", Lean::OnRapid, false);
    stream.extend(section("Vertical2", Lean::None, false));
    let out = run(&stream);
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert!(out.warnings[0].contains("Punch1"), "{:?}", out.warnings);
    // The vertical section posts whole, once: positioning, thread, condition,
    // cut, wire cut.
    for line in ["G40 G00 X-30. Y0.", "T91", "C110", "G01 X-20. Y0.", "T90"] {
        assert_eq!(
            out.program.lines().filter(|actual| *actual == line).count(),
            1,
            "{line}\n{}",
            out.program
        );
    }
}

#[test]
fn sodick_vertical_stream_posts_unchanged() {
    let out = run(&section("Vertical1", Lean::None, false));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let body: Vec<&str> = out
        .program
        .lines()
        .filter(|line| !line.starts_with('('))
        .collect();
    assert_eq!(
        body,
        [
            "G21",
            "G90",
            "G54",
            "G40 G00 X-30. Y0.",
            "T91",
            "C110",
            "G01 X-20. Y0.",
            "T90",
            "M02"
        ],
        "{}",
        out.program
    );
}
