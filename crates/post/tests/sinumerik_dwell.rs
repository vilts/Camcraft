//! #2357: original dwell refusal regressions from Siemens ISO dialects 07/2021 §6.3.
//! Gate map: covers assets/posts/sinumerik.cps, camcraft_post::post::engine,
//! camcraft_post::post::cnc.

use camcraft_post::cl::{CycleKind, MoveKind};
use camcraft_post::post::{
    cnc::{self, CycleRec, MoveRec, Record},
    engine,
};
use serde_json::json;
use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(rel),
    )
    .expect("fixture")
}

fn movement(kind: MoveKind, z: f64, feed: Option<f64>) -> Record {
    Record::Move(MoveRec {
        kind,
        x: 10.0,
        y: 10.0,
        z,
        feed,
        center: None,
        ccw: false,
        plane: Default::default(),
        abc: None,
    })
}

fn stream(kind: CycleKind, dwell: f64) -> String {
    let records = vec![
        Record::Param {
            name: "SpindleSpeed".into(),
            value: json!(900.0),
        },
        Record::Param {
            name: "ClearancePlane".into(),
            value: json!(10.0),
        },
        Record::Param {
            name: "RetractPlane".into(),
            value: json!(10.0),
        },
        Record::Tool {
            number: 4,
            name: "CYCLE TOOL".into(),
        },
        Record::Section { begin: true },
        movement(MoveKind::Rapid, 2.0, None),
        Record::Cycle(CycleRec {
            kind,
            x: 10.0,
            y: 10.0,
            r_z: 2.0,
            bottom_z: -12.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell,
            first_move: 1,
            feed: Some(120.0),
        }),
        movement(MoveKind::Feed, -12.0, Some(120.0)),
        movement(MoveKind::Feed, 2.0, Some(120.0)),
        movement(MoveKind::Rapid, 10.0, None),
        Record::Section { begin: false },
    ];
    let stream = cnc::serialize(&records);
    let parsed = cnc::parse(&stream).expect("fixture parses");
    let cycle = parsed
        .iter()
        .find_map(|record| match record {
            Record::Cycle(cycle) => Some(cycle),
            _ => None,
        })
        .expect("cycle survives serialization");
    assert_eq!(cycle.kind, kind);
    assert!(
        (cycle.dwell - dwell).abs() < 1e-10,
        "positive TAP/REAM dwell reaches the engine"
    );
    stream
}

fn run(stream: &str) -> engine::PostOutput {
    let machine = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine");
    engine::run(&read("assets/posts/sinumerik.cps"), &machine, stream).expect("post engine")
}

fn has_word(program: &str, word: &str) -> bool {
    program.split_whitespace().any(|w| w == word)
}

fn refuses(kind: CycleKind, g: &str) {
    for dwell in [0.001, 0.25] {
        let out = run(&stream(kind, dwell));
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains(g) && w.contains("dwell")),
            "{g} dwell {dwell} needs a named refusal: {:?}",
            out.warnings
        );
        assert!(
            !has_word(&out.program, g),
            "refused {g} must not be posted:\n{}",
            out.program
        );
        assert!(
            !has_word(&out.program, "G1"),
            "refused cycle must not fall back to cuts:\n{}",
            out.program
        );
    }
    // A hand-authored fractional millisecond must not round silently to zero.
    let raw = stream(kind, 0.001).replace("P 1 ", "P 0.1 ");
    let parsed = cnc::parse(&raw).expect("fractional millisecond fixture parses");
    assert!(parsed.iter().any(|record| matches!(record,
        Record::Cycle(cycle) if (cycle.dwell - 0.0001).abs() < 1e-10)));
    let out = run(&raw);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains(g) && w.contains("dwell")),
        "{g} sub-ms dwell needs a named refusal: {:?}",
        out.warnings
    );
    assert!(
        !has_word(&out.program, g),
        "refused sub-ms {g} must not be posted"
    );
}

#[test]
fn sinumerik_g84_dwell_is_refused_by_name() {
    refuses(CycleKind::Tap, "G84");
}

#[test]
fn sinumerik_g85_dwell_is_refused_by_name() {
    refuses(CycleKind::Ream, "G85");
}

#[test]
fn sinumerik_g84_and_g85_without_dwell_still_post() {
    for (kind, g) in [(CycleKind::Tap, "G84"), (CycleKind::Ream, "G85")] {
        let out = run(&stream(kind, 0.0));
        assert!(out.warnings.is_empty(), "{g}: {:?}", out.warnings);
        let line = out
            .program
            .lines()
            .find(|line| has_word(line, g))
            .expect("cycle posts");
        assert!(
            !line.split_whitespace().any(|word| word.starts_with('P')),
            "{line}"
        );
        assert!(
            !has_word(&out.program, "G1"),
            "cycle replaces its expanded cuts"
        );
    }
}
