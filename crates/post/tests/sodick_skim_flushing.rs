//! Provenance: synthetic streams from our CNC intermediate contract and
//! synthetic machine mirrors (#2686). The words asserted are the Sodick
//! T-code table's (T84 / T85 high / low pressure flushing, 1998 "Wire EDM
//! Machine Operation" manual); that a finish pass changes the flushing
//! before its C word is the program example of the 2012 "Wire EDM Machine
//! Operation Training, LN1W / LN2W Control" manual (part number 6300015).
//! No other CAM product's post or output was read. Not run on a control.
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

/// A rough cut on C001 out and back to the threading point, then one skim
/// on C002 out and back (the #538 stream: the skim's condition arrives
/// with the wire threaded).
fn skimmed(op: &str) -> Vec<Record> {
    vec![
        param("OperationName", json!(op)),
        param("WorkOffset", json!("G54")),
        param("EdmCondition", json!("C001")),
        Record::Section { begin: true },
        mv(MoveKind::Rapid, -30.0),
        mv(MoveKind::Feed, -20.0),
        mv(MoveKind::Feed, -30.0),
        param("EdmCondition", json!("C002")),
        mv(MoveKind::Feed, -20.5),
        mv(MoveKind::Feed, -30.0),
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

/// Done means: a skim states its flushing, and the post writes that T
/// word before the skim's C word.
#[test]
fn a_stated_skim_flushing_posts_its_t_word_before_the_skims_c_word() {
    let out = run(SODICK, &machine(json!({ "flushing": "high", "skim_flushing": "low" })), &skimmed("Punch1"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(
        body(&out),
        [
            "G21",
            "G90",
            "G54",
            "G40 G00 X-30. Y0.",
            "T91",
            "T84", // the rough cut's flushing (#1499)
            "C001",
            "G01 X-20. Y0.",
            "G01 X-30. Y0.", // back at the threading point
            "T85",           // the skim's flushing, before its C word
            "C002",
            "G01 X-20.5 Y0.",
            "G01 X-30. Y0.",
            "T90",
            "M02"
        ],
        "{}",
        out.program
    );
    let n = notes(&out);
    assert!(n.contains(&"(T84 HIGH-PRESSURE FLUSHING AFTER THREADING)"), "{n:?}");
    assert!(n.contains(&"(T85 LOW-PRESSURE FLUSHING BEFORE EACH SKIM PASS)"), "{n:?}");

    // The other way round, and the same word for both: the word is always
    // written, never left to what the rough cut selected.
    for (rough, skim, words) in [("low", "high", ["T85", "T84"]), ("high", "high", ["T84", "T84"])] {
        let out = run(SODICK, &machine(json!({ "flushing": rough, "skim_flushing": skim })), &skimmed("Punch1"));
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        let b = body(&out);
        let t: Vec<&str> = b.iter().copied().filter(|l| *l == "T84" || *l == "T85").collect();
        assert_eq!(t, words, "{}", out.program);
        let c2 = b.iter().position(|l| *l == "C002").expect("the skim's C word");
        assert_eq!(b[c2 - 1], words[1], "{}", out.program);
    }
}

/// The skim's flushing is modal: the next operation's rough cut selects
/// its own flushing again after its threading.
#[test]
fn the_next_sections_rough_cut_selects_the_rough_flushing_again() {
    let mut stream = skimmed("Punch1");
    stream.extend(skimmed("Punch2"));
    let out = run(SODICK, &machine(json!({ "flushing": "high", "skim_flushing": "low" })), &stream);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let b = body(&out);
    let t: Vec<&str> = b.iter().copied().filter(|l| *l == "T84" || *l == "T85").collect();
    assert_eq!(t, ["T84", "T85", "T84", "T85"], "{}", out.program);
}

/// Done means: the value is not known - the rough cut's flushing is
/// programmed, the skim's is not stated - and the skim is refused by name.
#[test]
fn a_skim_on_a_machine_that_states_only_the_rough_flushing_is_refused_by_name() {
    let out = run(SODICK, &machine(json!({ "flushing": "high" })), &skimmed("Punch1"));
    let why = out.warnings.iter().find(|w| w.contains("edm.skim_flushing")).unwrap_or_else(|| panic!("{:?}", out.warnings));
    assert!(why.contains("Punch1") && why.contains("Synthetic wire") && why.contains("edm.flushing"), "{why}");
    let b = body(&out);
    assert!(!b.contains(&"C002") && !b.contains(&"G01 X-20.5 Y0."), "no word of the refused skim:\n{}", out.program);

    // Without a skim the same machine posts as before (#1499).
    let rough: Vec<Record> = skimmed("Punch1").into_iter().enumerate().filter(|(i, _)| !(7..=9).contains(i)).map(|(_, r)| r).collect();
    let out = run(SODICK, &machine(json!({ "flushing": "high" })), &rough);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(body(&out).contains(&"T84") && !body(&out).contains(&"C002"), "{}", out.program);
}

/// A skim flushing without the rough flushing would leave the skim's word
/// active for the next rough cut: the machine is refused, nothing is cut.
#[test]
fn a_skim_flushing_without_the_rough_flushing_or_an_unknown_word_is_refused_and_nothing_is_cut() {
    let cases = [
        (json!({ "skim_flushing": "low" }), "edm.skim_flushing is stated without edm.flushing"),
        (json!({ "flushing": "high", "skim_flushing": "medium" }), "edm.skim_flushing \"medium\" is not high or low"),
        (json!({ "flushing": "high", "skim_flushing": true }), "edm.skim_flushing true is not high or low"),
    ];
    for (edm, why) in cases {
        let out = run(SODICK, &machine(edm.clone()), &skimmed("Punch1"));
        assert!(out.warnings.iter().any(|w| w.contains(why) && w.contains("Synthetic wire")), "{edm}: {:?}", out.warnings);
        for word in ["T91", "C001", "C002", "G01", "G00", "T84", "T85"] {
            assert!(!body(&out).iter().any(|l| l.split_whitespace().any(|w| w == word)), "{edm}: {word}\n{}", out.program);
        }
    }
}

/// A machine that states no flushing posts its skims as before (#2649):
/// no T word, and the header leaves the flushing to the operator.
#[test]
fn unstated_flushing_posts_a_skim_as_before() {
    let out = run(SODICK, &machine(json!({})), &skimmed("Punch1"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(
        body(&out),
        ["G21", "G90", "G54", "G40 G00 X-30. Y0.", "T91", "C001", "G01 X-20. Y0.", "G01 X-30. Y0.", "C002", "G01 X-20.5 Y0.", "G01 X-30. Y0.", "T90", "M02"],
        "{}",
        out.program
    );
    let n = notes(&out);
    assert!(n.contains(&"(TANK FILL AND FLUSHING ARE NOT PROGRAMMED - SET THEM AT THE CONTROL)"), "{n:?}");
    assert!(!n.iter().any(|l| l.contains("SKIM PASS")), "{n:?}");
}

/// The generic ISO wire post has no word for it: it says so.
#[test]
fn the_generic_wire_post_notes_a_stated_skim_flushing_it_does_not_program() {
    let codes = json!({ "condition": "FX-COND {id}", "comp_off": "FX-COMP-OFF", "thread": "FX-THREAD", "cut": "FX-CUT", "tip_disposal": "FX-TIP-OUT" });
    let stated = machine(json!({ "flushing": "high", "skim_flushing": "low", "codes": codes }));
    let out = run(WIRE_ISO, &stated, &skimmed("Punch1"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(notes(&out).contains(&"(SKIM FLUSHING LOW IS STATED BUT NOT PROGRAMMED - SET IT AT THE CONTROL)"), "{}", out.program);
    let plain = run(WIRE_ISO, &machine(json!({ "flushing": "high", "codes": codes })), &skimmed("Punch1"));
    assert!(!plain.program.contains("SKIM FLUSHING"), "{}", plain.program);
}
