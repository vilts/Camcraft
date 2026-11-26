//! A selected DXF circle or racetrack punch through the public CLI and the native post (#2022).
//! Provenance: the original examples/agie DXF drawings and the control-checked CCSEP20 setup.
use std::{fs, path::Path, process::Command};

const SCRIPT: &str = include_str!("../../../../examples/agie/racetrack.camscript");
const DXF: &str = include_str!("../../../../examples/agie/racetrack.dxf");
const CIRCLE_SCRIPT: &str = include_str!("../../../../examples/agie/circle-punch.camscript");
const CIRCLE_DXF: &str = include_str!("../../../../examples/agie/circle-punch.dxf");
const SETUP: &str = include_str!("../golden/nc/agie-native/CCSEP20.SBL");

/// Run `script` from a fresh project root that holds `drawing` as examples/agie/racetrack.dxf.
pub(super) fn run(dir: &Path, script: &str, drawing: &str) -> std::process::Output {
    let paths = dir.join("examples/agie");
    fs::create_dir_all(&paths).unwrap();
    fs::write(paths.join("racetrack.dxf"), drawing).unwrap();
    fs::write(paths.join("racetrack.camscript"), script).unwrap();
    Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .current_dir(dir)
        .env("CAMCRAFT_NO_SEED", "1")
        .args(["script", "examples/agie/racetrack.camscript"])
        .output()
        .unwrap()
}

/// The control-checked punch setup with only the stem and the threading hole changed.
pub(super) fn setup(stem: &str, hole: [&str; 2]) -> String {
    let point = |axis: &str, value: &str| {
        format!("\"CCSEP20\",\"CUT1\",\"stp1\",ID_POS{axis},\"{value}\",je_dim)")
    };
    SETUP
        .replace(&point("X", "-30.000"), &point("X", hole[0]))
        .replace(&point("Y", "0.000"), &point("Y", hole[1]))
        .replace("CCSEP20", stem)
}

#[test]
fn a_translated_and_rotated_racetrack_posts_two_exact_g03_caps() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(dir.path(), SCRIPT, DXF);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let job = dir.path().join("RACE20");
    assert_eq!(
        fs::read(job.join("RACE20.ISO")).unwrap(),
        include_bytes!("../golden/nc/agie-native/RACE20.ISO").as_slice()
    );
    assert_eq!(fs::read_to_string(job.join("RACE20.SBL")).unwrap(), setup("RACE20", ["2.000", "-16.000"]));
    assert_eq!(
        fs::read_to_string(job.join("RACE20.SBR")).unwrap(),
        "AGIE.USING_RACE20.ISO IMPORT RACE20.ISO;\r\n"
    );
}

#[test]
fn a_circle_starts_at_its_radial_seam_and_keeps_the_setup_of_the_checked_punch() {
    let dir = tempfile::tempdir().unwrap();
    let script = CIRCLE_SCRIPT.replace("circle-punch", "racetrack");
    let output = run(dir.path(), &script, CIRCLE_DXF);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let job = dir.path().join("CIRCLE20");
    assert_eq!(
        fs::read(job.join("CIRCLE20.ISO")).unwrap(),
        include_bytes!("../golden/nc/agie-native/CIRCLE20.ISO").as_slice()
    );
    assert_eq!(fs::read_to_string(job.join("CIRCLE20.SBL")).unwrap(), setup("CIRCLE20", ["-17.000", "0.000"]));
}

/// The drawing with an unselected R0.05 circle on a later layer. Placed, its
/// centre is at world X(2 - y) Y1, beyond the +X cap extremum at X20.
fn with_mark(y: f64) -> String {
    DXF.replacen(
        "0\nENDSEC\n0\nEOF",
        &format!("0\nCIRCLE\n8\nzz\n10\n0\n20\n{y}\n40\n0.05\n0\nENDSEC\n0\nEOF"),
        1,
    )
}

#[test]
fn other_drawing_geometry_must_clear_the_native_rough_pass_and_wire() {
    // At X20.25..20.35 the mark clears the generated wire kerf (X20.13) but
    // not the native rough pass and wire (0.4 mm beyond the cap, X20.4).
    let dir = tempfile::tempdir().unwrap();
    let output = run(dir.path(), SCRIPT, &with_mark(-18.3));
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unselected DXF primitive"), "{stderr}");
    assert!(!dir.path().join("RACE20").exists());
    // At X20.45..20.55 it clears the native envelope too.
    let dir = tempfile::tempdir().unwrap();
    let output = run(dir.path(), SCRIPT, &with_mark(-18.5));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn a_moved_open_tall_or_misplaced_racetrack_publishes_nothing() {
    let open = DXF.replacen("50\n0\n51\n180", "50\n5\n51\n180", 1);
    let first_chain = SCRIPT.replace(".chains.find(chain => chain.closed)", ".chains[0]");
    let moved = SCRIPT.replace(
        "cam.post(",
        "cam.call(\"move_dxf_profile\", {name: \"racetrack\", position:[3,1], angle:90});\ncam.post(",
    );
    let tall = SCRIPT
        .replace("dz: 20", "dz: 40")
        .replace("origin: [0, 0, 20]", "origin: [0, 0, 40]")
        .replace("top_z: 20", "top_z: 40");
    for (script, drawing, why) in [
        (first_chain, open, "is open"),
        (moved, DXF.to_owned(), "inputs moved since generation"),
        (SCRIPT.replace("position: [2, 1]", "position: [38, 1]"), DXF.into(), "inside the box stock"),
        (SCRIPT.replace("[2, -16]", "[2, 1]"), DXF.into(), "lies inside the profile"),
        (tall, DXF.into(), "qualified 20 mm stock height"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = run(dir.path(), &script, &drawing);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success() && stderr.contains(why), "{why}: {stderr}");
        assert!(!dir.path().join("RACE20").exists());
    }
}

#[test]
fn a_nonradial_hole_or_unqualified_circle_radius_publishes_nothing() {
    let script = CIRCLE_SCRIPT.replace("circle-punch", "racetrack");
    let moved = script.replace(
        "cam.post(",
        "cam.call(\"move_dxf_profile\", {name:\"racetrack\",position:[0,2]});\ncam.post(",
    );
    let radius = |r: &str| CIRCLE_DXF.replace("40\n10\n", &format!("40\n{r}\n"));
    for (s, d, why) in [
        (script.replace("[-17, 0]", "[-17, 0.001]"), CIRCLE_DXF.to_owned(), "exact horizontal or vertical radius"),
        (script.replace("[-17, 0]", "[-12, 0]"), CIRCLE_DXF.to_owned(), "at least 3 mm beyond the circle"),
        (script.clone(), radius("0.4"), "from 0.5 to 20 mm"),
        (moved, CIRCLE_DXF.to_owned(), "inputs moved since generation"),
        (script.clone(), radius("5.99"), "radius from 6 to 15 mm"),
        (script, radius("15.01"), "radius from 6 to 15 mm"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = run(dir.path(), &s, &d);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success() && stderr.contains(why), "{why}: {stderr}");
        assert!(!dir.path().join("CIRCLE20").exists());
    }
}
