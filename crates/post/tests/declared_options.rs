//! #258: declared per-post option schemas (`declaredOptions`), override
//! validation, default resolution, and post-script option behavior across
//! shipped milling `.cps` scripts.
//!
//! Provenance: our own contract over public RS-274 / controller behavior;
//! no vendor material consulted.

use camcraft_post::cl::MoveKind;
use camcraft_post::post::cnc::{self, MoveRec, Record};
use camcraft_post::post::options::{
    declared_options, parse_json_overrides, resolve_options, validate_overrides, OptionKind,
    OptionValue,
};
use camcraft_post::post::{engine, machine, ProgramId};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn fanuc_cps() -> String {
    read("assets/posts/fanuc.cps")
}

fn default_vmc() -> Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("vmc machine json")
}

fn vmc_with_safe_retract(z: f64) -> Value {
    let parsed = camcraft_post::machine::parse_mch(&read("assets/machines/3axis-vmc.mch")).expect("parse");
    let mut m = machine::mirror(&parsed);
    m["safeRetractZ"] = json!(z);
    m
}

fn simple_mill_stream() -> String {
    let mv = |kind, x, y, z, feed, center, ccw| {
        Record::Move(MoveRec {
            kind,
            x,
            y,
            z,
            feed,
            center,
            ccw,
            plane: Default::default(),
            abc: None,
        })
    };
    cnc::serialize(&[
        Record::Param { name: "OperationName".into(), value: json!("Face1") },
        Record::Param { name: "ToolDiameter".into(), value: json!(10.0) },
        Record::Param { name: "WorkOffset".into(), value: json!(1) },
        Record::Param { name: "ClearancePlane".into(), value: json!(50.0) },
        Record::Param { name: "RetractPlane".into(), value: json!(10.0) },
        Record::Tool { number: 1, name: "10mm Flat Endmill".into() },
        Record::Param { name: "SpindleSpeed".into(), value: json!(5000.0) },
        Record::Section { begin: true },
        mv(MoveKind::Rapid, 0.0, 0.0, 10.0, None, None, false),
        mv(MoveKind::Rapid, 50.0, 0.0, 10.0, None, None, false),
        mv(MoveKind::Feed, 50.0, 0.0, -2.0, Some(500.0), None, false),
        mv(MoveKind::Feed, 25.0, 25.0, -2.0, Some(500.0), Some((25.0, 0.0)), true),
        Record::Section { begin: false },
        Record::Param { name: "OperationName".into(), value: json!("Finish1") },
        Record::Param { name: "ToolDiameter".into(), value: json!(6.0) },
        Record::Param { name: "WorkOffset".into(), value: json!(1) },
        Record::Param { name: "ClearancePlane".into(), value: json!(50.0) },
        Record::Param { name: "RetractPlane".into(), value: json!(10.0) },
        Record::Tool { number: 2, name: "6mm Flat Endmill".into() },
        Record::Param { name: "SpindleSpeed".into(), value: json!(7000.0) },
        Record::Section { begin: true },
        mv(MoveKind::Rapid, 0.0, 0.0, 10.0, None, None, false),
        mv(MoveKind::Feed, 10.0, 0.0, -1.0, Some(400.0), None, false),
        Record::Section { begin: false },
    ])
}

#[test]
fn shipped_milling_posts_declare_valid_option_schemas() {
    for rel in [
        "assets/posts/fanuc.cps",
        "assets/posts/haas.cps",
        "assets/posts/mazak.cps",
        "assets/posts/doosan-dnm.cps",
        "assets/posts/tormach.cps",
        "assets/posts/grblhal.cps",
        "assets/posts/tinyg.cps",
        "assets/posts/carvera.cps",
    ] {
        let cps = read(rel);
        let schema = declared_options(&cps).unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert!(!schema.is_empty(), "{rel} must declare options");
        let resolved = resolve_options(&schema, &BTreeMap::new()).unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert_eq!(resolved.len(), schema.len(), "{rel}: every option resolves");
    }

    let dnm = declared_options(&read("assets/posts/doosan-dnm.cps")).expect("doosan schema");
    for key in [
        "showSequenceNumbers",
        "sequenceNumberStart",
        "sequenceNumberIncrement",
        "separateWordsWithSpace",
        "writeMachine",
        "writeToolList",
        "writeVersion",
        "optionalStop",
        "useRadiusArcs",
        "safeRetract",
        "rotaryBrake",
        "rotaryClamp",
        "rotaryUnclamp",
        "chipConveyor",
        "chipConveyorStart",
        "chipConveyorStop",
        "useM8ForTSC",
        "smoothingLevel",
        "probingStyle",
    ] {
        assert!(dnm.iter().any(|o| o.key == key), "doosan missing declared option {key}");
    }

    let tinyg = declared_options(&read("assets/posts/tinyg.cps")).expect("tinyg schema");
    let dwell = tinyg
        .iter()
        .find(|o| o.key == "SpindleDwellSeconds")
        .expect("tinyg declares SpindleDwellSeconds");
    assert_eq!(dwell.default, OptionValue::Float(0.0));
    assert!(matches!(
        dwell.kind,
        OptionKind::Number {
            min: Some(0.0),
            max: Some(4294.0)
        }
    ));
}

#[test]
fn malformed_schemas_and_bad_overrides_are_refused_by_name() {
    let bad_type = r#"declaredOptions = { foo: { type: "object", default: {} } };"#;
    let err = declared_options(bad_type).expect_err("bad type refused");
    assert!(err.to_string().contains("foo"), "{err}");

    let bad_enum = r#"declaredOptions = { mode: { type: "enum", choices: ["a", "b"], default: "c" } };"#;
    let err = declared_options(bad_enum).expect_err("bad enum default refused");
    assert!(err.to_string().contains("mode"), "{err}");

    let reserved = r#"declaredOptions = { Units: { type: "string", default: "mm" } };"#;
    let err = declared_options(reserved).expect_err("reserved key refused");
    assert!(err.to_string().contains("Units"), "{err}");

    let schema = declared_options(&fanuc_cps()).expect("fanuc schema");
    let mut bad = Map::new();
    bad.insert("noSuchOption".into(), json!(true));
    let err = parse_json_overrides(&schema, &bad).expect_err("unknown key refused");
    assert!(err.contains("noSuchOption"), "{err}");

    let mut bad_int = Map::new();
    bad_int.insert("sequenceNumberStart".into(), json!(0));
    let err = parse_json_overrides(&schema, &bad_int).expect_err("out-of-range int refused");
    assert!(err.contains("sequenceNumberStart"), "{err}");

    let mut bad_choice = Map::new();
    bad_choice.insert("safeRetract".into(), json!("G99"));
    let err = parse_json_overrides(&schema, &bad_choice).expect_err("bad enum choice refused");
    assert!(err.contains("safeRetract"), "{err}");

    let mut typed_bad = BTreeMap::new();
    typed_bad.insert("separateWordsWithSpace".into(), OptionValue::Int(1));
    let err = validate_overrides(&schema, &typed_bad).expect_err("type mismatch refused");
    assert!(err.contains("separateWordsWithSpace"), "{err}");
}

#[test]
fn fanuc_defaults_are_unchanged_and_declared_options_drive_output() {
    let cps = fanuc_cps();
    let mch = default_vmc();
    let stream = simple_mill_stream();
    let schema = declared_options(&cps).expect("fanuc schema");
    let defaults = resolve_options(&schema, &BTreeMap::new()).expect("defaults");
    let id = ProgramId::default();

    let baseline = engine::run_program_with(&cps, &mch, &stream, &Value::Null, &id).expect("baseline");
    let with_defaults =
        engine::run_program_with(&cps, &mch, &stream, &Value::Object(defaults), &id).expect("defaults");
    assert_eq!(baseline.program, with_defaults.program, "defaults must match baseline byte-for-byte");

    // Sequence numbers + no spaces + header comments + optional stop + R arcs + safeRetract G53.
    let mut ov = BTreeMap::new();
    ov.insert("showSequenceNumbers".into(), OptionValue::Str("every-block".into()));
    ov.insert("sequenceNumberStart".into(), OptionValue::Int(100));
    ov.insert("sequenceNumberIncrement".into(), OptionValue::Int(5));
    ov.insert("separateWordsWithSpace".into(), OptionValue::Bool(false));
    ov.insert("writeMachine".into(), OptionValue::Bool(true));
    ov.insert("writeToolList".into(), OptionValue::Bool(true));
    ov.insert("writeVersion".into(), OptionValue::Bool(true));
    ov.insert("optionalStop".into(), OptionValue::Bool(true));
    ov.insert("useRadiusArcs".into(), OptionValue::Bool(true));
    ov.insert("safeRetract".into(), OptionValue::Str("G53".into()));

    let opts = resolve_options(&schema, &ov).expect("resolve");
    let out = engine::run_program_with(
        &cps,
        &vmc_with_safe_retract(-5.0),
        &stream,
        &Value::Object(opts.clone()),
        &id,
    )
    .expect("runs custom fanuc options");
    assert!(out.warnings.is_empty(), "warnings: {:?}", out.warnings);
    assert!(out.program.contains("N100 "), "sequence numbers start at 100");
    assert!(out.program.contains("N105 "), "sequence numbers increment by 5");
    assert!(out.program.contains("G90G54G40G49G80"), "words joined without spaces");
    assert!(out.program.contains("T1") && out.program.contains("10mm Flat Endmill"), "tool list header");
    assert!(out.program.contains("MACHINE:"), "machine header");
    assert!(out.program.contains("POST:"), "post header");
    assert!(out.program.lines().any(|l| l.ends_with("M1") || l.contains("M1")), "optional stop");
    assert!(out.program.contains("G03") && out.program.contains("R"), "radius arcs written");
    assert!(!out.program.contains("I-") && !out.program.contains("I25."), "I omitted in R-arc mode");
    assert!(out.program.contains("G53"), "G53 retract written");
    assert!(out.program.contains("Z-5."), "stated safeRetractZ written: {}", out.program);
    assert!(!out.program.contains("G28"), "G28 omitted when G53 selected");

    // G53 on a machine without safeRetractZ must refuse by name, never guess Z0.
    let mut no_safe_z = default_vmc();
    no_safe_z.as_object_mut().expect("obj").remove("safeRetractZ");
    let no_z = engine::run_program_with(&cps, &no_safe_z, &stream, &Value::Object(opts), &id)
        .expect("runs to warning");
    assert!(
        no_z.warnings.iter().any(|w| w.contains("safeRetract") && w.contains("safeRetractZ")),
        "missing safeRetractZ refused by name: {:?}",
        no_z.warnings
    );
}
