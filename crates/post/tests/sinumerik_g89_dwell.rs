//! G89 dwell units and modal reset from Siemens ISO dialects Function Manual,
//! 07/2021, 6FC5397-7BP40-3BA2, table 6-1 (page 95) and section 6.3.1 (page 97).
//! Source: https://cache.industry.siemens.com/dl/files/254/109801254/att_1077768/v1/ONE_840Dsl_828D_iso_dialects_fct_man_0721_en-US.pdf
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

fn post(dwells: &[f64]) -> engine::PostOutput {
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
            name: "BACK BORE".into(),
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
    // Keep both holes at the same seat, without a rapid or cycle cancellation.
    for &dwell in dwells {
        records.push(Record::Cycle(CycleRec {
            kind: CycleKind::BackBore,
            x: 10.0,
            y: 10.0,
            r_z: 2.0,
            bottom_z: -12.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell,
            first_move: 1,
            feed: Some(120.0),
        }));
    }
    records.push(Record::Section { begin: false });
    let machine = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine");
    engine::run(
        &read("assets/posts/sinumerik.cps"),
        &machine,
        &cnc::serialize(&records),
    )
    .expect("post engine")
}

fn cycle_words(output: &engine::PostOutput) -> Vec<Vec<&str>> {
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    output
        .program
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .filter(|words| words.contains(&"G89"))
        .collect()
}

#[test]
fn sinumerik_g89_dwell_posts_p_in_milliseconds() {
    let output = post(&[0.5]);
    let cycles = cycle_words(&output);
    assert_eq!(cycles.len(), 1, "{}", output.program);
    assert!(cycles[0].contains(&"P500"), "{}", output.program);
    assert!(cycles[0].contains(&"F120"), "{}", output.program);
}

#[test]
fn sinumerik_g89_zero_dwell_clears_previous_p() {
    let output = post(&[0.5, 0.0]);
    let cycles = cycle_words(&output);
    assert_eq!(cycles.len(), 2, "{}", output.program);
    assert!(cycles[0].contains(&"P500"), "{}", output.program);
    assert!(cycles[1].contains(&"P0"), "{}", output.program);
    assert!(cycles[1].contains(&"F120"), "{}", output.program);
}
