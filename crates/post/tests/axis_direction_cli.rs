//! The command-line post's door for an inverted machine (#373, #374):
//! `post --machine` with a file stating an inverted X posts the mirrored
//! program (the engine reads the raw file's `"direction": -1`); a file
//! stating an inverted Z - which the post does not translate - refuses by
//! name and writes nothing; and a script that does not declare
//! `axisInversion` refuses an inverted X by name, writing nothing.
//!
//! Gate map (M4.4): covers camcraft_post::machine, camcraft_post::post::invert, the post binary.

use std::process::Command;

fn tmp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft_axisdir_cli_{tag}_{}", std::process::id()))
}

fn fanuc() -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts/fanuc.cps");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Run the CLI on a machine part list, a script and a stream; the exit
/// status, stderr, and the written program (None when nothing was written).
fn run(tag: &str, parts: &str, cps: &str, stream: &str) -> (bool, String, Option<String>) {
    let mch = tmp(&format!("{tag}.mch"));
    let cpsf = tmp(&format!("{tag}.cps"));
    let cnc = tmp(&format!("{tag}.cnc"));
    let out = tmp(&format!("{tag}.nc"));
    let file = format!(r#"{{"general": {{"model": "Flip 1"}}, "kinematics": {{"default": {{"parts": [{parts}]}}}}}}"#);
    std::fs::write(&mch, file).expect("write mch");
    std::fs::write(&cpsf, cps).expect("write cps");
    std::fs::write(&cnc, stream).expect("write cnc");
    let r = Command::new(env!("CARGO_BIN_EXE_post"))
        .arg("--machine")
        .arg(&mch)
        .arg(&cpsf)
        .arg(&cnc)
        .arg(&out)
        .output()
        .expect("run post");
    let written = std::fs::read_to_string(&out).ok();
    for p in [&mch, &cpsf, &cnc, &out] {
        let _ = std::fs::remove_file(p);
    }
    (r.status.success(), String::from_utf8_lossy(&r.stderr).to_string(), written)
}

// #1359: the fanuc post refuses a cut with the spindle stopped. #1633: the
// stream states its clearance, as `post::dump` does (a refused first
// traverse writes no move).
const STREAM: &str = "PARAM SpindleSpeed 6000\nPARAM ClearancePlane 5\nRapid 10000 5000 2000\nFeed 20000 5000 0 F500000\n";

#[test]
fn the_cli_posts_an_inverted_x_mirrored() {
    let (ok, err, nc) = run("flipx", r#"{"type": "linear", "name": "X", "direction": -1}"#, &fanuc(), STREAM);
    assert!(ok, "{err}");
    let nc = nc.expect("written");
    assert!(nc.contains("G00 X-10. Y5. Z2."), "{nc}");
    assert!(nc.contains("G01 X-20. Y5. Z0. F500"), "{nc}");
}

#[test]
fn the_cli_refuses_an_inverted_z_by_name() {
    let (ok, err, nc) = run("flipz", r#"{"type": "linear", "name": "Z", "direction": -1}"#, &fanuc(), STREAM);
    assert!(!ok, "refused");
    assert!(
        err.contains("machine Flip 1 has an inverted Z axis; posting for an inverted Z is not supported yet"),
        "{err}"
    );
    assert!(nc.is_none(), "nothing written");
}

#[test]
fn the_cli_refuses_an_unverified_script_for_an_inverted_x() {
    let (ok, err, nc) = run(
        "bare",
        r#"{"type": "linear", "name": "X", "direction": -1}"#,
        "function onLinear() { writeln('G1 X' + properties.X); }\n",
        STREAM,
    );
    assert!(!ok, "refused");
    assert!(err.contains("machine Flip 1 has an inverted X axis, and this post script is not verified"), "{err}");
    assert!(nc.is_none(), "nothing written");
}
