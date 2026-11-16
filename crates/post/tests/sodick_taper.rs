//! Provenance: synthetic streams from our CNC intermediate contract (#1496).
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

fn records(angle: Option<f64>, before_section: bool) -> Vec<Record> {
    let mut records = vec![
        Record::Param {
            name: "OperationName".into(),
            value: json!("Punch1"),
        },
        Record::Param {
            name: "WorkOffset".into(),
            value: json!("G54"),
        },
        Record::Param {
            name: "EdmCondition".into(),
            value: json!("C110"),
        },
    ];
    if !before_section {
        records.extend([Record::Section { begin: true }, mv(MoveKind::Rapid, -30.0)]);
    }
    if let Some(angle) = angle {
        // A direct post stream must not silently flatten this move when
        // the section's WireTaper parameter is absent.
        records.push(Record::Param {
            name: "WireAngle".into(),
            value: json!(angle),
        });
    }
    if before_section {
        records.extend([Record::Section { begin: true }, mv(MoveKind::Rapid, -30.0)]);
    }
    records.extend([mv(MoveKind::Feed, -20.0), Record::Section { begin: false }]);
    records
}

fn run(records: &[Record]) -> engine::PostOutput {
    let machine = json!({"kind": "wire_edm", "name": "Synthetic wire", "edm": {"awt": true}});
    engine::run(SCRIPT, &machine, &cnc::serialize(records)).expect("synthetic stream runs")
}

#[test]
fn sodick_refuses_angle_words_even_without_section_taper() {
    for angle in [3.0, -3.0, 0.0] {
        let out = run(&records(Some(angle), false));
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains("Punch1") && w.contains("angle-word")),
            "angle {angle}: {:?}\n{}",
            out.warnings,
            out.program
        );
        for word in ["G01", "T91", "C110"] {
            assert!(
                !out.program
                    .lines()
                    .filter(|line| !line.starts_with('('))
                    .any(|line| line.split_whitespace().any(|w| w == word)),
                "angle {angle}: {}",
                out.program
            );
        }
    }
}

#[test]
fn sodick_vertical_stream_still_posts_without_angle_words() {
    let out = run(&records(None, false));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(
        out.program.lines().any(|line| line == "G01 X-20. Y0."),
        "{}",
        out.program
    );
}

#[test]
fn sodick_refuses_angle_words_before_section_start() {
    for angle in [3.0, -3.0, 0.0] {
        // The app emits operation parameters before Section { begin: true }.
        let out = run(&records(Some(angle), true));
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains("Punch1") && w.contains("angle-word")),
            "angle {angle}: {:?}\n{}",
            out.warnings,
            out.program
        );
        for word in ["G01", "T91", "C110"] {
            assert!(
                !out.program
                    .lines()
                    .filter(|line| !line.starts_with('('))
                    .any(|line| line.split_whitespace().any(|w| w == word)),
                "angle {angle}: {}",
                out.program
            );
        }
    }
}

#[test]
fn sodick_angle_refusal_resets_at_section_end() {
    let mut stream = records(Some(3.0), true);
    let mut vertical = records(None, true);
    vertical[0] = Record::Param {
        name: "OperationName".into(),
        value: json!("Vertical2"),
    };
    stream.extend(vertical);
    let out = run(&stream);
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert!(out.warnings[0].contains("Punch1"), "{:?}", out.warnings);
    for line in ["G01 X-20. Y0.", "T91", "C110"] {
        assert_eq!(
            out.program.lines().filter(|actual| *actual == line).count(),
            1,
            "{}",
            out.program
        );
    }
}
