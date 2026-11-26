//! The command-line post reads a vendor `.mch` the way the app does
//! (#996): `post --machine <shipped machine>` posts with the file's
//! kinematics tree parsed and mirrored, so its rotaries, spindle limit
//! and coolant codes reach the script - the same program the app posts
//! for the same machine and stream, not a refusal the app never gives.
//!
//! Gate map (M4.4): covers camcraft_post::post bin (post.rs), camcraft_post::post::machine::mirror.

use camcraft_post::post::{engine, machine};
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("camcraft_vendor_mch_cli_{tag}_{}", std::process::id()))
}

/// Run the CLI on a shipped machine, the Fanuc post and a stream: the
/// exit status, stderr, and the written program (None when nothing was).
fn run(tag: &str, mch: &str, stream: &str) -> (bool, String, Option<String>) {
    let cnc = tmp(&format!("{tag}.cnc"));
    let out = tmp(&format!("{tag}.nc"));
    std::fs::write(&cnc, stream).expect("write cnc");
    let r = Command::new(env!("CARGO_BIN_EXE_post"))
        .arg("--machine")
        .arg(repo(mch))
        .arg(repo("assets/posts/fanuc.cps"))
        .arg(&cnc)
        .arg(&out)
        .output()
        .expect("run post");
    let written = std::fs::read_to_string(&out).ok();
    for p in [&cnc, &out] {
        let _ = std::fs::remove_file(p);
    }
    (r.status.success(), String::from_utf8_lossy(&r.stderr).to_string(), written)
}

/// What the app posts for the same file and stream: the machine parsed
/// and mirrored (`posting_mounts::machine_value_for`).
fn app_program(mch: &str, stream: &str) -> (String, Vec<String>) {
    let text = std::fs::read_to_string(repo(mch)).expect("read mch");
    let m = camcraft_post::machine::parse_mch(&text).expect("parse mch");
    let cps = std::fs::read_to_string(repo("assets/posts/fanuc.cps")).expect("read cps");
    let out = engine::run(&cps, &machine::mirror(&m), stream).expect("post");
    (out.program, out.warnings)
}

const TILTED: &str = "\
PARAM ClearancePlane 50
TOOL 1 T1
PARAM SpindleSpeed 8000
SECTION
Rapid X 0 Y 0 Z 5000 A 30000 B 15000
Feed X 10000 Y 0 Z 0 A 30000 B 15000 F500000
ENDSECTION
";

#[test]
fn the_cli_posts_a_tilted_stream_on_the_shipped_tilthead() {
    let (ok, err, nc) = run("tilt", "assets/machines/tilthead.mch", TILTED);
    assert!(ok, "{err}");
    assert!(!err.contains("without a tilting rotary"), "the file states A and B: {err}");
    assert!(!err.contains("warning"), "no warning for a stream inside the machine: {err}");
    let nc = nc.expect("written");
    assert!(nc.contains("G43.4"), "TCPM armed for the tilt: {nc}");
    assert!(nc.contains("A30") && nc.contains("B15"), "the angles posted: {nc}");
    assert!(nc.contains("M8"), "the file's flood code: {nc}");
    let (app, warnings) = app_program("assets/machines/tilthead.mch", TILTED);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(nc, app, "the CLI posts the app's program");
}

#[test]
fn the_cli_holds_a_shipped_machines_spindle_max_and_coolant_codes() {
    // vmc.mch states max_speed 12000 and Flood M8 / Mist M7.
    let over = "\
TOOL 1 T1
PARAM SpindleSpeed 20000
PARAM Coolant mist
SECTION
Rapid X 0 Y 0 Z 5000
Feed X 10000 Y 0 Z 0 F500000
ENDSECTION
";
    let (ok, err, nc) = run("vmc_over", "assets/machines/vmc.mch", over);
    assert!(ok, "{err}");
    assert!(err.contains("spindle rpm over machine maximum"), "the 12000 cap reached the script: {err}");
    let nc = nc.expect("written");
    assert!(nc.contains("M7"), "the file's mist code: {nc}");
    assert_eq!(nc, app_program("assets/machines/vmc.mch", over).0, "the CLI posts the app's program");

    let under = over.replace("SpindleSpeed 20000", "SpindleSpeed 9000");
    let (ok, err, nc) = run("vmc_under", "assets/machines/vmc.mch", &under);
    assert!(ok, "{err}");
    assert!(!err.contains("spindle rpm over machine maximum"), "under the cap: {err}");
    assert!(nc.expect("written").contains("S9000 M3"), "the spindle word posts");
}

#[test]
fn the_cli_posts_a_vendor_tree_waterjet_machine() {
    let mch = tmp("waterjet.mch");
    let cnc = tmp("waterjet.cnc");
    let out = tmp("waterjet.nc");
    let vendor = serde_json::json!({
        "general": { "model": "CLI waterjet", "kind": "waterjet" },
        "kinematics": { "default": {
            "units": { "length": "mm", "angle": "degrees" },
            "parts": [
                { "id": "x", "name": "X", "type": "linear", "travel": { "min": 0, "max": 460 } },
                { "id": "y", "name": "Y", "type": "linear", "travel": { "min": -305, "max": 0 } },
                { "id": "nozzle", "name": "nozzle", "type": "head" }
            ]
        } }
    });
    std::fs::write(&mch, vendor.to_string()).expect("write vendor tree");
    std::fs::write(&cnc, "WATERJET ABRASIVE\nSECTION\nRapid X 0 Y 0 Z 0\nJET ON C0 OUTER\nFeed X 10000 Y -10000 Z 0 F300000\nJET OFF C0\nENDSECTION\n").expect("write stream");
    let result = Command::new(env!("CARGO_BIN_EXE_post"))
        .arg("--machine")
        .arg(&mch)
        .arg(repo("assets/posts/waterjet-iso.cps"))
        .arg(&cnc)
        .arg(&out)
        .output()
        .expect("run post");
    let program = std::fs::read_to_string(&out).unwrap_or_default();
    for file in [&mch, &cnc, &out] {
        let _ = std::fs::remove_file(file);
    }
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "{stderr}");
    assert!(
        !stderr.contains("warning") && !stderr.contains("not declared a waterjet"),
        "{stderr}"
    );
    assert!(
        program.contains("G0 X0 Y0") && program.contains("G1 X10 Y-10 F300"),
        "{program}"
    );
    let water = program.find("M10\n").expect("water on");
    let abrasive = program.find("M12\n").expect("abrasive on");
    let cut = program.find("G1 X10 Y-10 F300").expect("cut");
    assert!(water < abrasive && abrasive < cut, "{program}");
}
