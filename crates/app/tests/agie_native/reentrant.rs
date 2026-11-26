//! The notched punch example: a reentrant contour posts through the CLI with
//! the zero-step setup of the rectangular punch.
use std::fs;

#[test]
fn cli_posts_the_notched_punch_from_its_original_mesh() {
    let dir = tempfile::tempdir().unwrap();
    let examples = dir.path().join("examples/agie");
    fs::create_dir_all(&examples).unwrap();
    fs::write(
        examples.join("notched-punch.obj"),
        include_str!("../../../../examples/agie/notched-punch.obj"),
    )
    .unwrap();
    let mut script = include_str!("../../../../examples/agie/notched-punch.camscript")
        .replace("cam.post(", "const receipt = cam.post(");
    script.push_str(
        r#"if (receipt.working_steps_generated !== false ||
              !receipt.controller_review.includes("CCB030") ||
              receipt.files.length !== 3) {
            throw new Error("Expected a setup-only notched job");
        }"#,
    );
    fs::write(examples.join("notched-punch.camscript"), script).unwrap();
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .current_dir(dir.path())
        .args(["script", "examples/agie/notched-punch.camscript"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{} {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let iso = fs::read_to_string(dir.path().join("NOTCH20/NOTCH20.ISO")).unwrap();
    for vertex in [
        "X10.00000 Y10.00000",
        "X0.00000 Y9.00000",
        "X-10.00000 Y10.00000",
    ] {
        assert!(iso.contains(vertex), "{iso}");
    }
    // The notch changes only the ISO object. The zero-step setup is the one
    // of the rectangular punch on the same stock and start hole.
    let sbl = fs::read_to_string(dir.path().join("NOTCH20/NOTCH20.SBL")).unwrap();
    assert_eq!(
        sbl,
        include_str!("../golden/nc/agie-native/CCSEP20.SBL").replace("CCSEP20", "NOTCH20")
    );
}
