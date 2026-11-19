//! #2473: a move with no A word inside an operation indexed away from A0
//! is refused by name; the post never turns A back to 0 on it.
//! Provenance: original record streams over our microARC machine and post.
//!
//! Gate map (M4.4): covers assets/posts/tormach.cps, camcraft_post::post::engine,
//! camcraft_post::post::machine.

use camcraft_post::cl::MoveKind;
use camcraft_post::machine::parse_mch;
use camcraft_post::post::{cnc, engine, machine};
use serde_json::json;

const POST: &str = include_str!("../../../assets/posts/tormach.cps");
const MICROARC: &str = include_str!("../../../assets/machines/tormach-770mx-microarc4.mch");

fn mv(kind: MoveKind, z: f64, abc: Option<[f64; 3]>) -> cnc::Record {
    let feed = (kind != MoveKind::Rapid).then_some(600.0);
    cnc::Record::Move(cnc::MoveRec {
        kind,
        x: 0.0,
        y: 0.0,
        z,
        feed,
        center: None,
        ccw: false,
        plane: Default::default(),
        abc,
    })
}

/// One operation: its moves, after the tool, spindle and planes it needs.
fn stream(moves: Vec<cnc::Record>) -> String {
    let mut records = vec![
        cnc::Record::Param {
            name: "OperationName".into(),
            value: json!("Index"),
        },
        cnc::Record::Param {
            name: "SpindleSpeed".into(),
            value: json!(6000),
        },
        cnc::Record::Param {
            name: "ClearancePlane".into(),
            value: json!(40.0),
        },
        cnc::Record::Param {
            name: "RetractPlane".into(),
            value: json!(20.0),
        },
        cnc::Record::Param {
            name: "WorkOffset".into(),
            value: json!("G54"),
        },
        cnc::Record::Tool {
            number: 3,
            name: "BALL".into(),
        },
        cnc::Record::Section { begin: true },
    ];
    records.extend(moves);
    records.push(cnc::Record::Section { begin: false });
    cnc::serialize(&records)
}

fn run(moves: Vec<cnc::Record>) -> engine::PostOutput {
    let mch = machine::mirror(&parse_mch(MICROARC).expect("shipped microARC parses"));
    engine::run_with(POST, &mch, &stream(moves), &json!({"Units": "mm"})).expect("post runs")
}

/// The A words of the program's blocks (comments skipped).
fn angles(program: &str) -> Vec<f64> {
    program
        .lines()
        .filter(|line| !line.starts_with('('))
        .flat_map(str::split_whitespace)
        .filter_map(|word| word.strip_prefix('A').and_then(|a| a.parse().ok()))
        .collect()
}

/// An operation indexed to A90, then one move of `kind` with no A word.
fn indexed_then_bare(kind: MoveKind) -> engine::PostOutput {
    let a90 = Some([90.0, 0.0, 0.0]);
    run(vec![
        mv(MoveKind::Rapid, 30.0, a90),
        mv(MoveKind::Feed, 10.0, a90),
        mv(kind, 5.0, None),
    ])
}

fn assert_refused_without_a0(out: &engine::PostOutput) {
    let warning = out.warnings.join("\n");
    assert!(
        warning.contains("move in operation Index states no A angle while the operation holds A"),
        "{warning}"
    );
    assert!(
        warning.contains("holds A90") || warning.contains("holds A-90"),
        "{warning}"
    );
    let posted = angles(&out.program);
    assert!(!posted.is_empty(), "the section indexes A first:\n{}", out.program);
    assert!(
        posted.iter().all(|a| (a.abs() - 90.0).abs() < 1e-9),
        "A never turns back to 0: {posted:?}\n{}",
        out.program
    );
}

#[test]
fn a_rapid_with_no_a_word_in_an_indexed_operation_is_refused() {
    assert_refused_without_a0(&indexed_then_bare(MoveKind::Rapid));
}

#[test]
fn a_linear_move_with_no_a_word_in_an_indexed_operation_is_refused() {
    assert_refused_without_a0(&indexed_then_bare(MoveKind::Feed));
}

/// A stream that states its turn to A0 keeps posting: after the A0 word
/// the operation holds A0, and a move with no A word stays upright.
#[test]
fn a_stated_turn_to_a0_then_a_move_with_no_a_word_still_posts() {
    let out = run(vec![
        mv(MoveKind::Rapid, 30.0, Some([90.0, 0.0, 0.0])),
        mv(MoveKind::Feed, 10.0, Some([90.0, 0.0, 0.0])),
        mv(MoveKind::Feed, 10.0, Some([0.0, 0.0, 0.0])),
        mv(MoveKind::Feed, 5.0, None),
    ]);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(angles(&out.program).last(), Some(&0.0), "{}", out.program);
}
