//! Provenance: issue #1819's named-refusal contract for expanded G86 bore cycles.
//! Gate map: covers assets/posts/heidenhain.cps, assets/posts/tinyg.cps,
//! assets/posts/carvera.cps, post::post::engine, post::post::cnc.

use camcraft_post::cl::{CycleKind, MoveKind};
use camcraft_post::post::{cnc::{self, CycleRec, MoveRec, Record}, engine};
use serde_json::json;
use std::path::Path;

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path))
        .expect("fixture exists")
}

fn output(post: &str, kind: CycleKind, dwell: f64) -> engine::PostOutput {
    let mut records = vec![
        Record::Param { name: "OperationName".into(), value: json!("Bore regression") },
        Record::Param { name: "SpindleSpeed".into(), value: json!(3000.0) },
        Record::Param { name: "ClearancePlane".into(), value: json!(10.0) },
        Record::Tool { number: 1, name: "Boring tool".into() },
        Record::Section { begin: true },
        Record::Cycle(CycleRec {
            kind, x: 10.0, y: 20.0, r_z: 2.0, bottom_z: -6.0,
            entry_z: 0.0, peck: 0.0, dwell, first_move: 0, feed: Some(100.0),
        }),
    ];
    for (motion, z, feed) in [
        (MoveKind::Rapid, 2.0, None),
        (MoveKind::Feed, -6.0, Some(100.0)),
        (MoveKind::Rapid, 2.0, None),
        (MoveKind::Rapid, 10.0, None),
    ] {
        records.push(Record::Move(MoveRec {
            kind: motion, x: 10.0, y: 20.0, z, feed, center: None,
            ccw: false, plane: Default::default(), abc: None,
        }));
    }
    records.push(Record::Section { begin: false });
    let machine = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).unwrap();
    engine::run(&read(&format!("assets/posts/{post}.cps")), &machine, &cnc::serialize(&records))
        .expect("post executes")
}

fn assert_bore_refused(post: &str) {
    for dwell in [0.0, 0.5] {
        let out = output(post, CycleKind::Bore, dwell);
        assert!(out.warnings.iter().any(|warning| {
            let warning = warning.to_ascii_lowercase();
            warning.contains("bore") && warning.contains("refus")
        }), "{post} must refuse BORE with dwell {dwell}: {:?}", out.warnings);
    }
}

#[test]
fn heidenhain_refuses_expanded_bore() { assert_bore_refused("heidenhain"); }

#[test]
fn tinyg_refuses_expanded_bore() { assert_bore_refused("tinyg"); }

#[test]
fn grbl_refuses_expanded_bore() { assert_bore_refused("grbl"); }

#[test]
fn carvera_refuses_expanded_bore() { assert_bore_refused("carvera"); }

#[test]
fn ordinary_drilling_remains_available() {
    for post in ["heidenhain", "tinyg", "grbl", "carvera"] {
        for dwell in [0.0, 0.5] {
            let out = output(post, CycleKind::Drill, dwell);
            assert!(out.warnings.is_empty(), "{post} drill dwell {dwell}: {:?}", out.warnings);
            assert!(!out.program.is_empty(), "{post} must still emit drilling");
        }
    }
}
