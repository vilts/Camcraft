//! Provenance: clean-room tests specced from issue #1962 (XYZAC trunnion
//! `.mch` schema, parent-child rotary order, unit scaling, explicit sign
//! convention, and `controller.rtcp` guard on `fanuc.cps` / `doosan-dnm.cps`).
//!
//! Gate map (M4.4): covers camcraft_post::machine::def, camcraft_post::machine::parse,
//! camcraft_post::machine::trunnion, camcraft_post::post::machine,
//! assets/posts/fanuc.cps, assets/posts/doosan-dnm.cps.

use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine};
use serde_json::json;
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn ac_trunnion_mch(
    unit: &str,
    rtcp: Option<bool>,
    sign_conv: Option<&str>,
    c_coord: i64,
    a_coord: i64,
) -> String {
    let mut controller = json!({
        "synced_configuration": {
            "parts": {
                "X": { "coordinate": 0, "max_normal_speed": 4000.0, "max_rapid_speed": 8000.0 },
                "Y": { "coordinate": 1, "max_normal_speed": 4000.0, "max_rapid_speed": 8000.0 },
                "Z": { "coordinate": 2, "max_normal_speed": 3000.0, "max_rapid_speed": 6000.0 },
                "C": { "coordinate": c_coord, "max_normal_speed": 2000.0, "max_rapid_speed": 4000.0 },
                "A": { "coordinate": a_coord, "max_normal_speed": 1500.0, "max_rapid_speed": 3000.0 }
            }
        }
    });
    if let Some(flag) = rtcp {
        controller["rtcp"] = json!(flag);
    }
    let mut kine_default = json!({
        "units": { "length": unit, "angle": "degrees" },
        "parts": [
            {
                "id": "base", "name": "table", "type": "table",
                "parts": [
                    {
                        "id": "a_axis", "name": "A", "type": "rotary",
                        "axis": [1.0, 0.0, 0.0],
                        "pivot": [1.0, -2.0, 3.0],
                        "travel": { "min": -110.0, "max": 30.0 },
                        "zero_offset": 15.0,
                        "rollover": false,
                        "parts": [
                            {
                                "id": "c_axis", "name": "C", "type": "rotary",
                                "axis": [0.0, 0.0, 1.0],
                                "offset": [0.0, 0.5, 2.0],
                                "travel": { "min": -360.0, "max": 360.0 },
                                "zero_offset": -90.0,
                                "rollover": true
                            }
                        ]
                    },
                    {
                        "id": "x", "name": "X", "type": "linear",
                        "travel": { "min": -200.0, "max": 200.0 },
                        "parts": [
                            {
                                "id": "y", "name": "Y", "type": "linear",
                                "travel": { "min": -150.0, "max": 150.0 }
                            }
                        ]
                    },
                    {
                        "id": "z", "name": "Z", "type": "linear",
                        "travel": { "min": -200.0, "max": 0.0 },
                        "safe_retract": 0.0,
                        "parts": [
                            {
                                "id": "head", "name": "head", "type": "head",
                                "spindle": { "min_speed": 1000.0, "max_speed": 18000.0 },
                                "tool_station": { "coolants": [{ "name": "Flood", "mCode": "M8" }] }
                            }
                        ]
                    }
                ]
            }
        ]
    });
    if let Some(sc) = sign_conv {
        kine_default["sign_convention"] = json!(sc);
    }
    json!({
        "general": { "model": "XYZAC TRUNNION", "vendor": "Camcraft", "kind": "mill" },
        "kinematics": { "default": kine_default },
        "controller": controller,
        "tooling": { "number_of_tools": 12 }
    })
    .to_string()
}

#[test]
fn explicit_non_rtcp_trunnion_does_not_promise_controller_compensation() {
    let mut raw: serde_json::Value =
        serde_json::from_str(&ac_trunnion_mch("mm", Some(false), Some("table_rh"), 3, 4))
            .expect("valid fixture");
    raw["controller"]["coordinate_rotation"] = json!(true);
    let raw_text = raw.to_string();
    let m = parse_mch(&raw_text).expect("non-RTCP trunnion parses");
    let mirrored = machine::mirror(&m);
    assert_eq!(mirrored["coordinateRotation"], json!(true));
    assert_eq!(mirrored["rtcp"], json!(false));
    let props = machine::script_props(&mirrored);
    assert_eq!(
        props["rtcp"],
        json!(false),
        "controller.rtcp=false must keep machine.rtcp false even with tilting A"
    );

    let raw_json: serde_json::Value = serde_json::from_str(&raw_text).expect("valid json");
    let raw_props = machine::script_props(&raw_json);
    assert_eq!(
        raw_props["rtcp"],
        json!(false),
        "raw script_props must also honor controller.rtcp=false"
    );

    let tilted_ac = "\
PARAM ClearancePlane 20
TOOL 1 T1
PARAM SpindleSpeed 6000
SECTION
Rapid X 0 Y 0 Z 20000 A 30000 B 0 C 45000
Feed X 10000 Y 0 Z 0 F 300000 A 30000 B 0 C 45000
ENDSECTION
";
    let tilted_a_only = "\
PARAM ClearancePlane 20
TOOL 1 T1
PARAM SpindleSpeed 6000
SECTION
Rapid X 0 Y 0 Z 20000 A 30000 B 0 C 0
Feed X 10000 Y 0 Z 0 F 300000 A 30000 B 0 C 0
ENDSECTION
";

    let fanuc = read("assets/posts/fanuc.cps");
    let out_fanuc = engine::run(&fanuc, &mirrored, tilted_ac).expect("fanuc runs");
    assert!(
        !out_fanuc.warnings.is_empty(),
        "fanuc.cps must refuse tilted stream when controller.rtcp is false"
    );
    assert!(
        !out_fanuc.program.contains("G43.4"),
        "fanuc.cps must not emit G43.4 when controller.rtcp is false:\n{}",
        out_fanuc.program
    );

    let doosan = read("assets/posts/doosan-dnm.cps");
    let out_doosan = engine::run(&doosan, &mirrored, tilted_ac).expect("doosan runs");
    assert!(
        !out_doosan.warnings.is_empty(),
        "doosan-dnm.cps must refuse tilted stream when controller.rtcp is false"
    );
    assert!(
        !out_doosan.program.contains("G43.4"),
        "doosan-dnm.cps must not emit G43.4 when controller.rtcp is false:\n{}",
        out_doosan.program
    );

    let brake = json!({ "rotaryBrake": true });
    let out_doosan_brake =
        engine::run_with(&doosan, &mirrored, tilted_a_only, &brake).expect("doosan runs with brake");
    assert!(
        !out_doosan_brake.warnings.is_empty(),
        "doosan-dnm.cps must refuse non-RTCP multi-axis trunnion even with rotaryBrake:\n{}",
        out_doosan_brake.program
    );
    assert!(
        !out_doosan_brake.program.contains("G43.4"),
        "doosan-dnm.cps must not emit G43.4:\n{}",
        out_doosan_brake.program
    );
}

#[test]
fn rotary_chain_order_preserved_when_controller_coordinates_differ() {
    // Controller lists C (coord 3) before A (coord 4), while kinematics nests C inside A.
    let m = parse_mch(&ac_trunnion_mch("mm", Some(false), Some("table_rh"), 3, 4))
        .expect("trunnion parses");
    let axis_names: Vec<&str> = m.axes.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        axis_names,
        vec!["X", "Y", "Z", "C", "A"],
        "existing ctrl_coord sort on Machine.axes must stay intact"
    );

    let mirrored = machine::mirror(&m);
    let rots = mirrored["kinematics"]["static"]["rotaries"]
        .as_array()
        .expect("rotaries array");
    assert_eq!(rots.len(), 2);
    assert_eq!(rots[0]["letter"], "A", "parent A must lead rotary chain");
    assert_eq!(rots[1]["letter"], "C", "child C must follow parent A");
    assert_eq!(rots[1]["parent"], "A", "child C records its parent A");

    let props = machine::script_props(&mirrored);
    let prop_rots = props["rotaries"].as_array().expect("script rotaries");
    assert_eq!(prop_rots[0]["name"], "A");
    assert_eq!(prop_rots[1]["name"], "C");
    assert_eq!(prop_rots[1]["parent"], "A");
}

#[test]
fn trunnion_fields_parse_and_mirror_with_unit_scaling() {
    let m = parse_mch(&ac_trunnion_mch("inch", Some(false), Some("table_rh"), 3, 4))
        .expect("inch trunnion parses");
    let mirrored = machine::mirror(&m);
    let rots = mirrored["kinematics"]["static"]["rotaries"]
        .as_array()
        .expect("rotaries");

    // A: pivot [1, -2, 3] in -> [25.4, -50.8, 76.2] mm; zero_offset stays 15 deg.
    assert_eq!(rots[0]["pivot"], json!([1.0 * 25.4, -2.0 * 25.4, 3.0 * 25.4]));
    assert_eq!(rots[0]["zeroOffset"], json!(15.0));
    assert_eq!(rots[0]["rollover"], json!(false));
    assert_eq!(rots[0]["signConvention"], json!("table_rh"));

    // C: offset [0, 0.5, 2] in -> [0, 12.7, 50.8] mm; zero_offset stays -90 deg.
    assert_eq!(rots[1]["offset"], json!([0.0, 0.5 * 25.4, 2.0 * 25.4]));
    assert_eq!(rots[1]["zeroOffset"], json!(-90.0));
    assert_eq!(rots[1]["rollover"], json!(true));
    assert_eq!(rots[1]["signConvention"], json!("table_rh"));

    let props = machine::script_props(&mirrored);
    assert_eq!(props["signConvention"], json!("table_rh"));
    assert_eq!(props["rotaries"][0]["pivot"], json!([1.0 * 25.4, -2.0 * 25.4, 3.0 * 25.4]));
    assert_eq!(props["rotaries"][0]["zeroOffset"], json!(15.0));
    assert_eq!(props["rotaries"][0]["rollover"], json!(false));
    assert_eq!(props["rotaries"][1]["offset"], json!([0.0, 0.5 * 25.4, 2.0 * 25.4]));
    assert_eq!(props["rotaries"][1]["zeroOffset"], json!(-90.0));
    assert_eq!(props["rotaries"][1]["rollover"], json!(true));
    assert_eq!(props["rotaries"][1]["signConvention"], json!("table_rh"));

    // Unstated sign_convention resolves explicitly to tool_rh (ISO 841), while
    // unstated pivot/offset/zero_offset/rollover remain absent in mirror.
    let tilthead = parse_mch(&read("assets/machines/tilthead.mch")).expect("tilthead parses");
    let tilt_mirror = machine::mirror(&tilthead);
    let tilt_rot = &tilt_mirror["kinematics"]["static"]["rotaries"][0];
    assert!(tilt_rot.get("pivot").is_none());
    assert!(tilt_rot.get("offset").is_none());
    assert!(tilt_rot.get("zeroOffset").is_none());
    assert!(tilt_rot.get("rollover").is_none());
    let tilt_props = machine::script_props(&tilt_mirror);
    assert_eq!(tilt_props["rtcp"], json!(true));
    assert_eq!(tilt_props["signConvention"], json!("tool_rh"));
    assert_eq!(tilt_props["rotaries"][0]["signConvention"], json!("tool_rh"));

    // Explicit [0,0,0] pivot/offset and 0.0 zero_offset are valid statements (not dropped),
    // and per-rotary sign_convention overrides machine default.
    let zero_and_override = json!({
        "general": { "model": "ZERO-PIVOT AC", "vendor": "Camcraft", "kind": "mill" },
        "kinematics": { "default": {
            "units": { "length": "mm", "angle": "degrees" },
            "sign_convention": "table_rh",
            "parts": [
                { "name": "A", "type": "rotary", "axis": [1.0, 0.0, 0.0], "pivot": [0.0, 0.0, 0.0], "zero_offset": 0.0,
                  "parts": [{ "name": "C", "type": "rotary", "axis": [0.0, 0.0, 1.0], "offset": [0.0, 0.0, 0.0], "sign_convention": "tool_rh" }] }
            ]
        }},
        "controller": { "rtcp": true }
    });
    let zm = parse_mch(&zero_and_override.to_string()).expect("zero pivot parses");
    assert_eq!(zm.rotary_axes().iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), vec!["A", "C"]);
    assert_eq!(zm.effective_sign_convention(Some(zm.rotary_axes()[0])).word(), "table_rh");
    assert_eq!(zm.effective_sign_convention(Some(zm.rotary_axes()[1])).word(), "tool_rh");
    let zp = machine::script_props(&machine::mirror(&zm));
    assert_eq!(zp["rtcp"], json!(true));
    assert_eq!(zp["rotaries"][0]["pivot"], json!([0.0, 0.0, 0.0]));
    assert_eq!(zp["rotaries"][0]["zeroOffset"], json!(0.0));
    assert_eq!(zp["rotaries"][0]["signConvention"], json!("table_rh"));
    assert_eq!(zp["rotaries"][1]["offset"], json!([0.0, 0.0, 0.0]));
    assert_eq!(zp["rotaries"][1]["signConvention"], json!("tool_rh"));
}

#[test]
fn invalid_trunnion_fields_refuse_by_name() {
    let cases: &[(&str, serde_json::Value, &str)] = &[
        (
            "bad pivot len",
            json!({ "kinematics": { "default": { "parts": [
                { "name": "A", "type": "rotary", "pivot": [1.0, 2.0] }
            ] } } }),
            "A",
        ),
        (
            "bad pivot element",
            json!({ "kinematics": { "default": { "parts": [
                { "name": "A", "type": "rotary", "pivot": [1.0, "bad", 3.0] }
            ] } } }),
            "pivot",
        ),
        (
            "pivot on linear",
            json!({ "kinematics": { "default": { "parts": [
                { "name": "X", "type": "linear", "pivot": [0.0, 0.0, 0.0] }
            ] } } }),
            "X",
        ),
        (
            "bad offset",
            json!({ "kinematics": { "default": { "parts": [
                { "name": "C", "type": "rotary", "offset": 5.0 }
            ] } } }),
            "offset",
        ),
        (
            "offset on linear",
            json!({ "kinematics": { "default": { "parts": [
                { "name": "Y", "type": "linear", "offset": [0.0, 0.0, 1.0] }
            ] } } }),
            "Y",
        ),
        (
            "inch pivot overflow",
            json!({ "kinematics": { "default": {
                "units": { "length": "inch" },
                "parts": [{ "name": "A", "type": "rotary", "pivot": [1e308, 0.0, 0.0] }]
            } } }),
            "pivot",
        ),
        (
            "bad zero_offset",
            json!({ "kinematics": { "default": { "parts": [
                { "name": "C", "type": "rotary", "zero_offset": "90" }
            ] } } }),
            "zero_offset",
        ),
        (
            "bad rollover",
            json!({ "kinematics": { "default": { "parts": [
                { "name": "C", "type": "rotary", "rollover": "true" }
            ] } } }),
            "rollover",
        ),
        (
            "bad sign_convention",
            json!({ "kinematics": { "default": { "sign_convention": "left_hand", "parts": [] } } }),
            "sign_convention",
        ),
        (
            "bad controller.rtcp",
            json!({ "controller": { "rtcp": "false" }, "kinematics": { "default": { "parts": [] } } }),
            "rtcp",
        ),
        (
            "disagreeing sign_convention",
            json!({
                "controller": { "sign_convention": "tool_rh" },
                "kinematics": { "default": { "sign_convention": "table_rh", "parts": [] } }
            }),
            "sign_convention",
        ),
    ];
    for (label, doc, needle) in cases {
        let err = parse_mch(&doc.to_string())
            .err()
            .unwrap_or_else(|| panic!("{label}: expected refusal"));
        assert!(
            err.contains(needle),
            "{label}: refusal {err:?} should contain {needle:?}"
        );
    }
}
