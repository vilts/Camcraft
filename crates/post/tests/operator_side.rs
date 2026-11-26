//! #2632: the operator side of a machine file. An old file without
//! `general.operator_side` still loads and takes the default of its kind;
//! a file that names a side keeps it; a wrong word refuses by name.
//! Gate map: `camcraft_post::machine::operator_side`.
//! Provenance: our own tests over our own machine files.

use camcraft_post::machine::{parse_mch, Kind, Machine, OperatorSide};

fn file(general: &str) -> String {
    format!(
        r#"{{ "general": {{ "model": "T", "vendor": "V"{general} }},
            "kinematics": {{ "default": {{ "units": {{ "length": "mm" }}, "parts": [] }} }} }}"#
    )
}

#[test]
fn an_old_file_without_the_field_loads_and_takes_the_default_of_its_kind() {
    let m = parse_mch(&file("")).expect("an old file loads");
    assert_eq!(m.operator_side, None, "unstated stays unstated");
    assert_eq!(OperatorSide::of(&m), OperatorSide::MinusY, "a mill is seen from the front");
    let null = parse_mch(&file(r#", "operator_side": null"#)).expect("null is unstated");
    assert_eq!(null.operator_side, None);
    // Each library file is an old file: it states no side and loads.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).expect("machine library") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("mch") {
            continue;
        }
        let m = parse_mch(&std::fs::read_to_string(&path).expect("read")).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(m.operator_side, None, "{}", path.display());
        checked += 1;
    }
    assert!(checked > 0);
}

#[test]
fn each_kind_has_its_documented_default_side() {
    for (kind, side) in [
        (Kind::Mill, OperatorSide::MinusY),
        (Kind::MillTurn, OperatorSide::MinusY),
        (Kind::Plasma, OperatorSide::MinusY),
        (Kind::Waterjet, OperatorSide::MinusY),
        (Kind::WireEdm, OperatorSide::MinusY),
        (Kind::SinkerEdm, OperatorSide::MinusY),
        (Kind::Lathe, OperatorSide::PlusY),
    ] {
        assert_eq!(OperatorSide::default_for(kind), side, "{kind:?}");
        let m = Machine { kind: Some(kind.word().to_owned()), ..Machine::default() };
        assert_eq!(OperatorSide::of(&m), side, "{kind:?} by its declared word");
    }
}

#[test]
fn a_file_that_names_a_side_keeps_it_over_the_default() {
    for side in OperatorSide::ALL {
        let m = parse_mch(&file(&format!(r#", "operator_side": "{}""#, side.word()))).expect("a stated side loads");
        assert_eq!(m.operator_side, Some(side));
        assert_eq!(OperatorSide::of(&m), side);
    }
}

#[test]
fn a_wrong_word_refuses_the_file_by_name() {
    for bad in [r#""front""#, r#""""#, "1", "true", r#"["-Y"]"#] {
        let err = parse_mch(&file(&format!(r#", "operator_side": {bad}"#))).expect_err(bad);
        assert!(err.contains("general.operator_side") && err.contains("\"-Y\""), "{bad}: {err}");
    }
}

#[test]
fn a_stored_machine_without_the_field_still_reads_and_a_stated_side_round_trips() {
    // A project file of an earlier version stores no `operator_side`.
    let old: Machine = serde_json::from_str(r#"{ "model": "T", "vendor": "V", "axes": [] }"#).expect("old stored machine");
    assert_eq!(old.operator_side, None);
    assert!(!serde_json::to_string(&old).expect("write").contains("operator_side"), "unstated writes no key");
    let stated = Machine { operator_side: Some(OperatorSide::PlusX), ..Machine::default() };
    let text = serde_json::to_string(&stated).expect("write");
    assert!(text.contains(r#""operator_side":"+X""#), "{text}");
    let back: Machine = serde_json::from_str(&text).expect("read");
    assert_eq!(back, stated);
    assert!(back.is_sane());
}
