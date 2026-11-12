//! #934: the Siemens 840D post writes the settable frame each work
//! offset selects. The engine hands a setup's offset as its generic word
//! (`WCS_SLOTS`, `G54`..`G59` - the setup is the one source of truth);
//! on the 840D the settable zero offsets are `G54`..`G57` and then
//! `G505`..`G599`, while `G58` / `G59` are axial programmable offsets
//! that select no frame. So offsets 1-4 post `G54`..`G57` and offsets 5
//! and 6 post `G505` / `G506`; a word naming no settable frame is
//! refused by name.
//! Provenance: SINUMERIK 840D sl Fundamentals (settable zero offsets,
//! public programming documentation); our own fixtures.
//!
//! Gate map (#934): covers assets/posts (sinumerik.cps frameWord).

use camcraft_app::cl::MoveKind;
use camcraft_app::post::cnc::{self, MoveRec, Record};
use camcraft_app::post::engine;
use camcraft_app::schema::WCS_SLOTS;
use serde_json::{json, Value};

const SINUMERIK: &str = include_str!("../../../assets/posts/sinumerik.cps");

fn param(name: &str, value: Value) -> Record {
    Record::Param { name: name.into(), value }
}

fn rapid(z: f64) -> Record {
    Record::Move(MoveRec { kind: MoveKind::Rapid, x: 0.0, y: 0.0, z, feed: None, center: None, ccw: false, plane: Default::default(), abc: None })
}

/// One section per work-offset word, each opening with its word.
fn stream(words: &[&str]) -> String {
    let mut rs = vec![param("SpindleSpeed", json!(2000.0)), Record::Tool { number: 1, name: "T1".into() }];
    for w in words {
        rs.push(param("WorkOffset", json!(w)));
        rs.push(Record::Section { begin: true });
        rs.push(rapid(5.0));
        rs.push(Record::Section { begin: false });
    }
    cnc::serialize(&rs)
}

/// The work-offset word of each section's opening block, in order.
fn opening_words(program: &str) -> Vec<String> {
    program
        .lines()
        .filter(|l| l.contains(" G17 G710 G90 "))
        .filter_map(|l| l.split_whitespace().skip_while(|w| *w != "G90").nth(1).map(str::to_string))
        .collect()
}

#[test]
fn sinumerik_posts_offsets_one_to_six_as_their_settable_frames() {
    let out = engine::run(SINUMERIK, &Value::Null, &stream(&WCS_SLOTS)).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(opening_words(&out.program), ["G54", "G55", "G56", "G57", "G505", "G506"], "{}", out.program);
    assert!(
        !out.program.lines().any(|l| l.split_whitespace().any(|w| w == "G58" || w == "G59")),
        "G58 / G59 select no frame on an 840D:\n{}",
        out.program
    );
}

#[test]
fn sinumerik_refuses_a_work_offset_that_names_no_settable_frame() {
    let out = engine::run(SINUMERIK, &Value::Null, &stream(&["G53"])).expect("engine runs");
    assert!(out.warnings.iter().any(|w| w.contains("G53") && w.contains("settable frames")), "{:?}", out.warnings);
}
