//! The cutoff examples: a native wire EDM open cut through CLI selection and
//! posting (#2962), on original synthetic full-height retained prisms with
//! straight stock edges.
use std::{fs, path::Path, process::Command};
const LOWER: &str = include_str!("../../../../examples/agie/cutoff-lower.camscript");
const UPPER: &str = include_str!("../../../../examples/agie/cutoff-upper.camscript");
const LOWER_MESH: &str = include_str!("../../../../examples/agie/cutoff-lower.obj");
const UPPER_MESH: &str = include_str!("../../../../examples/agie/cutoff-upper.obj");

fn run(directory: &Path, script: &str) -> std::process::Output {
    let examples = directory.join("examples/agie");
    fs::create_dir_all(&examples).unwrap();
    for (file, data) in [
        ("cutoff-lower.obj", LOWER_MESH),
        ("cutoff-upper.obj", UPPER_MESH),
    ] {
        fs::write(examples.join(file), data).unwrap();
    }
    fs::write(directory.join("cut.camscript"), script).unwrap();
    Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .current_dir(directory)
        .args(["script", "cut.camscript"])
        .output()
        .unwrap()
}

#[test]
fn a_native_cutoff_has_one_nominal_crossing_and_distinct_air_endpoints() {
    for (name, script, y, air_y, side) in [
        ("LOWER20", LOWER, -6., -3., "c_left"),
        ("UPPER20", UPPER, 6., 3., "c_right"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let result = run(dir.path(), script);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let job = dir.path().join(name);
        let iso = fs::read_to_string(job.join(format!("{name}.ISO"))).unwrap();
        assert_eq!(iso.matches("G01").count(), 1, "{iso}");
        assert!(
            iso.contains(&format!("X-44.00000 Y{y:.5}"))
                && iso.contains(&format!("X44.00000 Y{y:.5}")),
            "{iso}"
        );
        let sbl = fs::read_to_string(job.join(format!("{name}.SBL"))).unwrap();
        assert!(iso.contains("G00 X-44.00000"), "{iso}");
        // The zero-step setup: one ASDELETE, no ASAGIE, no start hole entry type.
        let cuts: Vec<_> = sbl.lines().filter(|l| l.contains("JE_GenerateCuts(")).collect();
        assert_eq!(cuts.len(), 1, "{sbl}");
        assert!(cuts[0].contains("ASDELETE") && !sbl.contains("ASAGIE") && !sbl.contains("ID_ENTRY,"));
        if name == "LOWER20" {
            assert_eq!(sbl, include_str!("../../../post/tests/golden/agie-setup/LOWER20.SBL"));
        }
        assert!(sbl.contains(&format!("ID_PUNCH,{side},")), "{sbl}");
        for field in [
            "ID_STPENTRY,\"stp1\"",
            "ID_STPEXIT,\"stp2\"",
            "ID_SEPCUT,\"0.0\"",
        ] {
            assert!(sbl.contains(field), "{field}");
        }
        for (id, x) in [("stp1", -44.), ("stp2", 44.)] {
            assert!(sbl.contains(&format!("\"{id}\",ID_POSX,\"{x:.3}\"")));
            assert!(sbl.contains(&format!("\"{id}\",ID_POSY,\"{air_y:.3}\"")));
        }
    }
}

#[test]
fn unsafe_or_unconfirmed_cutoffs_publish_no_files() {
    for script in [
        LOWER.replace("both_pieces_restrained: true", "capture: true"),
        LOWER.replace(
            "stock_centered_in_xy_travel: true",
            "stock_centered_in_xy_travel: false",
        ),
        LOWER.replace("forward ?", "!forward ?"),
        LOWER.replace("top_z: 20", "start: [0, 0], top_z: 20"),
        LOWER.replace("top_z: 20", "clearance: 4, top_z: 20"),
        LOWER.replace("dx: 80", "dx: 90"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let result = run(dir.path(), &script);
        assert!(!result.status.success(), "{script}");
        assert!(!dir.path().join("LOWER20").exists());
    }
}
