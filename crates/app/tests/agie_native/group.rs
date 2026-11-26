//! Punch groups of two (#2961), three and nine (#2980) complete modeled punches, posted through
//! camcraft-cli.
//! Provenance: CAMCraft's original synthetic rectangular and five-sided vertical solids.
use std::{fs, path::Path, process::Command};

const SCRIPT: &str = include_str!("../../../../examples/agie/punch-pair.camscript");
const LEFT: &str = include_str!("../../../../examples/agie/pair-left.obj");
const RIGHT: &str = include_str!("../../../../examples/agie/pair-right.obj");
const TRIO: &str = include_str!("../../../../examples/agie/punch-trio.camscript");
const GRID: &str = include_str!("../../../../examples/agie/punch-grid.camscript");
const SETUP_GOLDEN: &str = "../post/tests/golden/agie-setup";

fn run(dir: &Path, script: &str) -> std::process::Output {
    let examples = dir.join("examples/agie");
    fs::create_dir_all(&examples).unwrap();
    fs::write(examples.join("pair-left.obj"), LEFT).unwrap();
    fs::write(examples.join("pair-right.obj"), RIGHT).unwrap();
    fs::write(examples.join("punch-pair.camscript"), script).unwrap();
    Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .current_dir(dir)
        .env("CAMCRAFT_NO_SEED", "1")
        .args(["script", "examples/agie/punch-pair.camscript"])
        .output()
        .unwrap()
}

/// The directory `stem` that `script` posts, after it checks its SBL and SBR
/// against the setup goldens of camcraft-post (agie_safe_setup::group).
fn posted(temp: &Path, script: &str, stem: &str, works: usize) -> std::path::PathBuf {
    let result = run(temp, script);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let dir = temp.join(stem);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), works + 2);
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join(SETUP_GOLDEN);
    for ext in ["SBL", "SBR"] {
        let name = format!("{stem}.{ext}");
        assert_eq!(
            fs::read(dir.join(&name)).unwrap(),
            fs::read(golden.join(&name)).unwrap(),
            "{name}"
        );
    }
    dir
}

#[test]
fn three_model_owners_post_the_trio_goldens() {
    let temp = tempfile::tempdir().unwrap();
    let dir = posted(temp.path(), TRIO, "TRIO20", 3);
    for (n, iso) in [
        (
            1,
            include_bytes!("../golden/nc/agie-native/TRIO201.ISO").as_slice(),
        ),
        (
            2,
            include_bytes!("../golden/nc/agie-native/TRIO202.ISO").as_slice(),
        ),
        (
            3,
            include_bytes!("../golden/nc/agie-native/TRIO203.ISO").as_slice(),
        ),
    ] {
        assert_eq!(
            fs::read(dir.join(format!("TRIO20{n}.ISO"))).unwrap(),
            iso,
            "TRIO20{n}"
        );
    }
}

#[test]
fn nine_model_owners_post_the_grid_goldens_with_their_own_iso_and_hole() {
    let temp = tempfile::tempdir().unwrap();
    let dir = posted(temp.path(), GRID, "NINE20", 9);
    let sbl = fs::read_to_string(dir.join("NINE20.SBL")).unwrap();
    for i in 0..9 {
        let (x, y) = ((i % 3 - 1) * 32, (i / 3 - 1) * 32);
        let work = format!("\"NINE20\",\"CUT{}\"", i + 1);
        assert!(sbl.contains(&format!("{work},\"stp1\",ID_POSX,\"{}.000\"", x - 13)));
        assert!(sbl.contains(&format!("{work},\"stp1\",ID_POSY,\"{y}.000\"")));
        let iso = fs::read_to_string(dir.join(format!("NINE20{}.ISO", i + 1))).unwrap();
        assert!(
            iso.contains(&format!("N0001 G00 X{}.00000 Y{y}.00000 ;", x - 7)),
            "{iso}"
        );
    }
}

#[test]
fn a_tenth_work_an_unowned_model_a_shared_source_or_a_far_collision_writes_nothing() {
    for (script, why) in [
        // A tenth operation and model, on a clear cell of the grid.
        (
            GRID.replacen(
                "for (const [i, center]",
                "centers.push([0, 64]);\nfor (const [i, center]",
                1,
            ),
            "2 to 9",
        ),
        (
            GRID.replace(
                "cam.generate();",
                "cam.import(\"examples/agie/pair-left.obj\", \"unused\");\ncam.generate();",
            ),
            "punch of one work",
        ),
        (
            GRID.replace(
                "source: \"Retained\" + (i + 1)",
                "source: \"Retained\" + (i === 8 ? 1 : i + 1)",
            ),
            "different modeled punches",
        ),
        // Work 3 on work 1, with work 2 clear of both.
        (
            TRIO.replace("[32, 0]]", "[-32, 0]]"),
            "TRIO201 and TRIO203 share a cutting area",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let result = run(temp.path(), &script);
        let message = String::from_utf8_lossy(&result.stderr);
        assert!(
            !result.status.success() && message.contains(why),
            "{why}: {message}"
        );
        for stem in ["NINE20", "TRIO20"] {
            assert!(!temp.path().join(stem).exists());
        }
    }
}

#[test]
fn the_two_model_owners_have_distinct_programs_holes_and_captures() {
    let temp = tempfile::tempdir().unwrap();
    let result = run(temp.path(), SCRIPT);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let dir = temp.path().join("PAIR20");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 4);
    let sbl = fs::read_to_string(dir.join("PAIR20.SBL")).unwrap();
    let map = fs::read_to_string(dir.join("PAIR20.SBR")).unwrap();
    // The CLI job is the setup golden of camcraft-post (agie_safe_setup::group).
    assert_eq!(
        sbl.as_bytes(),
        include_bytes!("../../../post/tests/golden/agie-setup/PAIR20.SBL")
    );
    assert_eq!(
        map.as_bytes(),
        include_bytes!("../../../post/tests/golden/agie-setup/PAIR20.SBR")
    );
    for (num, pierce, wall) in [(1, "-30.000", "-24.00000"), (2, "1.000", "7.00000")] {
        let alias = format!("PAIR20{num}.ISO");
        let context = format!("\"PAIR20\",\"CUT{num}\"");
        assert!(sbl.contains(&format!(
            "{context},NULL,ID_GEOMETRY,\"AGIE.USING_{alias}\""
        )));
        assert!(sbl.contains(&format!("{context},\"stp1\",ID_POSX,\"{pierce}\"")));
        assert!(map.contains(&format!("AGIE.USING_{alias} IMPORT {alias};\r\n")));
        let iso = fs::read_to_string(dir.join(alias)).unwrap();
        assert!(iso.contains(&format!("G00 X{wall} Y0.00000")));
    }
}

#[test]
fn a_shared_source_an_unowned_model_or_a_crossing_thread_path_writes_nothing() {
    for script in [
        SCRIPT.replace("source: \"right\"", "source: \"left\""),
        SCRIPT.replace(
            "cam.generate();",
            "cam.import(\"examples/agie/pair-right.obj\", \"third\");\ncam.generate();",
        ),
        SCRIPT.replace("[1, 0]", "[-32, 0]"),
        SCRIPT.replace("[1, 0]", "[-3.45, 0]"),
        SCRIPT.replacen("capture: true", "capture: false", 1),
    ] {
        let temp = tempfile::tempdir().unwrap();
        assert!(!run(temp.path(), &script).status.success(), "{script}");
        assert!(!temp.path().join("PAIR20").exists());
    }
}
