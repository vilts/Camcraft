//! A die group (#2977) of two through-openings in one retained plate, posted through camcraft-cli.
//! Provenance: CAMCraft's original synthetic two-opening plate (`examples/agie/die-pair.obj`).
use std::{fs, path::Path, process::Command};

const SCRIPT: &str = include_str!("../../../../examples/agie/die-pair.camscript");
const PLATE: &str = include_str!("../../../../examples/agie/die-pair.obj");
const OTHER: &str = include_str!("../../../../examples/agie/pair-right.obj");

fn run(dir: &Path, script: &str) -> std::process::Output {
    let examples = dir.join("examples/agie");
    fs::create_dir_all(&examples).unwrap();
    fs::write(examples.join("die-pair.obj"), PLATE).unwrap();
    fs::write(examples.join("pair-right.obj"), OTHER).unwrap();
    fs::write(examples.join("die-pair.camscript"), script).unwrap();
    Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .current_dir(dir)
        .env("CAMCRAFT_NO_SEED", "1")
        .args(["script", "examples/agie/die-pair.camscript"])
        .output()
        .unwrap()
}

/// The CLI job is the control-checked die pair: the setup golden of
/// camcraft-post (agie_safe_setup::die_group) and the two ISOs that the control read.
#[test]
fn two_complete_rims_on_one_plate_post_the_control_checked_die_pair() {
    let temp = tempfile::tempdir().unwrap();
    let result = run(temp.path(), SCRIPT);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let dir = temp.path().join("DUODI20");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 4);
    let read = |name: &str| fs::read(dir.join(name)).unwrap();
    assert_eq!(
        read("DUODI20.SBL"),
        include_bytes!("../../../post/tests/golden/agie-setup/DUODI20.SBL")
    );
    assert_eq!(
        read("DUODI20.SBR"),
        include_bytes!("../../../post/tests/golden/agie-setup/DUODI20.SBR")
    );
    assert_eq!(
        read("DUODI201.ISO"),
        include_bytes!("../golden/nc/agie-native/DUODI201.ISO")
    );
    assert_eq!(
        read("DUODI202.ISO"),
        include_bytes!("../golden/nc/agie-native/DUODI202.ISO")
    );
    let sbl = String::from_utf8(read("DUODI20.SBL")).unwrap();
    for (n, hole) in [(1, "-35.500"), (2, "24.500")] {
        let work = format!("\"DUODI20\",\"CUT{n}\",");
        assert!(sbl.contains(&format!("{work}NULL,ID_PUNCH,c_die")));
        assert!(sbl.contains(&format!("{work}NULL,ID_SEPCUT,\"1.0\"")));
        assert!(sbl.contains(&format!("{work}\"stp1\",ID_POSX,\"{hole}\"")));
    }
}

#[test]
fn a_missing_shared_or_unsafe_die_work_writes_nothing() {
    for (case, script) in [
        ("same opening twice", SCRIPT.replace("x: 24.5", "x: -35.5")),
        (
            "one operation on a two-opening plate",
            SCRIPT.replace(", {name: \"DieR\", x: 24.5}", ""),
        ),
        (
            "hole in the other opening",
            SCRIPT.replace("start: [op.x, 0]", "start: [-35.5, 0]"),
        ),
        (
            "a punch and a die",
            SCRIPT.replace("side: \"die\"", "side: op.x < 0 ? \"die\" : \"punch\""),
        ),
        (
            "extra model",
            SCRIPT.replace(
                "cam.generate();",
                "cam.import(\"examples/agie/pair-right.obj\", \"extra\");\ncam.generate();",
            ),
        ),
        (
            "slug not captured",
            SCRIPT.replace("capture: true", "capture: false"),
        ),
        // A die group holds two openings: punch groups of up to nine do not raise it (#2980).
        (
            "three die works",
            SCRIPT.replace(
                "{name: \"DieR\", x: 24.5}]",
                "{name: \"DieR\", x: 24.5}, {name: \"DieR2\", x: 24.5}]",
            ),
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let result = run(temp.path(), &script);
        assert!(!result.status.success(), "{case}");
        assert!(!temp.path().join("DUODI20").exists(), "{case}");
        if case == "three die works" {
            let message = String::from_utf8_lossy(&result.stderr);
            assert!(
                message.contains("die groups of two openings only"),
                "{message}"
            );
        }
    }
}
