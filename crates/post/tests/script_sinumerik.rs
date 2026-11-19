//! The `sinumerik.cps` script suite (#926): the post binary on every recorded
//! edge-case stream (`recorded/streams/*.cnc`), its program or its named
//! refusal compared with the golden in `recorded/golden/sinumerik/`. No app, no
//! toolpath generation: the inputs are recorded, and their generators keep
//! them current (`recorded_streams.rs`, app's `recorded_post_streams.rs`).
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test script_sinumerik`
//!
//! Gate map (M4.4): covers assets/posts/sinumerik.cps, camcraft_post::post::engine,
//! camcraft_post::post::cnc, the post binary.

#[macro_use]
mod recorded;

post_suite!("sinumerik.cps");

fn peck_output(dwell: f64) -> camcraft_post::post::engine::PostOutput {
    use camcraft_post::cl::{CycleKind, MoveKind};
    use camcraft_post::post::{cnc::{self, CycleRec, MoveRec, Record}, engine};
    use serde_json::json;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let machine = serde_json::from_str(
        &std::fs::read_to_string(root.join("assets/machines/3axis-vmc.mch")).expect("machine fixture"),
    ).expect("machine JSON");
    let records = vec![
        Record::Param { name: "SpindleSpeed".into(), value: json!(900.0) },
        Record::Param { name: "ClearancePlane".into(), value: json!(10.0) },
        Record::Param { name: "RetractPlane".into(), value: json!(10.0) },
        Record::Tool { number: 4, name: "DRILL".into() },
        Record::Section { begin: true },
        Record::Move(MoveRec {
            kind: MoveKind::Rapid, x: 10.0, y: 10.0, z: 2.0,
            feed: None, center: None, ccw: false, plane: Default::default(), abc: None,
        }),
        Record::Cycle(CycleRec {
            kind: CycleKind::Peck, x: 10.0, y: 10.0, r_z: 2.0, bottom_z: -12.0,
            entry_z: 0.0, peck: 3.0, dwell, first_move: 1, feed: Some(120.0),
        }),
        Record::Section { begin: false },
    ];
    let script = std::fs::read_to_string(root.join("assets/posts/sinumerik.cps")).expect("post fixture");
    engine::run(&script, &machine, &cnc::serialize(&records)).expect("post engine")
}

#[test]
fn sinumerik_g73_dwell_is_refused_by_name() {
    for dwell in [0.001, 0.5] {
        let out = peck_output(dwell);
        assert!(
            out.warnings.iter().any(|w| w.contains("G73") && w.contains("dwell") && w.contains("refused")),
            "G73 dwell {dwell} must be refused by name: {:?}", out.warnings,
        );
        assert!(
            !out.program.split_whitespace().any(|w| w == "G73"),
            "a refused dwell must not write G73: {}", out.program,
        );
    }
}

#[test]
fn sinumerik_g73_without_dwell_still_posts() {
    let out = peck_output(0.0);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let cycle = out.program.lines()
        .find(|line| line.split_whitespace().any(|w| w == "G73"))
        .expect("zero-dwell peck cycle");
    let words: Vec<_> = cycle.split_whitespace().collect();
    for word in ["X10", "Y10", "Z-12", "R2", "Q3", "F120"] {
        assert!(words.contains(&word), "G73 keeps {word}: {cycle}");
    }
    assert!(!words.iter().any(|w| w.starts_with('P')), "no dwell means no P: {cycle}");
}
