//! #707: the Makera Carvera post on its own machine file - a two-tool
//! pocket posts with the firmware's own words (M6 Tn, M3 S, the M7 air
//! blast, G2/G3 with relative centres, M2 to end) and none it does not
//! list (no G43 / G49 / G40 / G80 / G91.1, no M8 flood, no M30), locked
//! by a golden; and the refusals by name: an inch machine (the firmware's
//! G20 halts it), tool 0 (the wireless probe's number) and cutter
//! compensation in the control.
//! Provenance: our own tests over our own post; the words are the Carvera
//! firmware's supported-codes tables (see assets/posts/carvera.cps).
//! Gate map (M4.4): covers post::post (carvera.cps) with the shipped
//! Makera Carvera machine (posting::MACHINES).

mod common;

use camcraft_app::cl::{CompSide, MoveKind};
use camcraft_app::machine::parse_mch;
use camcraft_app::post::{cnc, engine, machine};
use camcraft_app::posting::MACHINES;
use serde_json::{json, Value};
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn carvera() -> Value {
    let text = MACHINES
        .iter()
        .find(|(n, _)| *n == "Makera Carvera")
        .map(|(_, t)| *t)
        .expect("the Carvera ships");
    machine::mirror(&parse_mch(text).expect("the Carvera parses"))
}

fn mv(kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec { kind, x, y, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

/// Two operations: a 3 mm end mill (T1) roughs a 20 mm square pocket,
/// and a 1 mm end mill (T3) finishes its rounded corner with an arc.
fn two_tool_pocket(tool_a: u32, comp: bool) -> String {
    use cnc::Record as R;
    let mut recs = vec![
        R::Param { name: "OperationName".into(), value: json!("Pocket1") },
        R::Param { name: "SpindleSpeed".into(), value: json!(12000) },
        R::Tool { number: tool_a, name: "3MM FLAT".into() },
        R::Section { begin: true },
        mv(MoveKind::Rapid, 5.0, 5.0, 5.0, None),
        mv(MoveKind::Plunge, 5.0, 5.0, -1.0, Some(200.0)),
    ];
    if comp {
        recs.push(R::Comp { side: Some(CompSide::Left), radius_mm: 1.5, wear: false });
    }
    recs.extend([
        mv(MoveKind::Feed, 25.0, 5.0, -1.0, Some(600.0)),
        mv(MoveKind::Feed, 25.0, 25.0, -1.0, Some(600.0)),
        mv(MoveKind::Feed, 5.0, 25.0, -1.0, Some(600.0)),
        mv(MoveKind::Feed, 5.0, 5.0, -1.0, Some(600.0)),
        mv(MoveKind::Rapid, 5.0, 5.0, 5.0, None),
        R::Section { begin: false },
        R::Param { name: "OperationName".into(), value: json!("Corner1") },
        R::Param { name: "SpindleSpeed".into(), value: json!(15000) },
        R::Tool { number: 3, name: "1MM FLAT".into() },
        R::Section { begin: true },
        mv(MoveKind::Rapid, 20.0, 25.0, 5.0, None),
        mv(MoveKind::Plunge, 20.0, 25.0, -1.0, Some(100.0)),
        R::Move(cnc::MoveRec {
            kind: MoveKind::Feed,
            x: 25.0,
            y: 20.0,
            z: -1.0,
            feed: Some(300.0),
            center: Some((20.0, 20.0)),
            ccw: false,
            plane: Default::default(),
            abc: None,
        }),
        mv(MoveKind::Rapid, 25.0, 20.0, 5.0, None),
        R::Section { begin: false },
    ]);
    cnc::serialize(&recs)
}

fn post(mch: &Value, stream: &str) -> engine::PostOutput {
    engine::run(&read("assets/posts/carvera.cps"), mch, stream).expect("the engine runs")
}

#[test]
fn a_two_tool_pocket_posts_in_the_carveras_own_words() {
    let out = post(&carvera(), &two_tool_pocket(1, false));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    for want in ["G21 G17 G90 G94", "M6 T1", "M3 S12000", "M6 T3", "M3 S15000", "M7", "M9", "M5", "G2 X25 Y20 Z-1 I0 J-5 F300"] {
        assert!(p.lines().any(|l| l == want), "missing `{want}`:\n{p}");
    }
    assert_eq!(p.lines().last(), Some("M2"), "the program ends M2 (M30 is no action on a Carvera):\n{p}");
    // Words the firmware's tables do not list are never written.
    for word in ["G43", "G49", "G40", "G80", "G91.1", "G20", "M8", "M30", "G53"] {
        assert!(!p.split_whitespace().any(|w| w == word), "`{word}` is not a Carvera word:\n{p}");
    }
    common::write_golden("nc/carvera/two_tool_pocket.nc", p);
}

#[test]
fn an_inch_machine_is_refused_by_name() {
    let mut mch = carvera();
    mch["units"] = json!("inch");
    let out = post(&mch, &two_tool_pocket(1, false));
    assert!(out.warnings.iter().any(|w| w.contains("millimetres only")), "{:?}", out.warnings);
}

#[test]
fn tool_zero_is_the_probe_and_refused_as_a_cutter() {
    let out = post(&carvera(), &two_tool_pocket(0, false));
    assert!(out.warnings.iter().any(|w| w.contains("wireless probe")), "{:?}", out.warnings);
}

#[test]
fn control_cutter_compensation_is_refused_by_name() {
    let out = post(&carvera(), &two_tool_pocket(1, true));
    assert!(out.warnings.iter().any(|w| w.contains("G41/G42")), "{:?}", out.warnings);
}
