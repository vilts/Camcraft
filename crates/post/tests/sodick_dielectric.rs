//! Provenance: synthetic streams from our CNC intermediate contract and
//! synthetic machine mirrors (#1499). The words asserted are the 1998
//! Sodick "Wire EDM Machine Operation" manual's T-code table (T84 / T85
//! high / low pressure flushing, T94 / T89 submersed / non-submersed
//! machining) in its Program Example 1 order (T84, T94, then the C
//! condition word).
//! Gate map: covers assets/posts/sodick-wire.cps, assets/posts/wire-iso.cps,
//! camcraft_post::post::engine, camcraft_post::post::cnc.

use camcraft_post::cl::{ArcPlane, MoveKind};
use camcraft_post::post::cnc::{self, MoveRec, Record};
use camcraft_post::post::engine;
use serde_json::{json, Value};

const SODICK: &str = include_str!("../../../assets/posts/sodick-wire.cps");
const WIRE_ISO: &str = include_str!("../../../assets/posts/wire-iso.cps");

fn mv(kind: MoveKind, x: f64) -> Record {
    Record::Move(MoveRec {
        kind,
        x,
        y: 0.0,
        z: 0.0,
        feed: (kind == MoveKind::Feed).then_some(2.0),
        center: None,
        ccw: false,
        plane: ArcPlane::Xy,
        abc: None,
    })
}

fn param(name: &str, value: Value) -> Record {
    Record::Param {
        name: name.into(),
        value,
    }
}

fn section(op: &str) -> Vec<Record> {
    vec![
        param("OperationName", json!(op)),
        param("WorkOffset", json!("G54")),
        param("EdmCondition", json!("C110")),
        Record::Section { begin: true },
        mv(MoveKind::Rapid, -30.0),
        mv(MoveKind::Feed, -20.0),
        Record::Section { begin: false },
    ]
}

/// A wire machine mirror with `edm` merged over AWT.
fn machine(edm: Value) -> Value {
    let mut e = json!({ "awt": true });
    if let (Some(o), Some(add)) = (e.as_object_mut(), edm.as_object()) {
        for (k, v) in add {
            o.insert(k.clone(), v.clone());
        }
    }
    json!({ "kind": "wire_edm", "name": "Synthetic wire", "edm": e })
}

fn run(script: &str, machine: &Value, records: &[Record]) -> engine::PostOutput {
    engine::run(script, machine, &cnc::serialize(records)).expect("synthetic stream runs")
}

fn body(out: &engine::PostOutput) -> Vec<&str> {
    out.program.lines().filter(|l| !l.starts_with('(')).collect()
}

fn notes(out: &engine::PostOutput) -> Vec<&str> {
    out.program.lines().filter(|l| l.starts_with('(')).collect()
}

const DIELECTRIC_WORDS: [&str; 4] = ["T84", "T85", "T89", "T94"];

/// Done means: flushing and submerged cutting stated post the T words in
/// the manual's order, with the wire threaded, before the condition.
#[test]
fn stated_flushing_and_tank_post_their_t_words_after_threading_in_the_manuals_order() {
    let out = run(SODICK, &machine(json!({ "tank": "submerged", "flushing": "high" })), &section("Punch1"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(
        body(&out),
        [
            "G21",
            "G90",
            "G54",
            "G40 G00 X-30. Y0.", // to the threading point, wire cut
            "T91",               // thread the wire (AWT)
            "T84",               // high pressure flushing on (manual, T-code table)
            "T94",               // submersed machining (manual, T-code table)
            "C110",              // the condition, after both (Program Example 1 order)
            "G01 X-20. Y0.",
            "T90",
            "M02"
        ],
        "{}",
        out.program
    );
    let n = notes(&out);
    assert!(n.contains(&"(T94 SUBMERGED MACHINING AFTER THREADING - NO FILL OR DRAIN WORD IS WRITTEN - CHECK THE TANK IS FULL)"), "{n:?}");
    assert!(n.contains(&"(T84 HIGH-PRESSURE FLUSHING AFTER THREADING)"), "{n:?}");
    assert!(!n.iter().any(|l| l.contains("TANK FILL AND FLUSHING ARE NOT PROGRAMMED")), "{n:?}");
}

#[test]
fn each_stated_choice_writes_its_own_word_and_the_other_is_left_to_the_operator() {
    let cases = [
        (json!({ "tank": "non-submerged" }), "T89", "(FLUSHING IS NOT PROGRAMMED - SET IT AT THE CONTROL)"),
        (json!({ "tank": "submerged" }), "T94", "(FLUSHING IS NOT PROGRAMMED - SET IT AT THE CONTROL)"),
        (json!({ "flushing": "low" }), "T85", "(TANK IS NOT PROGRAMMED - SET IT AT THE CONTROL)"),
        (json!({ "flushing": "high" }), "T84", "(TANK IS NOT PROGRAMMED - SET IT AT THE CONTROL)"),
    ];
    for (edm, word, other) in cases {
        let out = run(SODICK, &machine(edm.clone()), &section("Punch1"));
        assert!(out.warnings.is_empty(), "{edm}: {:?}", out.warnings);
        let b = body(&out);
        let written: Vec<&str> = b.iter().copied().filter(|l| DIELECTRIC_WORDS.contains(l)).collect();
        assert_eq!(written, [word], "{edm}: {}", out.program);
        let (th, w, c) = (
            b.iter().position(|l| *l == "T91"),
            b.iter().position(|l| *l == word),
            b.iter().position(|l| *l == "C110"),
        );
        assert!(th < w && w < c, "{edm}: {}", out.program);
        assert!(notes(&out).contains(&other), "{edm}: {}", out.program);
    }
}

/// Done means: a machine that states nothing posts as today.
#[test]
fn unstated_tank_and_flushing_post_as_before() {
    let out = run(SODICK, &machine(json!({})), &section("Punch1"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(
        body(&out),
        ["G21", "G90", "G54", "G40 G00 X-30. Y0.", "T91", "C110", "G01 X-20. Y0.", "T90", "M02"],
        "{}",
        out.program
    );
    assert!(notes(&out).contains(&"(TANK FILL AND FLUSHING ARE NOT PROGRAMMED - SET THEM AT THE CONTROL)"));
}

/// Without AWT the operator threads at an M00; the words follow it.
#[test]
fn without_awt_the_words_follow_the_operators_threading_stop() {
    let mut m = machine(json!({ "tank": "submerged", "flushing": "low" }));
    m["edm"]["awt"] = json!(false);
    let out = run(SODICK, &m, &section("Punch1"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let b = body(&out);
    let at = |w: &str| b.iter().position(|l| *l == w).unwrap_or(usize::MAX);
    assert!(at("G40 G00 X-30. Y0.") < at("M00") && at("M00") < at("T85") && at("T85") < at("T94") && at("T94") < at("C110"), "{}", out.program);
}

/// Done means: an unknown choice is refused by name, nothing is cut.
#[test]
fn an_unknown_tank_or_flushing_word_is_refused_by_name_and_nothing_is_cut() {
    let cases = [
        (json!({ "tank": "flooded" }), "edm.tank \"flooded\" is not submerged or non-submerged"),
        (json!({ "flushing": "medium" }), "edm.flushing \"medium\" is not high or low"),
        (json!({ "tank": true }), "edm.tank true is not submerged or non-submerged"),
    ];
    for (edm, why) in cases {
        let out = run(SODICK, &machine(edm.clone()), &section("Punch1"));
        assert!(out.warnings.iter().any(|w| w.contains(why) && w.contains("Synthetic wire")), "{edm}: {:?}", out.warnings);
        for word in ["T91", "C110", "G01", "G00"].iter().chain(DIELECTRIC_WORDS.iter()) {
            assert!(!body(&out).iter().any(|l| l.split_whitespace().any(|w| w == *word)), "{edm}: {word}\n{}", out.program);
        }
    }
}

/// Every threading of a two-section program carries the words.
#[test]
fn every_section_states_the_words_after_its_own_threading() {
    let mut stream = section("Punch1");
    stream.extend(section("Punch2"));
    let out = run(SODICK, &machine(json!({ "tank": "submerged", "flushing": "high" })), &stream);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let b = body(&out);
    let threads: Vec<usize> = b.iter().enumerate().filter(|(_, l)| **l == "T91").map(|(i, _)| i).collect();
    assert_eq!(threads.len(), 2, "{}", out.program);
    for t in threads {
        assert_eq!(&b[t..t + 4], ["T91", "T84", "T94", "C110"], "{}", out.program);
    }
}

/// The generic ISO wire post has no word for either: it says the stated
/// ask is the operator's, never drops it silently.
#[test]
fn the_generic_wire_post_notes_a_stated_tank_and_flushing_it_does_not_program() {
    let codes = json!({ "condition": "FX-COND {id}", "comp_off": "FX-COMP-OFF", "thread": "FX-THREAD", "cut": "FX-CUT", "tip_disposal": "FX-TIP-OUT" });
    let stated = machine(json!({ "tank": "submerged", "flushing": "high", "codes": codes }));
    let out = run(WIRE_ISO, &stated, &section("Punch1"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let n = notes(&out);
    assert!(n.contains(&"(TANK SUBMERGED IS STATED BUT NOT PROGRAMMED - SET IT AT THE CONTROL)"), "{n:?}");
    assert!(n.contains(&"(FLUSHING HIGH IS STATED BUT NOT PROGRAMMED - SET IT AT THE CONTROL)"), "{n:?}");
    let plain = run(WIRE_ISO, &machine(json!({ "codes": codes })), &section("Punch1"));
    assert!(!plain.program.contains("STATED BUT NOT PROGRAMMED"), "{}", plain.program);
    let strip = |p: &str| p.lines().filter(|l| !l.contains("STATED BUT NOT PROGRAMMED")).collect::<Vec<_>>().join("\n");
    assert_eq!(strip(&out.program), strip(&plain.program), "only the notes differ");
}
