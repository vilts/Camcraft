//! #531: the condition table's reader, lookup and refusals, by name -
//! the pure parser cases (the app suite `edm_conditions` keeps the
//! project round trip). Our own SYNTHETIC conditions: invented numbers
//! of a plausible size, from no machine's manual.

use super::*;
use crate::machine::edm::tests::{inch, with_edm, SINKER, WIRE};
use crate::machine::parse_mch;

fn wire_with(conds: &str) -> String {
    with_edm(WIRE, &format!(r#"{{ "conditions": {conds} }}"#))
}

fn sinker_with(conds: &str) -> String {
    with_edm(SINKER, &format!(r#"{{ "conditions": {conds} }}"#))
}

/// Synthetic: a rough, a skim and a finish, the finish stating no gap.
const TABLE: &str = r#"[
    { "id": "C110", "name": "rough", "gap": 0.04, "wire_diameter": 0.25, "role": "rough" },
    { "id": "C210", "gap": 0.02, "wire_diameter": 0.25, "role": "skim", "notes": "synthetic" },
    { "id": 330, "wire_diameter": 0.25, "role": "finish" }
]"#;

fn edm(text: &str) -> Edm {
    parse_mch(text).expect("parses").edm.expect("edm section")
}

fn refusal(text: &str) -> String {
    parse_mch(text).expect_err("refused")
}

#[test]
fn a_stated_table_reads_and_the_template_states_none() {
    assert!(edm(WIRE).conditions.is_empty(), "no vendor conditions ship");
    assert!(edm(&wire_with("[]")).conditions.is_empty(), "[] reads as no table");
    let e = edm(&wire_with(TABLE));
    assert_eq!(e.conditions.len(), 3);
    let ids: Vec<&str> = e.conditions.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["C110", "C210", "330"], "a numeric id reads as its word");
    assert_eq!(e.conditions[1].notes, "synthetic");
    assert!(e.is_sane());
    let s = edm(&sinker_with(r#"[ { "id": "E1", "gap": 0.05, "undersize": 0.2 } ]"#));
    assert_eq!(s.conditions[0].undersize, Some(0.2));
}

#[test]
fn an_inch_file_states_its_gap_in_inches() {
    let e = edm(&inch(&wire_with(r#"[ { "id": "R1", "gap": 0.001, "wire_diameter": 0.01 } ]"#)));
    let c = &e.conditions[0];
    assert!((c.gap.expect("gap") - 0.0254).abs() < 1e-12 && (c.wire_diameter.expect("wire") - 0.254).abs() < 1e-12, "{c:?}");
}

#[test]
fn the_lookup_refuses_a_missing_condition_or_field_by_name() {
    let e = edm(&wire_with(TABLE));
    let rough = e.condition("Wire 1", "C110", &[ConditionField::Gap, ConditionField::WireDiameter]).expect("complete");
    assert_eq!((rough.gap, rough.wire_diameter), (Some(0.04), Some(0.25)));
    let none = e.condition("Wire 1", "C999", &[ConditionField::Gap]).expect_err("missing");
    assert!(none.contains("Wire 1") && none.contains("C999"), "{none}");
    let gapless = e.condition("Wire 1", "330", &[ConditionField::Gap]).expect_err("no gap");
    assert!(gapless.contains("330") && gapless.contains("gap"), "{gapless}");
    let sinker = e.condition("Wire 1", "C110", &[ConditionField::Undersize]).expect_err("no undersize");
    assert!(sinker.contains("undersize"), "{sinker}");
    assert!(Edm::default().condition("Bare", "C110", &[]).is_err(), "an empty table states nothing");
}

#[test]
fn a_garbled_table_refuses_the_file() {
    for (conds, says) in [
        (r#"{ "id": "C1" }"#, "must be a list"),
        (r#"[5]"#, "not a condition object"),
        (r#"[ { "gap": 0.02 } ]"#, "states no id"),
        (r#"[ { "id": true } ]"#, "states no id"),
        (r#"[ { "id": "C1" }, { "id": "C1" } ]"#, "stated twice"),
        (r#"[ { "id": "C1", "gap": -0.01 } ]"#, r#""C1" gap: -0.01 mm must be above 0 mm"#),
        (r#"[ { "id": "C1", "gap": "wide" } ]"#, "gap must be a number"),
        (r#"[ { "id": "C1", "role": "polish" } ]"#, "polish"),
        (r#"[ { "id": "C1", "name": 7 } ]"#, "name must be a word"),
    ] {
        let err = refusal(&wire_with(conds));
        assert!(err.contains(says), "{conds}: {err}");
    }
}

/// Review #560 request 1: the geometry the kerf reads is bounded, each
/// refusal naming the condition id, the key and the unit.
#[test]
fn the_kerf_geometry_is_bounded_by_name() {
    for (conds, says) in [
        (r#"[ { "id": "C1", "gap": 0, "wire_diameter": 0.25 } ]"#, r#""C1" gap: 0 mm must be above 0 mm"#),
        (r#"[ { "id": "C1", "gap": 0.125, "wire_diameter": 0.25 } ]"#, r#""C1" gap: 0.125 mm must be smaller than the wire's radius (0.125 mm)"#),
        (r#"[ { "id": "C1", "gap": 5, "wire_diameter": 0.25 } ]"#, "smaller than the wire's radius"),
        (r#"[ { "id": "C1", "gap": 0.02, "wire_diameter": 50 } ]"#, r#""C1" wire_diameter: 50 mm is outside [0.02, 1] mm"#),
        (r#"[ { "id": "C1", "wire_diameter": 0 } ]"#, "wire_diameter: 0 mm is outside [0.02, 1] mm"),
        // An inch wire typed into a mm file: below the cited 0.02 mm.
        (r#"[ { "id": "C1", "gap": 0.001, "wire_diameter": 0.010 } ]"#, r#""C1" wire_diameter: 0.01 mm is outside [0.02, 1] mm"#),
        // The gap is bounded on its own, with no wire stated beside it.
        (r#"[ { "id": "C1", "gap": 5 } ]"#, r#""C1" gap: 5 mm is outside (0, 0.5] mm"#),
    ] {
        let err = refusal(&wire_with(conds));
        assert!(err.contains(says), "{conds}: {err}");
    }
    for (conds, says) in [
        (r#"[ { "id": "E1", "undersize": 0 } ]"#, r#""E1" undersize: 0 mm is outside (0, 1] mm"#),
        (r#"[ { "id": "E1", "undersize": 3 } ]"#, "undersize: 3 mm is outside (0, 1] mm"),
        (r#"[ { "id": "E1", "gap": 0 } ]"#, "gap: 0 mm must be above 0 mm"),
        (r#"[ { "id": "E1", "gap": 5, "undersize": 0.1 } ]"#, r#""E1" gap: 5 mm is outside (0, 0.5] mm"#),
        (r#"[ { "id": "E1", "gap": 0.3, "undersize": 0.1 } ]"#, r#""E1" gap: 0.3 mm is larger than the undersize (0.1 mm)"#),
    ] {
        let err = refusal(&sinker_with(conds));
        assert!(err.contains(says), "{conds}: {err}");
    }
    // The bounds are on mm: an inch file's 0.05 in (1.27 mm) wire refuses,
    // its 0.010 in (0.254 mm) wire passes.
    let err = refusal(&inch(&wire_with(r#"[ { "id": "C1", "wire_diameter": 0.05 } ]"#)));
    assert!(err.contains("wire_diameter: 1.27"), "{err}");
    assert!(parse_mch(&inch(&wire_with(r#"[ { "id": "C1", "gap": 0.001, "wire_diameter": 0.010 } ]"#))).is_ok());
    // At the edges: the thinnest wire, the widest gap, a gap equal to its undersize.
    assert!(parse_mch(&wire_with(r#"[ { "id": "C1", "gap": 0.005, "wire_diameter": 0.02 } ]"#)).is_ok());
    assert!(parse_mch(&wire_with(r#"[ { "id": "C1", "gap": 0.5 } ]"#)).is_ok());
    assert!(parse_mch(&sinker_with(r#"[ { "id": "E1", "gap": 0.3, "undersize": 0.3 } ]"#)).is_ok());
}

/// Review #560 request 2: each kind's own condition field.
#[test]
fn a_condition_states_only_its_kinds_field() {
    let err = refusal(&wire_with(r#"[ { "id": "C1", "undersize": 0.1 } ]"#));
    assert!(err.contains(r#"edm condition "C1" undersize is a sinker field; this is a wire_edm machine"#), "{err}");
    let err = refusal(&sinker_with(r#"[ { "id": "E1", "wire_diameter": 0.25 } ]"#));
    assert!(err.contains(r#"edm condition "E1" wire_diameter is a wire field; this is a sinker_edm machine"#), "{err}");
}

/// Review #560 request 3: a condition's wire is one the machine takes,
/// when the machine states its wires (after unit scaling).
#[test]
fn a_conditions_wire_must_be_one_the_machine_takes() {
    let one = |wires: &str, d: &str| with_edm(WIRE, &format!(r#"{{ "wire_diameters": {wires}, "conditions": [ {{ "id": "C1", "wire_diameter": {d} }} ] }}"#));
    let err = refusal(&one("[0.25]", "0.30"));
    assert!(err.contains(r#""C1" wire_diameter: 0.3 mm is not one of the machine's edm.wire_diameters (0.25 mm)"#), "{err}");
    assert!(parse_mch(&one("[0.25, 0.3]", "0.30")).is_ok(), "a listed wire passes");
    assert!(parse_mch(&one("[0.25]", "0.2505")).is_ok(), "within {WIRE_MATCH_MM} mm");
    assert!(parse_mch(&wire_with(r#"[ { "id": "C1", "wire_diameter": 0.3 } ]"#)).is_ok(), "no wires stated: any in range");
    assert!(parse_mch(&inch(&one("[0.01]", "0.01"))).is_ok(), "both sides scale");
}

/// Review #560 request 4: one selector, one id.
#[test]
fn one_selector_answers_one_condition() {
    for (conds, says) in [
        (r#"[ { "id": 1 }, { "id": "01" } ]"#, "are one selector"),
        (r#"[ { "id": "C1" }, { "id": "c1" } ]"#, "are one selector"),
        (r#"[ { "id": 1.0 } ]"#, "is not a whole number"),
        (r#"[ { "id": 1.5 } ]"#, "is not a whole number"),
        (r#"[ { "id": -3 } ]"#, "is not a whole number"),
        (r#"[ { "id": " C1 " } ]"#, "has surrounding spaces"),
        (r#"[ { "id": "  " } ]"#, "states no id"),
    ] {
        let err = refusal(&wire_with(conds));
        assert!(err.contains(says), "{conds}: {err}");
    }
    assert!(parse_mch(&wire_with(r#"[ { "id": 1 }, { "id": "C1" } ]"#)).is_ok());
}

/// Review #560 request 5: a misspelt condition key refuses by name.
#[test]
fn an_unknown_condition_key_is_refused_by_name() {
    let err = refusal(&wire_with(r#"[ { "id": "C1", "gapp": 0.02 } ]"#));
    assert!(err.contains(r#"edm condition "C1" gapp is not a condition key (id, name, gap"#), "{err}");
}

/// Review #560 request 6: finite after scaling, named.
#[test]
fn a_condition_length_that_overflows_once_scaled_is_refused_by_name() {
    let err = refusal(&inch(&wire_with(r#"[ { "id": "C1", "gap": 1e308 } ]"#)));
    assert!(err.contains(r#"edm condition "C1" gap = 1e+308 is out of range"#), "{err}");
}

/// The project reader's gate restates every rule above.
#[test]
fn a_hand_edited_project_machine_with_a_bad_table_is_not_sane() {
    let base = parse_mch(&with_edm(WIRE, &format!(r#"{{ "wire_diameters": [0.25], "conditions": {TABLE} }}"#))).expect("parses");
    assert!(base.is_sane());
    type Edit = fn(&mut Condition);
    let bad: [(&str, Edit); 9] = [
        ("a duplicate id", |c| c.id = "C110".into()),
        ("a case duplicate", |c| c.id = "c110".into()),
        ("a spaced id", |c| c.id = " C210".into()),
        ("a NaN gap", |c| c.gap = Some(f64::NAN)),
        ("a zero gap", |c| c.gap = Some(0.0)),
        ("a gap past the radius", |c| c.gap = Some(0.2)),
        ("a 50 mm wire", |c| c.wire_diameter = Some(50.0)),
        ("a wire the machine does not take", |c| c.wire_diameter = Some(0.3)),
        ("an undersize on a wire", |c| c.undersize = Some(0.1)),
    ];
    for (what, edit) in bad {
        let mut m = base.clone();
        if let Some(e) = m.edm.as_mut() {
            edit(&mut e.conditions[1]);
        }
        assert!(!m.is_sane(), "{what}");
    }
    let mut s = parse_mch(SINKER).expect("sinker");
    s.edm = Some(Edm { conditions: vec![Condition { id: "E1".into(), wire_diameter: Some(0.25), ..Condition::default() }], ..Edm::default() });
    assert!(!s.is_sane(), "a wire_diameter on a sinker");
    s.edm = Some(Edm { conditions: vec![Condition { id: "E1".into(), gap: Some(0.3), undersize: Some(0.1), ..Condition::default() }], ..Edm::default() });
    assert!(!s.is_sane(), "a sinker gap larger than its undersize");
}
