//! Gate map: covers app::posting::agie, native Wire Contour geometry and post dispatch.
//! Provenance: our synthetic rectangular punch and the documented Agie native import files.

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost,
    SetPostOptions, SetPostScriptOptions, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{
    DEFAULT_CLEARANCE, Height, OpParams, SrcRef, WireContourOp, WireSide,
};
use camcraft_app::post::options::OptionValue;
use camcraft_app::posting::{self, agie};
use camcraft_app::schema::{Location, PostOptions, PostRef};
use camcraft_app::stock::StockShape;
use flecs_ecs::prelude::World;
use std::{collections::BTreeMap, fs};

#[path = "agie_native/cutoff.rs"]
mod cutoff;
#[path = "agie_native/die.rs"]
mod die;
#[path = "agie_native/dxf.rs"]
mod dxf;
#[path = "agie_native/group.rs"]
mod group;
#[path = "agie_native/quality.rs"]
mod quality;
#[path = "agie_native/reentrant.rs"]
mod reentrant;
#[path = "agie_native/tangent.rs"]
mod tangent;

fn separation(value: f64) -> BTreeMap<String, OptionValue> {
    BTreeMap::from([("separationLengthMm".into(), OptionValue::Float(value))])
}

fn configured_profile() -> String {
    let template = posting::builtin_machine_text("AGIECUT Classic 2S (template)").unwrap();
    let mut machine: serde_json::Value = serde_json::from_str(template).unwrap();
    machine["edm"]["conditions"] = serde_json::json!([{
        "id": "AGIE_QN2", "role": "rough", "gap": 0.005, "wire_diameter": 0.25
    }]);
    machine.to_string()
}

fn punch(height: f64, datum: [f64; 3], start: [f64; 2]) -> World {
    punch_setup(height, datum, start, 60.0, &configured_profile())
}

fn punch_setup(
    height: f64,
    datum: [f64; 3],
    start: [f64; 2],
    stock_depth: f64,
    profile: &str,
) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let mch = camcraft_app::machine::parse_mch(profile).unwrap();
    h.execute(
        &world,
        LoadMachine::new(&world, "agiecut-classic-2s.mch".into(), Some(mch)).unwrap(),
    );
    h.execute(
        &world,
        SelectPost::new(&world, PostRef::Named(agie::POST.into())).unwrap(),
    );
    h.execute(
        &world,
        AddTool::new(&world, "wire", camcraft_app::wire_tools::wire_tool(0.25)).unwrap(),
    );
    let stock = StockShape::Box {
        dx: 80.0,
        dy: stock_depth,
        dz: height,
    };
    h.execute(
        &world,
        SetStock::new(&world, stock, Location::default()).unwrap(),
    );
    let part = cube(
        Point3::new(-20.0, -10.0, 0.0),
        Point3::new(20.0, 10.0, height),
    );
    h.execute(&world, ImportModel::from_mesh("punch", part));
    let pos = Point3::new(datum[0], datum[1], datum[2]);
    h.execute(
        &world,
        SetWcsOrigin::new(
            &world,
            Location {
                pos,
                ..Location::default()
            },
        )
        .unwrap(),
    );
    let wire = WireContourOp {
        lead_arc_radius: None,
        top_z: Height::fixed(height),
        bottom_z: Height::fixed(0.0),
        condition: "AGIE_QN2".into(),
        side: WireSide::Punch,
        picks: camcraft_app::path_picks::PathPicks {
            start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(start[0], start[1]))],
            exit: Vec::new(),
        },
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        tabs: None,
        taper_deg: 0.0,
        burnout: None,
        clearance: DEFAULT_CLEARANCE,
        skims: Vec::new(),
        four_axis: None,
        sharp_corners: false,
    };
    h.execute(
        &world,
        CreateOperation::new(
            &world,
            "wire1",
            "wire",
            SrcRef::Entity("punch".into()),
            OpParams::WireContour(wire),
        )
        .unwrap(),
    );
    h.execute(&world, GenerateOperation::new(&world, "wire1").unwrap());
    world
}

#[test]
fn the_mounted_agie_post_writes_nominal_geometry_and_preserves_existing_jobs() {
    let world = punch(20.0, [2.0, 3.0, 20.0], [-30.0, 0.0]);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("PUNCH40");
    // The desktop post dialog applies this persisted per-post selection.
    let post = PostRef::Named(agie::POST.into());
    let mut h = History::default();
    h.execute(
        &world,
        SetPostScriptOptions::for_post(&world, &post, separation(1.0)).unwrap(),
    );
    let files = posting::post_mounts(&world, &dest).expect("native post");
    assert_eq!(files[0].posted.bundle_files.len(), 3);
    let review = files[0].posted.controller_review.as_ref().unwrap();
    assert!(review.contains("Confirm zero steps"));
    assert!(review.contains("CCB030") && review.contains("245, 155, 140 and 134"));
    let iso = fs::read_to_string(dest.join("PUNCH40.ISO")).unwrap();
    // Translated nominal X limits. A wire radius or gap would change both.
    assert!(
        iso.contains("X-22.00000") && iso.contains("X18.00000"),
        "{iso}"
    );
    assert!(
        iso.contains("Y-13.00000") && iso.contains("Y7.00000"),
        "{iso}"
    );
    let sbl = fs::read_to_string(dest.join("PUNCH40.SBL")).unwrap();
    assert!(
        sbl.contains("ID_POSX,\"-32.000\"") && sbl.contains("ID_POSY,\"-3.000\""),
        "{sbl}"
    );
    assert!(sbl.contains("ID_PUNCH,c_punch") && sbl.contains("ID_MATERIAL,\"0001\""));
    assert!(sbl.contains("ASDELETE") && !sbl.contains("ASAGIE"));
    assert!(sbl.contains("ID_POSPOSZ,\"-20.000\""));
    assert!(sbl.contains("ID_SEPCUT,\"1.0\",je_dim)"));
    let sbr = fs::read_to_string(dest.join("PUNCH40.SBR")).unwrap();
    assert!(sbr.contains("IMPORT PUNCH40.ISO"));
    assert!(posting::post_mounts(&world, &dest).is_err());
    assert_eq!(fs::read_to_string(dest.join("PUNCH40.ISO")).unwrap(), iso);
    assert!(posting::post_setup_unwritten(&world).is_err());
}

#[test]
fn qualified_qn2_heights_set_both_piece_and_work_with_stock_top_at_zero() {
    for height in [15., 20., 40., 80.] {
        let world = punch(height, [2., 3., height], [-30., 0.]);
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("HEIGHT");
        posting::post_mounts_with_script_options(&world, &dest, Some(&separation(1.0))).unwrap();
        let sbl = fs::read_to_string(dest.join("HEIGHT.SBL")).unwrap();
        let height_field = format!("ID_HEIGHT,\"{height:.3}\"");
        for context in ["JE_AttribObject(PIECE,", "JE_AttribObject(WORK,"] {
            assert!(
                sbl.lines()
                    .any(|line| line.contains(context) && line.contains(&height_field))
            );
        }
        assert!(sbl.contains(&format!("ID_POSPOSZ,\"-{height:.3}\"")));
        assert!(sbl.contains("ID_POSX,\"-32.000\"") && sbl.contains("ID_POSY,\"-3.000\""));
    }
}

#[test]
fn an_incompatible_plate_or_datum_writes_no_agie_job() {
    for (height, origin, expected) in [
        (12.0, 12.0, "thickness"),
        (25.0, 25.0, "thickness"),
        (39.999, 39.999, "thickness"),
        (40.001, 40.001, "thickness"),
        (20.0, 0.0, "stock top"),
        (80.0, 20.0, "stock top"),
    ] {
        let world = punch(height, [0.0, 0.0, origin], [-30.0, 0.0]);
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("REFUSED");
        let why = posting::post_mounts_with_script_options(&world, &dest, Some(&separation(1.0)))
            .unwrap_err();
        assert!(why.contains(expected), "{why}");
        assert!(!dest.exists());
    }
}

#[test]
fn generic_preview_inside_declared_travel_cannot_clear_native_offsets() {
    let base = &configured_profile();
    for (axis, limit) in [("X", 20.18), ("Y", 10.18)] {
        let mut profile: serde_json::Value = serde_json::from_str(base).unwrap();
        let x = &mut profile["kinematics"]["default"]["parts"][0];
        let slide = if axis == "X" { x } else { &mut x["parts"][0] };
        slide["travel"] = serde_json::json!({"min": -40.0, "max": limit});
        let world = punch_setup(20.0, [0., 0., 20.], [-30., 0.], 60., &profile.to_string());
        let checks = posting::machine_checks(&world);
        assert_eq!(checks.len(), 1);
        assert!(checks[0].1.as_ref().unwrap().is_ok());
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("LIMIT");
        let why = posting::post_mounts_with_script_options(&world, &dest, Some(&separation(1.0)))
            .unwrap_err();
        assert!(
            why.contains(&format!("declared {axis} travel")) && why.contains("resident"),
            "{why}"
        );
        assert!(!dest.exists());
    }
}

#[test]
fn stock_must_contain_the_whole_native_rough_pass_and_wire() {
    let profile = &configured_profile();
    for (depth, passes) in [(20.36, false), (20.60, false), (20.80, true)] {
        let world = punch_setup(20.0, [0., 0., 20.], [-30., 0.], depth, profile);
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("MARGIN");
        let result =
            posting::post_mounts_with_script_options(&world, &dest, Some(&separation(1.0)));
        if passes {
            result.unwrap();
            assert!(dest.join("MARGIN.SBL").exists());
        } else {
            let why = result.unwrap_err();
            assert!(why.contains("0.4 mm"), "{why}");
            assert!(!dest.exists());
        }
    }
}

#[test]
fn a_second_workpiece_crossing_the_threading_path_blocks_publication() {
    let world = punch(20.0, [0.0, 0.0, 20.0], [-30.0, 0.0]);
    let mut history = History::default();
    let keeper = cube(Point3::new(-26.0, -2.5, 0.0), Point3::new(-24.0, 2.5, 20.0));
    history.execute(&world, ImportModel::from_mesh("keep", keeper));
    history.execute(&world, GenerateOperation::new(&world, "wire1").unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("TWOPARTS");
    let why = posting::post_mounts(&world, &dest).unwrap_err();
    assert!(why.contains("exactly one modeled workpiece"), "{why}");
    assert!(!dest.exists());
}

#[test]
fn the_closed_punch_requires_an_explicit_supported_separation_cut() {
    let world = punch(20.0, [0.0, 0.0, 20.0], [-30.0, 0.0]);
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("SEPTEST");
    for choice in [None, Some(0.0), Some(0.5), Some(2.0)] {
        let options = choice.map(separation);
        let why =
            posting::post_mounts_with_script_options(&world, &dest, options.as_ref()).unwrap_err();
        assert!(why.contains("separationLengthMm"), "{why}");
        assert!(!dest.exists());
    }
    // A CLI override applies to this run only.
    posting::post_mounts_with_script_options(&world, &dest, Some(&separation(1.0))).unwrap();
    let sbl = fs::read_to_string(dest.join("SEPTEST.SBL")).unwrap();
    assert!(sbl.contains("ID_SEPCUT,\"1.0\",je_dim)"));
    assert!(posting::post_mounts(&world, &temp.path().join("NOCHOICE")).is_err());
}

#[test]
fn the_work_must_fit_between_each_stated_guide() {
    for (guide, z) in [("lower_guide_z", -19.0), ("upper_guide_z", -1.0)] {
        let mut profile: serde_json::Value = serde_json::from_str(&configured_profile()).unwrap();
        profile["edm"][guide] = serde_json::json!(z);
        let world = punch_setup(20.0, [0., 0., 20.], [-30., 0.], 60., &profile.to_string());
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("GUIDE");
        let why = posting::post_mounts_with_script_options(&world, &dest, Some(&separation(1.0)))
            .unwrap_err();
        assert!(why.contains("guide"), "{why}");
        assert!(!dest.exists());
    }
}

#[test]
fn smoothing_blocks_the_native_agie_job_before_publication() {
    let world = punch(20.0, [0., 0., 20.], [-30., 0.]);
    let options = PostOptions { smoothing: true, ..PostOptions::default() };
    History::default().execute(&world, SetPostOptions::new(&world, options).unwrap());
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("SMOOTH");
    let error = posting::post_mounts_with_script_options(&world, &dest, Some(&separation(1.0))).unwrap_err();
    assert_eq!(error, "Native wire EDM jobs require default post options and One program output");
    assert!(!dest.exists());
}

#[test]
fn the_native_files_match_the_control_checked_rectangular_punch() {
    let world = punch(20.0, [0., 0., 20.], [-30., 0.]);
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("CCSEP20");
    posting::post_mounts_with_script_options(&world, &dest, Some(&separation(1.0))).unwrap();
    for (name, golden) in [
        (
            "CCSEP20.ISO",
            include_bytes!("golden/nc/agie-native/CCSEP20.ISO").as_slice(),
        ),
        (
            "CCSEP20.SBR",
            include_bytes!("golden/nc/agie-native/CCSEP20.SBR").as_slice(),
        ),
        (
            "CCSEP20.SBL",
            include_bytes!("golden/nc/agie-native/CCSEP20.SBL").as_slice(),
        ),
    ] {
        assert_eq!(fs::read(dest.join(name)).unwrap(), golden, "{name}");
    }
}

#[test]
fn qn2_requires_the_rough_wire_size_in_the_mounted_condition() {
    for (role, diameter) in [
        ("rough", None),
        ("rough", Some(0.2505)),
        ("finish", Some(0.25)),
        ("skim", Some(0.25)),
    ] {
        let mut profile: serde_json::Value = serde_json::from_str(&configured_profile()).unwrap();
        let row = profile["edm"]["conditions"][0].as_object_mut().unwrap();
        row.insert("role".into(), role.into());
        match diameter {
            Some(d) => row.insert("wire_diameter".into(), serde_json::json!(d)),
            None => row.remove("wire_diameter"),
        };
        let world = punch_setup(20.0, [0., 0., 20.], [-30., 0.], 60., &profile.to_string());
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("REFUSED");
        let why = posting::post_mounts_with_script_options(&world, &dest, Some(&separation(1.0)))
            .unwrap_err();
        assert!(why.contains("AGIE_QN2 machine condition"), "{why}");
        assert!(!dest.exists());
    }
}

#[test]
fn script_post_reports_the_required_control_review_and_zero_working_steps() {
    let temp = tempfile::tempdir().unwrap();
    let example = temp.path().join("examples/agie");
    fs::create_dir_all(&example).unwrap();
    fs::write(
        example.join("punch.obj"),
        include_bytes!("../../../examples/agie/punch.obj"),
    )
    .unwrap();
    let mut script = include_str!("../../../examples/agie/punch.camscript")
        .replace("cam.post(", "const receipt = cam.post(");
    script.push_str(
        r#"if (receipt.working_steps_generated !== false ||
              !receipt.controller_review.includes("CCB030") ||
              !receipt.controller_review.includes("245, 155, 140 and 134") ||
              receipt.files.length !== 3) {
            throw new Error("Expected a setup-only job and the effective work checks");
        }"#,
    );
    fs::write(example.join("punch.camscript"), script).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .current_dir(temp.path())
        .args(["script", "examples/agie/punch.camscript"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let sbl = fs::read_to_string(temp.path().join("PUNCH40/PUNCH40.SBL")).unwrap();
    assert_eq!(
        sbl,
        include_str!("golden/nc/agie-native/CCSEP20.SBL").replace("CCSEP20", "PUNCH40")
    );
}
