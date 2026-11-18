//! Sinumerik G83 dwell as P in ms, by the maintainer's 840D ISO-mode (G291) check (#2349).
//! Gate map: covers assets/posts/sinumerik.cps, camcraft_post::post::engine
//! and camcraft_post::post::cnc.

use camcraft_post::cl::{CycleKind, MoveKind};
use camcraft_post::post::{
    cnc::{self, CycleRec, MoveRec, Record},
    engine,
};
use serde_json::json;
use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel))
        .expect("fixture")
}

fn stream(kind: CycleKind, dwell: f64) -> String {
    let records = vec![
        Record::Param { name: "SpindleSpeed".into(), value: json!(900.0) },
        Record::Param { name: "ClearancePlane".into(), value: json!(10.0) },
        Record::Param { name: "RetractPlane".into(), value: json!(10.0) },
        Record::Tool { number: 4, name: "DRILL".into() },
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
        Record::Cycle(CycleRec {
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
        }),
        Record::Section { begin: false },
    ];
    cnc::serialize(&records)
}

fn run(kind: CycleKind, dwell: f64) -> engine::PostOutput {
    let machine = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine");
    engine::run(&read("assets/posts/sinumerik.cps"), &machine, &stream(kind, dwell))
        .expect("post engine")
}

fn lines_with<'a>(program: &'a str, word: &str) -> Vec<&'a str> {
    program
        .lines()
        .filter(|line| line.split_whitespace().any(|w| w == word))
        .collect()
}

fn pins_dwell(kind: CycleKind, g: &str) {
    let out = run(kind, 0.5);
    assert!(out.warnings.is_empty(), "{g}: {:?}", out.warnings);
    let lines = lines_with(&out.program, g);
    assert_eq!(lines.len(), 1, "{}", out.program);
    let words: Vec<&str> = lines[0].split_whitespace().collect();
    assert!(words.contains(&"Q3"), "{g} keeps its peck: {}", lines[0]);
    // P in whole ms, before F (#2349; maintainer, 840D in G291).
    let p = words.iter().position(|w| *w == "P500");
    let f = words.iter().position(|w| w.starts_with('F'));
    assert!(p.is_some() && p < f, "{g} must carry P500 before F: {}", lines[0]);
}

#[test]
fn sinumerik_g83_dwell_posts_p_in_ms() {
    pins_dwell(CycleKind::Deep, "G83");
}

#[test]
fn sinumerik_g83_without_dwell_is_unchanged() {
    let out = run(CycleKind::Deep, 0.0);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines = lines_with(&out.program, "G83");
    assert_eq!(lines.len(), 1, "{}", out.program);
    assert!(
        !lines[0].split_whitespace().any(|w| w.starts_with('P')),
        "G83 must carry no P word: {}",
        lines[0]
    );
    assert!(lines[0].split_whitespace().any(|w| w == "Q3"), "{}", lines[0]);
}

#[test]
fn sinumerik_g82_dwell_still_posts() {
    // A spot-drill dwell keeps its G82 P word in ms.
    let out = run(CycleKind::Drill, 0.5);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines = lines_with(&out.program, "G82");
    assert_eq!(lines.len(), 1, "{}", out.program);
    assert!(lines[0].split_whitespace().any(|w| w == "P500"), "{}", lines[0]);
}
