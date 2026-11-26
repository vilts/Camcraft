//! Provenance: clean-room tests specced from issue #1962 (Machines pane
//! validation for trunnion kinematics fields, controller.rtcp, and project
//! serialization protection).
//!
//! Gate map (M4.4): covers camcraft_app::machine::validate,
//! camcraft_app::machine, camcraft_app::project.

use camcraft_app::machine::{self, validate::validate_text};
use camcraft_app::post::machine as post_machine;
use camcraft_app::project;
use camcraft_app::world::new_world;
use serde_json::json;

fn valid_trunnion_json() -> serde_json::Value {
    json!({
        "general": { "model": "XYZAC TRUNNION", "vendor": "Camcraft", "kind": "mill" },
        "kinematics": {
            "default": {
                "units": { "length": "mm", "angle": "degrees" },
                "sign_convention": "table_rh",
                "parts": [
                    {
                        "id": "base", "name": "table", "type": "table",
                        "parts": [
                            {
                                "id": "a", "name": "A", "type": "rotary",
                                "axis": [1.0, 0.0, 0.0],
                                "pivot": [0.0, 12.5, -45.0],
                                "travel": { "min": -110.0, "max": 30.0 },
                                "zero_offset": 0.0,
                                "rollover": false,
                                "parts": [
                                    {
                                        "id": "c", "name": "C", "type": "rotary",
                                        "axis": [0.0, 0.0, 1.0],
                                        "offset": [0.0, 0.0, 20.0],
                                        "travel": { "min": -360.0, "max": 360.0 },
                                        "zero_offset": 45.0,
                                        "rollover": true
                                    }
                                ]
                            },
                            {
                                "id": "x", "name": "X", "type": "linear",
                                "travel": { "min": -200.0, "max": 200.0 },
                                "parts": [
                                    { "id": "y", "name": "Y", "type": "linear", "travel": { "min": -150.0, "max": 150.0 } }
                                ]
                            },
                            {
                                "id": "z", "name": "Z", "type": "linear",
                                "travel": { "min": -200.0, "max": 0.0 },
                                "safe_retract": 0.0,
                                "parts": [
                                    {
                                        "id": "head", "name": "head", "type": "head",
                                        "spindle": { "min_speed": 1000.0, "max_speed": 18000.0 }
                                    }
                                ]
                            }
                        ]
                    }
                ]
            }
        },
        "controller": {
            "rtcp": false,
            "synced_configuration": {
                "parts": {
                    "X": { "coordinate": 0, "max_normal_speed": 4000.0, "max_rapid_speed": 8000.0 },
                    "Y": { "coordinate": 1, "max_normal_speed": 4000.0, "max_rapid_speed": 8000.0 },
                    "Z": { "coordinate": 2, "max_normal_speed": 3000.0, "max_rapid_speed": 6000.0 },
                    "C": { "coordinate": 3, "max_normal_speed": 2000.0, "max_rapid_speed": 4000.0 },
                    "A": { "coordinate": 4, "max_normal_speed": 1500.0, "max_rapid_speed": 3000.0 }
                }
            }
        },
        "tooling": { "number_of_tools": 12 }
    })
}

#[test]
fn validator_accepts_ac_trunnion_and_refuses_bad_fields_by_name() {
    let valid = valid_trunnion_json();
    assert_eq!(validate_text(&valid.to_string()), vec![]);

    let mut bad_pivot = valid.clone();
    bad_pivot["kinematics"]["default"]["parts"][0]["parts"][0]["pivot"] = json!([1.0, "y", 3.0]);
    let refs = validate_text(&bad_pivot.to_string());
    assert!(
        refs.iter().any(|r| r.key == "A pivot"),
        "expected named `A pivot` refusal, got {refs:?}"
    );

    let mut bad_offset = valid.clone();
    bad_offset["kinematics"]["default"]["parts"][0]["parts"][0]["parts"][0]["offset"] = json!([1.0, 2.0]);
    let refs = validate_text(&bad_offset.to_string());
    assert!(
        refs.iter().any(|r| r.key == "C offset"),
        "expected named `C offset` refusal, got {refs:?}"
    );

    let mut bad_zero = valid.clone();
    bad_zero["kinematics"]["default"]["parts"][0]["parts"][0]["zero_offset"] = json!("zero");
    let refs = validate_text(&bad_zero.to_string());
    assert!(
        refs.iter().any(|r| r.key == "A zero_offset"),
        "expected named `A zero_offset` refusal, got {refs:?}"
    );

    let mut bad_rollover = valid.clone();
    bad_rollover["kinematics"]["default"]["parts"][0]["parts"][0]["parts"][0]["rollover"] = json!(1);
    let refs = validate_text(&bad_rollover.to_string());
    assert!(
        refs.iter().any(|r| r.key == "C rollover"),
        "expected named `C rollover` refusal, got {refs:?}"
    );

    let mut bad_rtcp = valid.clone();
    bad_rtcp["controller"]["rtcp"] = json!("no");
    let refs = validate_text(&bad_rtcp.to_string());
    assert!(
        refs.iter().any(|r| r.key == "controller.rtcp"),
        "expected named `controller.rtcp` refusal, got {refs:?}"
    );

    let mut bad_conv = valid.clone();
    bad_conv["kinematics"]["default"]["sign_convention"] = json!("left_rh");
    let refs = validate_text(&bad_conv.to_string());
    assert!(
        refs.iter().any(|r| r.key == "kinematics.default.sign_convention"),
        "expected named `kinematics.default.sign_convention` refusal, got {refs:?}"
    );

    let mut linear_pivot = valid.clone();
    linear_pivot["kinematics"]["default"]["parts"][0]["parts"][1]["pivot"] = json!([0.0, 0.0, 0.0]);
    let refs = validate_text(&linear_pivot.to_string());
    assert!(
        refs.iter().any(|r| r.key == "X pivot"),
        "expected named `X pivot` refusal, got {refs:?}"
    );

    let mut conflict_conv = valid;
    conflict_conv["controller"]["sign_convention"] = json!("tool_rh");
    let refs = validate_text(&conflict_conv.to_string());
    assert!(
        refs.iter().any(|r| r.key == "controller.sign_convention"),
        "expected named `controller.sign_convention` refusal, got {refs:?}"
    );
}

#[test]
fn project_roundtrips_non_rtcp_trunnion_and_protects_version() {
    let m = machine::parse_mch(&valid_trunnion_json().to_string()).expect("parses");
    let world = new_world();
    machine::set(&world, "trunnion.mch", Some(&m));

    let path = std::env::temp_dir().join(format!("camcraft_trunnion_1962_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    const {
        assert!(
            project::PROJECT_VERSION >= 66,
            "PROJECT_VERSION must bump past v65 so older builds do not discard controller_rtcp"
        );
    }
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "saved project writes current PROJECT_VERSION"
    );

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let _ = std::fs::remove_file(&path);

    let (_, back) = machine::of(&world2);
    let back = back.expect("machine reloaded");
    assert_eq!(back, m, "full trunnion machine must round-trip cleanly");

    let props = post_machine::script_props(&post_machine::mirror(&back));
    assert_eq!(props["rtcp"], json!(false));
    assert_eq!(props["signConvention"], json!("table_rh"));
    assert_eq!(props["rotaries"][0]["name"], json!("A"));
    assert_eq!(props["rotaries"][1]["name"], json!("C"));
    assert_eq!(props["rotaries"][1]["parent"], json!("A"));
    assert_eq!(props["rotaries"][0]["pivot"], json!([0.0, 12.5, -45.0]));
    assert_eq!(props["rotaries"][1]["offset"], json!([0.0, 0.0, 20.0]));
    assert_eq!(props["rotaries"][1]["zeroOffset"], json!(45.0));
    assert_eq!(props["rotaries"][1]["rollover"], json!(true));
}

#[test]
fn a_v66_trunnion_steps_up_without_losing_its_stated_rtcp_and_kinematics() {
    let expected = machine::parse_mch(&valid_trunnion_json().to_string()).expect("trunnion parses");
    let source = new_world();
    machine::set(&source, "trunnion.mch", Some(&expected));
    let mut file: serde_json::Value =
        serde_json::from_str(&project::save_text(&source).unwrap()).unwrap();
    file["version"] = json!(66);
    assert_eq!(file["machine"]["machine"]["controller_rtcp"], json!(false));

    let loaded = new_world();
    project::open(&loaded, &file.to_string()).expect("v66 trunnion loads");
    assert_eq!(machine::of(&loaded).1.as_ref(), Some(&expected));
    let saved = project::save_text(&loaded).expect("save migrated trunnion");
    let actual: serde_json::Value = serde_json::from_str(&saved).unwrap();
    file["version"] = json!(project::PROJECT_VERSION);
    assert_eq!(actual, file, "only the format version changes; all stated trunnion fields survive");

    let reloaded = new_world();
    project::open(&reloaded, &saved).expect("current-format trunnion reloads");
    let (_, back) = machine::of(&reloaded);
    let back = back.expect("reloaded machine");
    assert_eq!(back, expected, "RTCP, chain, pivots, offsets, zeros, rollover and convention survive");
    let props = post_machine::script_props(&post_machine::mirror(&back));
    assert_eq!(props["rtcp"], json!(false), "explicit controller refusal survives into post properties");
    assert_eq!(project::save_text(&reloaded).unwrap(), saved, "migrated file is byte-stable");
}
