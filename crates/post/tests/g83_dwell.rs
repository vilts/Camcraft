//! G83 dwell regression, from the public Fanuc cycle formats and Haas G83 documentation (#1820).
//! Gate map: covers assets/posts/fanuc.cps, haas.cps, mazak.cps, doosan-dnm.cps,
//! camcraft_post::post::engine and camcraft_post::post::cnc.

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

fn stream(dwells: &[f64]) -> String {
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
    // Back-to-back cycles at the same position retain the open cycle group.
    // A hop or XY traverse would cancel it and hide a stale dwell signature.
    for &dwell in dwells {
        records.push(Record::Cycle(CycleRec {
            kind: CycleKind::Deep,
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

fn run(stem: &str, dwells: &[f64]) -> engine::PostOutput {
    let machine = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine");
    engine::run(
        &read(&format!("assets/posts/{stem}.cps")),
        &machine,
        &stream(dwells),
    )
    .expect("post engine")
}

fn cycle_lines(program: &str) -> Vec<&str> {
    program
        .lines()
        .filter(|line| line.split_whitespace().any(|word| word == "G83"))
        .collect()
}

fn refuses(stem: &str) {
    for dwell in [0.001, 0.5] {
        let out = run(stem, &[dwell]);
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains("G83") && w.contains("dwell")),
            "{stem}: G83 dwell {dwell} must be refused by name: {:?}",
            out.warnings
        );
        assert!(
            cycle_lines(&out.program).is_empty(),
            "{stem}: refused G83 must not be emitted:\n{}",
            out.program
        );
    }
}

#[test]
fn fanuc_refuses_g83_dwell() {
    refuses("fanuc");
}

#[test]
fn mazak_refuses_g83_dwell() {
    refuses("mazak");
}

#[test]
fn doosan_refuses_g83_dwell() {
    refuses("doosan-dnm");
}

#[test]
fn g83_without_dwell_remains_available() {
    for stem in ["fanuc", "mazak", "doosan-dnm", "haas"] {
        let out = run(stem, &[0.0]);
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let lines = cycle_lines(&out.program);
        assert_eq!(lines.len(), 1, "{stem}: {}", out.program);
        let p = lines[0]
            .split_whitespace()
            .find(|word| word.starts_with('P'));
        assert_eq!(
            p,
            if stem == "haas" { Some("P0") } else { None },
            "{stem}: {}",
            out.program
        );
    }
}

#[test]
fn haas_g83_writes_integer_milliseconds() {
    let out = run("haas", &[0.5]);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines = cycle_lines(&out.program);
    assert_eq!(lines.len(), 1, "{}", out.program);
    assert!(
        lines[0].split_whitespace().any(|word| word == "P500"),
        "{}",
        out.program
    );
}

#[test]
fn haas_g83_updates_dwell_in_an_open_cycle_group() {
    let out = run("haas", &[0.5, 0.75]);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines = cycle_lines(&out.program);
    assert_eq!(
        lines.len(),
        2,
        "changed dwell must restart G83:\n{}",
        out.program
    );
    for (line, p) in lines.iter().zip(["P500", "P750"]) {
        assert!(
            line.split_whitespace().any(|word| word == p),
            "{line}: expected {p}"
        );
    }
}

#[test]
fn haas_g83_zero_dwell_clears_modal_p() {
    let out = run("haas", &[0.5, 0.0]);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines = cycle_lines(&out.program);
    assert_eq!(
        lines.len(),
        2,
        "zero dwell must restart G83:\n{}",
        out.program
    );
    assert!(
        lines[1].split_whitespace().any(|word| word == "P0"),
        "zero dwell must reset modal P:\n{}",
        out.program
    );
}
