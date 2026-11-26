//! #3690: explicit decimal points prevent control-dependent feed scaling.
//! Provenance: FANUC B-63784EN/01, section 5.3, units of F with/without a point.
//! Gate map: covers assets/posts/fanuc.cps, assets/posts/doosan-dnm.cps,
//! assets/posts/mazak.cps, camcraft_post::post::engine.

use camcraft_post::cl::MoveKind;
use camcraft_post::post::{cnc, engine};
use serde_json::{json, Value};
use std::path::Path;

const MILL: &str = "PARAM SpindleSpeed 6000
PARAM ClearancePlane 50.8
PARAM RetractPlane 25.4
TOOL 1 END MILL
SECTION
Rapid 25400 25400 50800
Rapid 25400 25400 25400
Feed 25400 25400 -25400 F50800
Feed 50800 25400 -25400 F254000
Rapid 50800 25400 50800
ENDSECTION
";

fn check(stem: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/posts")
        .join(format!("{stem}.cps"));
    let script = std::fs::read_to_string(path).unwrap();
    let out = engine::run_with(&script, &Value::Null, MILL, &json!({"Units": "inch"})).unwrap();
    assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
    assert!(out.program.contains("G20"), "{stem}: {}", out.program);
    let words: Vec<_> = out
        .program
        .lines()
        .filter(|l| !l.starts_with('('))
        .flat_map(|l| l.split('(').next().unwrap_or("").split_whitespace())
        .collect();
    for feed in ["F2.", "F10."] {
        assert!(
            words.contains(&feed),
            "{stem}: expected unscaled {feed}\n{}",
            out.program
        );
    }
}

#[test]
fn fanuc_whole_inch_feeds_keep_their_decimal_point() {
    check("fanuc");
}

#[test]
fn doosan_whole_inch_feeds_keep_their_decimal_point() {
    check("doosan-dnm");
}

#[test]
fn mazak_whole_inch_feeds_keep_their_decimal_point() {
    check("mazak");
}

fn capped(stem: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/posts")
        .join(format!("{stem}.cps"));
    let script = std::fs::read_to_string(path).unwrap();
    let machine = json!({"name": "RATED HEAD", "feedRateMax": 5000,
        "controller": {"rtcp": true},
        "kinematics": {"static": {"rotaries": [{"letter": "A", "type": "rotary",
            "direction": [1, 0, 0], "travel": [-120, 120], "speeds": {"normal": 90}}]}}});
    let movement = |kind, x, z, feed, a| {
        cnc::Record::Move(cnc::MoveRec {
            kind,
            x,
            y: 0.0,
            z,
            feed,
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: Some([a, 0.0, 0.0]),
        })
    };
    let records = vec![
        cnc::Record::Param {
            name: "SpindleSpeed".into(),
            value: json!(6000),
        },
        cnc::Record::Param {
            name: "ClearancePlane".into(),
            value: json!(50.8),
        },
        cnc::Record::Param {
            name: "RetractPlane".into(),
            value: json!(25.4),
        },
        cnc::Record::Tool {
            number: 1,
            name: "END MILL".into(),
        },
        cnc::Record::Section { begin: true },
        movement(MoveKind::Rapid, 0.0, 50.8, None, 0.0),
        movement(MoveKind::Rapid, 0.0, 5.0, None, 0.0),
        movement(MoveKind::Feed, 25.4, 5.0, Some(600.0), 90.0),
        cnc::Record::Section { begin: false },
    ];
    let out = engine::run_with(
        &script,
        &machine,
        &cnc::serialize(&records),
        &json!({"Units": "inch", "rotaryBrake": true}),
    )
    .unwrap();
    assert!(
        out.warnings.is_empty(),
        "{stem}: {:?}\n{}",
        out.warnings,
        out.program
    );
    let line = out
        .program
        .lines()
        .find(|l| l.starts_with("G01") && l.contains("A90."))
        .unwrap();
    // 25.4 mm of tip travel / 90 degrees at 90 deg/min = 1 inch/min.
    assert!(
        line.split_whitespace().any(|w| w == "F1."),
        "{stem}: capped whole feed must keep its point: {line}"
    );
}

#[test]
fn fanuc_capped_inch_feed_keeps_its_decimal_point() {
    capped("fanuc");
}

#[test]
fn doosan_capped_inch_feed_keeps_its_decimal_point() {
    capped("doosan-dnm");
}
