//! Exercise the standalone post on one control release. The independent coordinates below
//! follow the iTNC 530 Klartext manual for NC software 34049x-03 (533 190-22).
//! Gate map: covers assets/posts/heidenhain-klartext.cps, post::machine, post::engine.

#[macro_use]
mod recorded;

use camcraft_post::{
    machine::parse_mch,
    post::{engine, machine},
};
use serde_json::Value;

const SCRIPT: &str = "heidenhain-klartext.cps";
const STREAM: &str = include_str!("klartext/contour.cnc");
const MCH: &str = include_str!("../../../assets/machines/fehlmann-picomax55.mch");

post_suite!("heidenhain-klartext.cps");

fn post(mch: &str, stream: &str, options: &Value) -> Result<String, String> {
    let parsed = parse_mch(mch).map_err(|e| e.to_string())?;
    let script =
        std::fs::read_to_string(recorded::repo().join("assets/posts").join(SCRIPT)).unwrap();
    let out = engine::run_with(&script, &machine::mirror(&parsed), stream, options)
        .map_err(|e| e.to_string())?;
    if out.warnings.is_empty() {
        Ok(out.program)
    } else {
        Err(out.warnings.join("\n"))
    }
}

/// Read one signed axis value from the emitted text. The origin is (100,25,5)
/// in the fixture; all of these expected values are in the selected preset.
fn axis(block: &str, letter: char) -> f64 {
    block
        .split_whitespace()
        .find_map(|w| w.strip_prefix(letter)?.parse().ok())
        .unwrap_or_else(|| panic!("no {letter} in {block}"))
}

#[test]
fn qualified_cli_program_has_correct_blank_arcs_feed_and_tool_change_order() {
    let input = recorded::repo().join("crates/post/tests/klartext/contour.cnc");
    let machine = recorded::repo().join("assets/machines/fehlmann-picomax55.mch");
    let (verdict, nc) = recorded::verdict_on(SCRIPT, "picomax", &input, Some(&machine));
    assert_eq!(verdict, "nc", "{nc}");
    let blocks: Vec<&str> = nc
        .lines()
        .map(|line| line.split_once(' ').unwrap().1)
        .collect();
    assert!(blocks[0].starts_with("BEGIN PGM "), "{nc}");
    assert_eq!(
        blocks.last().unwrap().strip_prefix("END PGM "),
        blocks[0].strip_prefix("BEGIN PGM ")
    );
    let find = |prefix: &str| {
        *blocks
            .iter()
            .find(|line| line.starts_with(prefix))
            .unwrap_or_else(|| panic!("{prefix}: {nc}"))
    };
    let lo = find("BLK FORM 0.1");
    let hi = find("BLK FORM 0.2");
    assert_eq!(
        [axis(lo, 'X'), axis(lo, 'Y'), axis(lo, 'Z')],
        [-20., -15., -2.]
    );
    assert_eq!(
        [axis(hi, 'X'), axis(hi, 'Y'), axis(hi, 'Z')],
        [20., 15., 5.]
    );
    assert!(
        nc.contains("CYCL DEF 247") && (nc.contains("Q339=+2") || nc.contains("Q339=2")),
        "{nc}"
    );
    let arcs: Vec<_> = blocks
        .iter()
        .filter(|s| s.starts_with("C "))
        .copied()
        .collect();
    assert_eq!(arcs.len(), 2, "{nc}");
    for (block, end, direction) in [(arcs[0], [0., 10.], "DR+"), (arcs[1], [10., 0.], "DR-")] {
        assert_eq!([axis(block, 'X'), axis(block, 'Y')], end);
        assert!(block.split_whitespace().any(|w| w == direction), "{block}");
    }
    let centre = find("CC ");
    assert_eq!([axis(centre, 'X'), axis(centre, 'Y')], [0., 0.]);
    assert!(
        blocks
            .iter()
            .any(|b| b.starts_with("L ") && b.contains("F220") && axis(b, 'X') == 15.),
        "{nc}"
    );
    let start_stop = blocks
        .iter()
        .position(|b| b.split_whitespace().any(|w| w == "M0"))
        .expect("setup stop");
    let retract = blocks
        .iter()
        .position(|b| b.contains("M140 MB MAX"))
        .expect("full retract");
    let tool = blocks
        .iter()
        .position(|b| b.starts_with("TOOL CALL 2 "))
        .expect("tool call");
    let first_xy = blocks
        .iter()
        .position(|b| b.starts_with("L ") && b.split_whitespace().any(|w| w.starts_with('X')))
        .expect("first traverse");
    assert!(
        start_stop < tool && tool < retract && retract < first_xy,
        "{nc}"
    );
    assert!(
        blocks[tool + 1..first_xy]
            .iter()
            .any(|b| b.contains("M140 MB MAX")),
        "retract established with current tool: {nc}"
    );
    assert!(
        blocks[tool + 1..first_xy]
            .iter()
            .all(|b| !b.starts_with("L Z")),
        "descent at unverified XY: {nc}"
    );
    assert!(
        blocks[start_stop + 1..tool]
            .iter()
            .any(|b| b.split_whitespace().any(|w| w == "M0")),
        "tool load stop: {nc}"
    );
    assert!(
        blocks[retract + 1..first_xy]
            .iter()
            .any(|b| b.split_whitespace().any(|w| w == "M0")),
        "confirm the reached height before traversing: {nc}"
    );
    recorded::compare(SCRIPT, "picomax", "nc", &nc);
}

#[test]
fn each_tool_swap_gets_its_own_stop_and_retract() {
    let second = STREAM
        .replace("TOOL 2 FLAT-6", "TOOL 3 FLAT-6")
        .replace("Arc contour", "Second contour");
    let nc = post(MCH, &format!("{STREAM}{second}"), &Value::Null).expect("two operations");
    let (_, after_first) = nc.split_once("TOOL CALL 2 ").expect("initial tool");
    let (before_second, _) = after_first
        .split_once("TOOL CALL 3 ")
        .expect("replacement tool");
    let (_, after_retract) = before_second
        .rsplit_once("M140 MB MAX")
        .expect("second change retract");
    assert!(after_retract.split_whitespace().any(|w| w == "M0"), "{nc}");
}

#[test]
fn an_unknown_software_or_unconfirmed_change_mode_cannot_inherit_the_profile() {
    for (field, value) in [
        ("/controller/identity/nc_software", "340490-04"),
        ("/controller/identity/model", "TNC 640"),
        ("/general/model", "Another XYZ mill"),
        ("/tooling/tool_change", "automatic"),
    ] {
        let mut machine: Value = serde_json::from_str(MCH).unwrap();
        *machine.pointer_mut(field).unwrap() = Value::from(value);
        assert!(
            post(&machine.to_string(), STREAM, &Value::Null).is_err(),
            "{field} = {value}"
        );
    }
}

#[test]
fn unsupported_motion_or_missing_setup_data_is_refused() {
    let unsafe_streams = [
        STREAM.replace("PARAM StockZBottom 3\n", ""),
        STREAM.replace("PARAM ClearancePlane 15", "PARAM ClearancePlane 10"),
        STREAM.replace("PARAM StartZ 15", "PARAM StartZ 9"),
        STREAM.replace("TOOL 2 FLAT-6\n", ""),
        STREAM.replace("PARAM ToolDiameter 6\n", ""),
        STREAM.replace("PARAM SpindleSpeed 3500", "PARAM SpindleSpeed 0"),
        STREAM.replace("PARAM SpindleSpeed 3500", "PARAM SpindleSpeed 9001"),
        STREAM.replace("F 220000", "F 100000000"),
        STREAM.replace("Plunge X", "PROBE 10\nPlunge X"),
        STREAM.replace("9000 F 120000", "9000 F 120000 A 0 B 0 C 0"),
        format!("{STREAM}{}", STREAM.replace("PARAM WorkOffset 2", "PARAM WorkOffset 3")),
        STREAM.replace("Z 9000 F 350000 I", "Z 8000 F 350000 I"),
        STREAM.replace("I 100000 J 25000", "I 100000 K 9000"),
        STREAM.replace("Plunge X", "COMP LEFT 3000\nPlunge X"),
        STREAM.replace(
            "Plunge X",
            "CYCLE TAP X 110000 Y 25000 Z 9000 R 15000 F 120000\nPlunge X",
        ),
        STREAM.replace("PARAM Coolant off", "PARAM Coolant mist"),
        STREAM.replace("Rapid X 115000 Y 25000 Z 15000\n", ""),
        STREAM.replace(
            "PARAM Coolant off",
            "PARAM Smoothing 0.01\nPARAM Coolant off",
        ),
    ];
    for stream in unsafe_streams {
        assert!(
            post(MCH, &stream, &Value::Null).is_err(),
            "unexpectedly posted:\n{stream}"
        );
    }
    assert!(post(MCH, STREAM, &serde_json::json!({"Units":"inch"})).is_err());
}

#[test]
fn an_unconfirmed_release_preserves_an_existing_cli_output() {
    let temp = std::env::temp_dir();
    let nc = temp.join(format!("camcraft-klartext-kept-{}.H", std::process::id()));
    let mch = temp.join(format!(
        "camcraft-klartext-other-{}.mch",
        std::process::id()
    ));
    std::fs::write(&nc, "old program").unwrap();
    std::fs::write(&mch, MCH.replace("340490-03", "340490-04")).unwrap();
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_post"))
        .arg("--machine")
        .arg(&mch)
        .arg(recorded::repo().join("assets/posts").join(SCRIPT))
        .arg(recorded::repo().join("crates/post/tests/klartext/contour.cnc"))
        .arg(&nc)
        .output()
        .unwrap();
    let saved = std::fs::read_to_string(&nc).unwrap();
    std::fs::remove_file(nc).unwrap();
    std::fs::remove_file(mch).unwrap();
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("controller profile"));
    assert_eq!(saved, "old program");
}

fn assert_cli_keeps_destination(case: &str, stream: &str, diagnostic: &str) {
    let dir = std::env::temp_dir().join(format!("camcraft-klartext-{case}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("job.cnc");
    let nc = dir.join("job.H");
    std::fs::write(&input, stream).unwrap();
    std::fs::write(&nc, "old program").unwrap();
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_post"))
        .arg("--machine")
        .arg(recorded::repo().join("assets/machines/fehlmann-picomax55.mch"))
        .arg(recorded::repo().join("assets/posts").join(SCRIPT))
        .arg(&input)
        .arg(&nc)
        .output()
        .unwrap();
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains(diagnostic), "{stderr}");
    assert_eq!(std::fs::read_to_string(&nc).unwrap(), "old program");
    std::fs::remove_file(nc).unwrap();
    std::fs::remove_file(input).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn an_engine_refusal_after_a_valid_preset_preserves_the_cli_destination() {
    // The engine rejects the second index before onParameter sees it.
    let stream = STREAM.replace("PARAM WorkOffset 2", "PARAM WorkOffset 2\nPARAM WorkOffset 7");
    assert_cli_keeps_destination("work-offset", &stream, "work offset 7");
}

#[test]
fn conflicting_definitions_of_one_tool_preserve_the_cli_destination() {
    // These tool names would look identical after sanitizing them for an NC comment.
    let first = STREAM.replace("FLAT-6", "FLAT/6");
    let other_slot = first.replace("TOOL 2", "TOOL 3");
    for (field, changed) in [
        ("name", first.replace("FLAT/6", "FLAT?6")),
        ("kind", first.replace("Flat end mill", "ball-nose")),
        ("diameter", first.replace("ToolDiameter 6", "ToolDiameter 12")),
        ("corner", first.replace("ToolDiameter 6", "ToolDiameter 6\nPARAM ToolCornerRadius 0.5")),
    ] {
        for (order, middle) in [("adjacent", ""), ("returned", other_slot.as_str())] {
            assert_cli_keeps_destination(
                &format!("{field}-{order}"),
                &format!("{first}{middle}{changed}"),
                "conflicting definitions for tool T2",
            );
        }
    }
}

#[test]
fn milling_cutter_kinds_are_explicit_and_incompatible_tools_refuse_the_cli_save() {
    for kind in ["flat end mill", "ball-nose", "bull-nose", "chamfer mill"] {
        let stream = STREAM.replace("Flat end mill", kind);
        assert!(post(MCH, &stream, &Value::Null).is_ok(), "{kind}");
    }
    for kind in ["wire", "turning insert", "tap", "touch probe"] {
        let stream = STREAM.replace("Flat end mill", kind);
        assert_cli_keeps_destination("non-mill", &stream, "unsupported milling tool kind");
    }
}

#[test]
fn precision_notes_preserve_the_full_bound_as_klartext_comments() {
    let machine = machine::mirror(&parse_mch(MCH).unwrap());
    let script = std::fs::read_to_string(recorded::repo().join("assets/posts").join(SCRIPT)).unwrap();
    let id = camcraft_post::post::ProgramId {
        notes: vec!["Reduced accuracy: model Test;\nM2 ~ was checked to 0.024 mm, not 0.010 mm".into()],
        ..Default::default()
    };
    let out = engine::run_program_with(&script, &machine, STREAM, &Value::Null, &id).unwrap();
    let text = out.program.lines().skip(1)
        .take_while(|line| line.split_once(' ').is_some_and(|(_, body)| body.starts_with("; ")))
        .map(|line| line.split_once("; ").unwrap().1)
        .collect::<Vec<_>>().join(" ");
    assert_eq!(text, "Reduced accuracy: model Test M2 was checked to 0.024 mm, not 0.010 mm");
}

#[test]
fn raw_and_mirrored_machine_inputs_release_the_same_program() {
    let machine: Value = serde_json::from_str(MCH).unwrap();
    let script =
        std::fs::read_to_string(recorded::repo().join("assets/posts").join(SCRIPT)).unwrap();
    let raw = engine::run(&script, &machine, STREAM).expect("Camcraft .mch");
    assert!(raw.warnings.is_empty(), "{:?}", raw.warnings);
    assert_eq!(
        raw.program,
        post(MCH, STREAM, &Value::Null).expect("parsed and mirrored machine")
    );
}

#[test]
fn only_explicit_single_spindle_xyz_geometry_and_a_confirmed_change_can_post() {
    let mut wrong_axis: Value = serde_json::from_str(MCH).unwrap();
    wrong_axis["kinematics"]["default"]["parts"][0]["name"] = Value::from("U");
    let why = post(&wrong_axis.to_string(), STREAM, &Value::Null).expect_err("unknown X travel");
    assert!(why.contains("XYZ"), "{why}");

    let mut combined_axes: Value = serde_json::from_str(MCH).unwrap();
    let x = &mut combined_axes["kinematics"]["default"]["parts"][0];
    x["name"] = Value::from("X,Y");
    x["parts"] = serde_json::json!([{"id":"table", "name":"Table", "type":"table"}]);
    let why = post(&combined_axes.to_string(), STREAM, &Value::Null)
        .expect_err("one axis name cannot qualify two linear axes");
    assert!(why.contains("XYZ"), "{why}");

    let mut missing_mode: Value = serde_json::from_str(MCH).unwrap();
    missing_mode.as_object_mut().unwrap().remove("tooling");
    let why =
        post(&missing_mode.to_string(), STREAM, &Value::Null).expect_err("unstated tool change");
    assert!(why.contains("manual-retract-top"), "{why}");

    let machine: Value = serde_json::from_str(MCH).unwrap();
    assert_eq!(machine.pointer("/controller/post").unwrap(), "Heidenhain Klartext");
    for field in ["min_speed", "max_speed"] {
        let mut missing_limit = machine.clone();
        missing_limit["kinematics"]["default"]["parts"][1]["parts"][0]["spindle"]
            .as_object_mut().unwrap().remove(field);
        let why = post(&missing_limit.to_string(), STREAM, &Value::Null)
            .expect_err("unstated spindle limit");
        assert!(why.contains("spindle"), "{field}: {why}");
    }

    let below_minimum = STREAM.replace("PARAM SpindleSpeed 3500", "PARAM SpindleSpeed 30");
    let why = post(MCH, &below_minimum, &Value::Null).expect_err("below 50 rpm");
    assert!(why.contains("minimum"), "{why}");
}

#[test]
fn successive_sections_with_the_same_tool_recheck_transfer_without_another_exchange() {
    let nc = post(MCH, &format!("{STREAM}{STREAM}"), &Value::Null).unwrap();
    assert_eq!(nc.matches("INSTALL TOOL 2").count(), 1, "{nc}");
    let calls: Vec<_> = nc.split("TOOL CALL 2 ").skip(1).collect();
    assert_eq!(calls.len(), 2, "{nc}");
    for call in calls {
        let (_, transfer) = call.split_once("M140 MB MAX").expect("post-call retract");
        let (stationary, _) = transfer.split_once("L X").expect("first XY traverse");
        assert!(stationary.split_whitespace().any(|w| w == "M0"), "{nc}");
    }
}
