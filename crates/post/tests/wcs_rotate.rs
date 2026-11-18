//! #760: post::wcs_rotate transforms XY coordinates/arcs/cycles about
//! OriginX/Y into a Z-rotated WCS frame (Bake by default, G68/ROT in
//! Control mode when both machine and script declare support) and refuses
//! unverified cases by name.
//!
//! Provenance: RS-274 G68/G69 and DIN/ISO 840D ROT RPL coordinate rotation
//! conventions; synthetic test streams are our own.
//!
//! Gate map (M4.4): covers post::wcs_rotate, post::engine, post::machine,
//! assets/posts/{fanuc,haas,mazak,doosan-dnm,sinumerik}.cps.

mod inverted_support;

use camcraft_post::cl::{ArcPlane, CycleKind, MoveKind};
use camcraft_post::machine::parse_mch;
use camcraft_post::post::cnc::{self, CycleRec, MoveRec, Record};
use camcraft_post::post::{engine, machine as post_machine, probe_rec};
use inverted_support::{arc, cps, mv};
use serde_json::{Value, json};

fn mch_with_rotation(allow: Option<bool>) -> Value {
    let mut raw: Value =
        serde_json::from_str(include_str!("../../../assets/machines/3axis-vmc.mch")).expect("mch");
    if let Some(yes) = allow
        && let Some(obj) = raw.as_object_mut()
    {
        obj.insert("coordinateRotation".into(), json!(yes));
    }
    raw
}

fn rotated_square_stream(yaw_deg: f64, control_mode: bool) -> String {
    let mut recs = vec![
        Record::Param { name: "OperationName".into(), value: json!("RotTest") },
        Record::Param { name: "SpindleSpeed".into(), value: json!(5000.0) },
        Record::Param { name: "WorkOffset".into(), value: json!(1.0) },
        Record::Param { name: "OriginX".into(), value: json!(10.0) },
        Record::Param { name: "OriginY".into(), value: json!(5.0) },
        Record::Param { name: "OriginZ".into(), value: json!(0.0) },
        Record::Param { name: "ClearancePlane".into(), value: json!(25.0) },
        Record::Param { name: "RetractPlane".into(), value: json!(15.0) },
        Record::Param { name: "WcsYaw".into(), value: json!(yaw_deg) },
    ];
    if control_mode {
        recs.push(Record::Param { name: "WcsRotationMode".into(), value: json!("control") });
    }
    recs.extend([
        Record::Tool { number: 1, name: "EM".into() },
        Record::Section { begin: true },
        mv(MoveKind::Rapid, [10.0, 5.0, 25.0], None),
        // Setup (10, 25) is +20 along Setup +Y from Origin (10, 5) => WCS (20, 0) under +90 deg yaw.
        mv(MoveKind::Feed, [10.0, 25.0, -1.0], Some(600.0)),
        Record::Section { begin: false },
    ]);
    cnc::serialize(&recs)
}

#[test]
fn bake_keeps_setup_table_coordinates_without_g68() {
    let mut recs = vec![
        Record::Param { name: "OperationName".into(), value: json!("Bake") },
        Record::Param { name: "SpindleSpeed".into(), value: json!(5000.0) },
        Record::Param { name: "WorkOffset".into(), value: json!(1.0) },
        Record::Param { name: "OriginX".into(), value: json!(10.0) },
        Record::Param { name: "OriginY".into(), value: json!(5.0) },
        Record::Param { name: "OriginZ".into(), value: json!(0.0) },
        Record::Param { name: "ClearancePlane".into(), value: json!(25.0) },
        Record::Param { name: "RetractPlane".into(), value: json!(15.0) },
        Record::Param { name: "WcsYaw".into(), value: json!(90.0) },
        Record::Tool { number: 1, name: "EM".into() },
        Record::Section { begin: true },
        mv(MoveKind::Rapid, [10.0, 15.0, 25.0], None), // Setup (10, 15) rel to Origin (10, 5) => table (0, 10)
        mv(MoveKind::Feed, [10.0, 15.0, -1.0], Some(600.0)),
        arc([0.0, 25.0, -1.0], (0.0, 15.0), true, ArcPlane::Xy), // center (0,15) rel (10,5) => I-10. J0.
        Record::Section { begin: false },
    ];
    let stream = cnc::serialize(&recs);
    let out = engine::run(&cps("fanuc"), &mch_with_rotation(None), &stream).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(
        out.program.contains("X0. Y10."),
        "Bake emits table coordinates rel to Origin without G68:\n{}",
        out.program
    );
    assert!(
        out.program.contains("G03 X-10. Y20. Z-1. I-10. J0."),
        "Bake keeps Setup table arc coordinates directly:\n{}",
        out.program
    );
    assert!(!out.program.contains("G68"), "Bake writes no G68:\n{}", out.program);

    recs.pop();
    recs.push(Record::Cycle(CycleRec {
        kind: CycleKind::Drill,
        x: 30.0,
        y: 5.0, // rel Origin (10, 5) => table (20, 0)
        r_z: 2.0,
        bottom_z: -5.0,
        entry_z: 5.0,
        peck: 0.0,
        dwell: 0.0,
        first_move: 0,
        feed: Some(300.0),
    }));
    let cyc_stream = cnc::serialize(&recs);
    let cyc_out = engine::run(&cps("fanuc"), &mch_with_rotation(None), &cyc_stream).expect("posts");
    assert!(
        cyc_out.program.contains("G81") && cyc_out.program.contains("X20. Y0."),
        "Bake drilled hole uses table coords:\n{}",
        cyc_out.program
    );

    // Switching the exact same arc + cycle stream to Control emits G68 R90. + WCS coords (X10 Y0 / G03 X20 Y10 I0 J10 / X0 Y-20).
    let opts = json!({ "WcsRotation": "control" });
    let ctl_out = engine::run_with(&cps("fanuc"), &mch_with_rotation(Some(true)), &cyc_stream, &opts).expect("Control posts");
    assert!(ctl_out.program.contains("G68 X0. Y0. R90."), "{}", ctl_out.program);
    assert!(ctl_out.program.contains("G03 X20. Y10. Z-1. I0. J10."), "{}", ctl_out.program);
    assert!(ctl_out.program.contains("X0. Y-20."), "{}", ctl_out.program);
}

#[test]
fn control_mode_emits_g68_or_rot_only_when_machine_and_post_declare_it() {
    let stream = rotated_square_stream(30.0, true);
    let opts = json!({ "WcsRotation": "control" });
    let no_cap_err = engine::run_with(&cps("fanuc"), &mch_with_rotation(None), &stream, &opts)
        .expect_err("machine without coordinateRotation must refuse Control mode");
    assert!(
        no_cap_err.to_string().contains("coordinate"),
        "expected named refusal for missing machine capability, got: {no_cap_err}"
    );

    let unsupported_post_err =
        engine::run_with(&cps("grblhal"), &mch_with_rotation(Some(true)), &stream, &opts)
            .expect_err("post without wcsRotation must refuse Control mode");
    assert!(
        unsupported_post_err.to_string().contains("wcsRotation")
            || unsupported_post_err.to_string().contains("rotation"),
        "expected named refusal for unsupported post, got: {unsupported_post_err}"
    );

    for stem in ["fanuc", "haas", "mazak", "doosan-dnm"] {
        let out = engine::run_with(&cps(stem), &mch_with_rotation(Some(true)), &stream, &opts)
            .unwrap_or_else(|e| panic!("{stem}: {e}"));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        assert!(out.program.contains("G68 X0. Y0. R30."), "{stem}:\n{}", out.program);
        assert!(out.program.contains("G69"), "{stem}:\n{}", out.program);
        assert!(out.program.contains("X10. Y17.321"), "{stem}:\n{}", out.program);
    }

    let sinu = engine::run_with(&cps("sinumerik"), &mch_with_rotation(Some(true)), &stream, &opts)
        .expect("sinumerik posts");
    assert!(sinu.warnings.is_empty(), "{:?}", sinu.warnings);
    assert!(sinu.program.contains("ROT RPL=30"), "sinumerik:\n{}", sinu.program);
    assert!(sinu.program.contains("X10 Y17.321"), "sinumerik:\n{}", sinu.program);
    // ROT cancel block must appear before spindle stop / M30 / end.
    assert!(
        sinu.program.lines().any(|l| l.ends_with(" ROT")),
        "sinumerik must cancel with bare ROT:\n{}",
        sinu.program
    );
}

#[test]
fn mch_parser_reads_optional_controller_coordinate_rotation() {
    let base = include_str!("../../../assets/machines/doosan-dnm-3axis.mch");
    let m0 = parse_mch(base).expect("base parses");
    let mirror0 = post_machine::mirror(&m0);
    assert!(mirror0.get("coordinateRotation").is_none(), "unstated omits key: {mirror0}");

    let with_cap = base.replace(
        "\"controller\": {",
        "\"controller\": { \"coordinate_rotation\": true,",
    );
    let m1 = parse_mch(&with_cap).expect("with cap parses");
    let mirror1 = post_machine::mirror(&m1);
    assert_eq!(mirror1.get("coordinateRotation"), Some(&json!(true)), "{mirror1}");
}

#[test]
fn probing_or_non_xy_arc_under_rotated_wcs_refuses_by_name() {
    let xz_arc_stream = cnc::serialize(&[
        Record::Param { name: "WcsYaw".into(), value: json!(90.0) },
        Record::Section { begin: true },
        mv(MoveKind::Rapid, [0.0, 0.0, 10.0], None),
        arc([10.0, 0.0, 10.0], (5.0, 10.0), false, ArcPlane::Xz),
        Record::Section { begin: false },
    ]);
    let err = engine::run(&cps("fanuc"), &mch_with_rotation(None), &xz_arc_stream)
        .expect_err("XZ arc on rotated WCS must refuse");
    assert!(err.to_string().contains("arc"), "got: {err}");

    let probe_stream = cnc::serialize(&[
        Record::Param { name: "WcsYaw".into(), value: json!(90.0) },
        Record::Section { begin: true },
        Record::Touch(probe_rec::TouchRec {
            target: [10.0, 0.0, 0.0],
            expect: [5.0, 0.0, 0.0],
            feed: 200.0,
            measure: Some(0),
        }),
        Record::Section { begin: false },
    ]);
    let err = engine::run(&cps("fanuc"), &mch_with_rotation(None), &probe_stream)
        .expect_err("probing on rotated WCS must refuse");
    assert!(err.to_string().contains("probing"), "got: {err}");
}

#[test]
fn five_axis_abc_move_in_rotated_wcs_refuses_by_name() {
    let tilted_stream = cnc::serialize(&[
        Record::Param { name: "WcsYaw".into(), value: json!(45.0) },
        Record::Section { begin: true },
        Record::Move(MoveRec {
            kind: MoveKind::Feed,
            x: 10.0,
            y: 0.0,
            z: 0.0,
            feed: Some(300.0),
            center: None,
            ccw: false,
            plane: ArcPlane::Xy,
            abc: Some([15.0, 0.0, 0.0]),
        }),
        Record::Section { begin: false },
    ]);
    let err = engine::run(&cps("fanuc"), &mch_with_rotation(None), &tilted_stream)
        .expect_err("ABC move on rotated WCS must refuse");
    assert!(err.to_string().contains("rotated work offset"), "got: {err}");
}
