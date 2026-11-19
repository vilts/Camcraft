//! G73/G84 dwell regression from the public cycle contract and issue #2356.
//! Gate map: covers assets/posts/fanuc.cps, haas.cps, mazak.cps, doosan-dnm.cps,
//! camcraft_post::post::engine and camcraft_post::post::cnc.

use camcraft_post::cl::{CycleKind, MoveKind};
use camcraft_post::post::{
    cnc::{self, CycleRec, MoveRec, Record},
    engine,
};
use serde_json::json;
use std::path::Path;

const POSTS: [&str; 4] = ["fanuc", "haas", "mazak", "doosan-dnm"];

fn read(rel: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(rel),
    )
    .expect("fixture")
}

fn stream(kind: CycleKind, dwells: &[f64]) -> String {
    let mut records = vec![
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
            name: "DRILL".into(),
        },
        Record::Section { begin: true },
        Record::Move(MoveRec {
            kind: MoveKind::Rapid,
            x: 10.0,
            y: 10.0,
            z: 2.0,
            feed: None,
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: None,
        }),
    ];
    // No intervening move: a later request reaches the open-group shortcut.
    for &dwell in dwells {
        records.push(Record::Cycle(CycleRec {
            kind,
            x: 10.0,
            y: 10.0,
            r_z: 2.0,
            bottom_z: -12.0,
            entry_z: 0.0,
            peck: 3.0,
            dwell,
            first_move: 1,
            feed: Some(120.0),
        }));
    }
    records.push(Record::Section { begin: false });
    cnc::serialize(&records)
}

fn run(stem: &str, stream: &str) -> engine::PostOutput {
    let machine = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine");
    engine::run(&read(&format!("assets/posts/{stem}.cps")), &machine, stream).expect("post engine")
}

fn has_word(program: &str, word: &str) -> bool {
    program.split_whitespace().any(|w| w == word)
}

fn refuses(stem: &str, kind: CycleKind, g: &str) {
    for dwell in [0.001, 0.5] {
        let out = run(stem, &stream(kind, &[dwell]));
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains(&format!("canned cycle {g}")) && w.contains("dwell")),
            "{stem}: {g} dwell {dwell} needs a named refusal: {:?}",
            out.warnings
        );
        assert!(
            !has_word(&out.program, g),
            "{stem}: refused cycle must not be emitted:\n{}",
            out.program
        );
    }
    let out = run(stem, &stream(kind, &[0.0, 0.5]));
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains(g) && w.contains("dwell")),
        "{stem}: an open {g} group must not swallow a later dwell: {:?}",
        out.warnings
    );
    assert!(
        !out.program.lines().any(|line| {
            let words: Vec<_> = line.split_whitespace().collect();
            words.iter().any(|w| w.starts_with('X'))
                && words.iter().all(|w| w.starts_with(['X', 'Y', 'N']))
        }),
        "{stem}: refused second hole must not emit a modal XY-only cycle point:\n{}",
        out.program
    );
    // Fractional millisecond input is legal in a hand-authored stream. Preserve
    // that premise explicitly so millisecond serialization cannot erase it.
    let raw = stream(kind, &[0.001]).replace("P 1 ", "P 0.1 ");
    let parsed = cnc::parse(&raw).expect("fractional millisecond stream");
    let dwell = parsed
        .iter()
        .find_map(|r| match r {
            Record::Cycle(c) => Some(c.dwell),
            _ => None,
        })
        .unwrap();
    assert!(
        (dwell - 0.0001).abs() < 1e-10,
        "raw fixture retains sub-ms dwell"
    );
    let out = run(stem, &raw);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains(g) && w.contains("dwell")),
        "{stem}: raw {g} dwell must not round silently to zero: {:?}",
        out.warnings
    );
    assert!(
        !has_word(&out.program, g),
        "{stem}: sub-ms refusal emits no {g}"
    );
}

#[test]
fn fanuc_refuses_g73_dwell() {
    refuses("fanuc", CycleKind::Peck, "G73");
}
#[test]
fn fanuc_refuses_g84_dwell() {
    refuses("fanuc", CycleKind::Tap, "G84");
}
#[test]
fn haas_refuses_g73_dwell() {
    refuses("haas", CycleKind::Peck, "G73");
}
#[test]
fn haas_refuses_g84_dwell() {
    refuses("haas", CycleKind::Tap, "G84");
}
#[test]
fn mazak_refuses_g73_dwell() {
    refuses("mazak", CycleKind::Peck, "G73");
}
#[test]
fn mazak_refuses_g84_dwell() {
    refuses("mazak", CycleKind::Tap, "G84");
}
#[test]
fn doosan_refuses_g73_dwell() {
    refuses("doosan-dnm", CycleKind::Peck, "G73");
}
#[test]
fn doosan_refuses_g84_dwell() {
    refuses("doosan-dnm", CycleKind::Tap, "G84");
}

#[test]
fn g73_and_g84_without_dwell_remain_available() {
    for stem in POSTS {
        for (kind, g) in [(CycleKind::Peck, "G73"), (CycleKind::Tap, "G84")] {
            let out = run(stem, &stream(kind, &[0.0]));
            assert!(out.warnings.is_empty(), "{stem} {g}: {:?}", out.warnings);
            assert!(
                has_word(&out.program, g),
                "{stem}: zero-dwell {g} remains available:\n{}",
                out.program
            );
        }
    }
}
