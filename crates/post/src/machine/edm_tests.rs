//! #530: the `edm` section's reader and refusals, by name - the pure
//! parser cases (the app suite `edm_machine` keeps the posting gate and
//! the project round trip). Our own synthetic templates, no vendor
//! values.

use super::*;
use crate::machine::parse_mch;

pub(crate) const WIRE: &str = r#"{
  "general": { "model": "WIRE (TEST)", "kind": "wire_edm" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [ { "id": "base", "name": "table", "type": "table", "parts": [
      { "id": "x", "name": "X", "type": "linear", "parts": [ { "id": "y", "name": "Y", "type": "linear" } ] },
      { "id": "head", "name": "guides", "type": "head" } ] } ] } },
  "edm": {}
}"#;

pub(crate) const SINKER: &str = r#"{
  "general": { "model": "SINKER (TEST)", "kind": "sinker_edm" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [ { "id": "base", "name": "table", "type": "table", "parts": [
      { "id": "z", "name": "Z", "type": "linear", "parts": [ { "id": "head", "name": "ram", "type": "head" } ] } ] } ] } },
  "edm": {}
}"#;

const MILL: &str = r#"{
  "general": { "model": "MILL (TEST)", "kind": "mill" },
  "kinematics": { "default": { "units": { "length": "mm" }, "parts": [
    { "id": "x", "name": "X", "type": "linear" }, { "id": "y", "name": "Y", "type": "linear" },
    { "id": "z", "name": "Z", "type": "linear", "parts": [ { "id": "head", "type": "head" } ] } ] } }
}"#;

pub(crate) fn with_edm(base: &str, edm: &str) -> String {
    let mut v: Value = serde_json::from_str(base).expect("template json");
    v["edm"] = serde_json::from_str(edm).expect("edm json");
    v.to_string()
}

pub(crate) fn inch(text: &str) -> String {
    let mut v: Value = serde_json::from_str(text).expect("json");
    v["kinematics"]["default"]["units"]["length"] = "inch".into();
    v.to_string()
}

#[test]
fn a_filled_edm_section_reads_and_scales_with_the_unit() {
    let mm = with_edm(
        WIRE,
        r#"{ "lower_guide_z": 0, "upper_guide_z": 150, "max_taper_deg": 15,
             "wire_diameters": [0.25, 0.2], "awt": true }"#,
    );
    let e = parse_mch(&mm).expect("parses").edm.expect("edm");
    assert_eq!((e.lower_guide_z, e.upper_guide_z, e.max_taper_deg), (Some(0.0), Some(150.0), Some(15.0)));
    assert_eq!(e.wire_diameters, vec![0.25, 0.2]);
    assert_eq!(e.awt, Some(true));

    let text = inch(&with_edm(
        SINKER,
        r#"{ "max_orbit_radius": 0.1, "pickup": "3R macro",
             "orbits": [ { "kind": "circular", "word": "OBT1" }, { "kind": "vector", "word": "OBT2" } ] }"#,
    ));
    let e = parse_mch(&text).expect("parses").edm.expect("edm");
    assert!((e.max_orbit_radius.expect("radius") - 2.54).abs() < 1e-9, "an inch length reads in mm: {e:?}");
    assert_eq!(e.orbits.len(), 2);
    assert_eq!(e.orbits[1].word, "OBT2");
    assert_eq!(e.pickup.as_deref(), Some("3R macro"));
}

#[test]
fn a_garbled_edm_section_refuses_the_file_by_name() {
    for (base, edm, says) in [
        (WIRE, r#"{ "max_taper_deg": 95 }"#, "max_taper_deg"),
        (WIRE, r#"{ "max_taper_deg": "steep" }"#, "max_taper_deg"),
        (WIRE, r#"{ "lower_guide_z": 100, "upper_guide_z": 50 }"#, "upper_guide_z"),
        (WIRE, r#"{ "wire_diameters": [0.25, -1] }"#, "wire_diameters"),
        (WIRE, r#"{ "awt": "yes" }"#, "awt"),
        (SINKER, r#"{ "orbits": [ { "kind": "spiral", "word": "S" } ] }"#, "spiral"),
        (SINKER, r#"{ "orbits": [ { "kind": "circular", "word": " " } ] }"#, "word"),
        (SINKER, r#"{ "orbits": [ { "kind": "circular", "word": "O1", "radius": 1 } ] }"#, "radius is not an orbit key"),
        (SINKER, r#"{ "max_orbit_radius": 0 }"#, "max_orbit_radius"),
        (SINKER, r#"[1, 2]"#, "edm must be an object"),
    ] {
        let err = parse_mch(&with_edm(base, edm)).expect_err(edm);
        assert!(err.contains(says), "{edm}: {err}");
    }
}

#[test]
fn the_kind_words_read_and_an_unknown_kind_still_refuses() {
    for (w, k) in [
        ("wire_edm", Kind::WireEdm),
        ("Wire EDM", Kind::WireEdm),
        ("wedm", Kind::WireEdm),
        ("sinker_edm", Kind::SinkerEdm),
        ("die-sinker", Kind::SinkerEdm),
        ("ram EDM", Kind::SinkerEdm),
    ] {
        assert_eq!(Kind::from_word(w), Some(k), "{w}");
        assert_eq!(Kind::from_word(k.word()), Some(k), "{k:?} round-trips its word");
    }
    for w in ["EDM", "wire", "Die Sinker EDM"] {
        assert_eq!(Kind::from_word(w), None, "{w} is not read as a kind");
    }
    let err = parse_mch(&WIRE.replace("wire_edm", "laser")).expect_err("unknown kind");
    assert!(err.contains("wire_edm") && err.contains("sinker_edm"), "{err}");
}

/// Review #551 request 1: a misspelt key refuses by its own name.
#[test]
fn an_unknown_edm_key_is_refused_by_name() {
    for (base, edm, says) in [
        (WIRE, r#"{ "max_taper": 10 }"#, "edm.max_taper is not an edm key (lower_guide_z"),
        (WIRE, r#"{ "wire_diam": [0.25] }"#, "edm.wire_diam is not an edm key"),
        (SINKER, r#"{ "orbit": [] }"#, "edm.orbit is not an edm key (orbits"),
    ] {
        let err = parse_mch(&with_edm(base, edm)).expect_err(edm);
        assert!(err.contains(says), "{edm}: {err}");
    }
}

/// Review #551 request 2: only an EDM states the section, and only its
/// own kind's keys.
#[test]
fn an_edm_section_on_another_kind_or_with_the_other_kinds_keys_is_refused() {
    for edm in ["{}", r#"{ "max_taper_deg": 10 }"#] {
        let err = parse_mch(&with_edm(MILL, edm)).expect_err("a mill with edm");
        assert!(err.contains("only a wire_edm or sinker_edm machine states an edm section"), "{err}");
    }
    let undeclared = with_edm(&MILL.replace(r#""kind": "mill""#, r#""kind": """#), "{}");
    assert!(parse_mch(&undeclared).is_err(), "an inferred kind is never an EDM");
    let err = parse_mch(&with_edm(WIRE, r#"{ "orbits": [ { "kind": "circular", "word": "O1" } ] }"#)).expect_err("orbits on a wire");
    assert!(err.contains("edm.orbits is a sinker_edm key; this is a wire_edm machine"), "{err}");
    for key in WIRE_KEYS {
        let err = parse_mch(&with_edm(SINKER, &format!(r#"{{ "{key}": 1 }}"#))).expect_err(key);
        assert!(err.contains(&format!("edm.{key} is a wire_edm key; this is a sinker_edm machine")), "{err}");
    }
    for key in SINKER_KEYS {
        let err = parse_mch(&with_edm(WIRE, &format!(r#"{{ "{key}": 1 }}"#))).expect_err(key);
        assert!(err.contains(&format!("edm.{key} is a sinker_edm key")), "{err}");
    }
}

/// Review #551 request 3: an EDM states no spindle, live tool or
/// coolant, refused by the key's name like plasma.
#[test]
fn an_edm_stating_a_spindle_or_coolant_is_refused_by_name() {
    for base in [WIRE, SINKER] {
        let spindle = base.replace(r#""type": "head" }"#, r#""type": "head", "spindle": { "max_speed": 12000 } }"#);
        assert_ne!(spindle, base, "the template has a head");
        let err = parse_mch(&spindle).expect_err("spindle on an EDM");
        assert!(err.contains("spindle.max_speed: an EDM machine has no spindle"), "{err}");
        let coolant = base.replace(
            r#""type": "head" }"#,
            r#""type": "head", "tool_station": { "coolants": [ { "name": "Flood", "on": "M8" } ] } }"#,
        );
        let err = parse_mch(&coolant).expect_err("coolant on an EDM");
        assert!(err.contains("coolants: an EDM machine states no coolant"), "{err}");
        let keys: Vec<String> = edm_refusals(&serde_json::from_str(&spindle).expect("json")).into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["spindle.max_speed"]);
        assert!(edm_refusals(&serde_json::from_str(base).expect("json")).is_empty(), "the template keeps the rules");
    }
}

/// Review #551 request 4: an inch number that overflows once scaled is
/// refused by its key, not later as an unnamed limit.
#[test]
fn a_length_that_overflows_once_scaled_is_refused_by_name() {
    for (edm, says) in [
        (r#"{ "lower_guide_z": 1e308 }"#, "edm.lower_guide_z"),
        (r#"{ "upper_guide_z": -1e308 }"#, "edm.upper_guide_z"),
        (r#"{ "wire_diameters": [1e308] }"#, "edm.wire_diameters"),
    ] {
        let text = inch(&with_edm(WIRE, edm));
        let err = parse_mch(&text).expect_err(edm);
        assert!(err.contains(says), "{edm}: {err}");
    }
    let err = parse_mch(&inch(&with_edm(SINKER, r#"{ "max_orbit_radius": 1e308 }"#))).expect_err("orbit radius");
    assert!(err.contains("edm.max_orbit_radius"), "{err}");
}

/// The parsed form's gate (a hand-edited project file) keeps the kind
/// rules: an edm on a mill, a sinker key on a wire, a spindle on an EDM.
#[test]
fn the_parsed_machine_keeps_the_kind_rules() {
    let wire = parse_mch(WIRE).expect("wire");
    assert!(wire.is_sane());
    let mut m = wire.clone();
    m.edm = Some(Edm { max_orbit_radius: Some(1.0), ..Edm::default() });
    assert!(!m.is_sane(), "a sinker key on a wire");
    let mut m = wire.clone();
    m.spindle_max = Some(12000.0);
    assert!(!m.is_sane(), "a spindle on a wire");
    let mut m = parse_mch(MILL).expect("mill");
    assert!(m.is_sane());
    m.edm = Some(Edm::default());
    assert!(!m.is_sane(), "an edm section on a mill");
    let mut s = parse_mch(SINKER).expect("sinker");
    s.edm = Some(Edm { awt: Some(true), ..Edm::default() });
    assert!(!s.is_sane(), "a wire key on a sinker");
}
