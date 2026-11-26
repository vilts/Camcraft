//! The die-plate example: the native wire EDM die selection and post through
//! the normal CLI (#2957), on an original synthetic retained plate with a
//! faceted through-opening.
use std::{fs, path::Path, process::Command};

// Two openings of one plate (#2977).
#[path = "die_group.rs"]
mod die_group;

const SCRIPT: &str = include_str!("../../../../examples/agie/die-plate.camscript");
const PLATE: &str = include_str!("../../../../examples/agie/die-plate.obj");

fn run(directory: &Path, script: &str, mesh: &str) -> std::process::Output {
    let examples = directory.join("examples/agie");
    fs::create_dir_all(&examples).unwrap();
    fs::write(examples.join("die-plate.obj"), mesh).unwrap();
    fs::write(examples.join("die-plate.camscript"), script).unwrap();
    Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .current_dir(directory)
        .args(["script", "examples/agie/die-plate.camscript"])
        .output()
        .unwrap()
}

#[test]
fn the_selected_die_rim_posts_nominal_walls_and_inward_compensation() {
    let directory = tempfile::tempdir().unwrap();
    let result = run(directory.path(), SCRIPT, PLATE);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = directory.path().join("DIE20");
    let iso = fs::read_to_string(output.join("DIE20.ISO")).unwrap();
    assert!(iso.contains("G00 X-12.50000 Y0.00000"));
    assert!(iso.contains("G01 X12.50000 Y6.00000"));
    // The nominal opening is counter-clockwise from the seam on the wall X-12.5.
    assert!(iso.contains("N0001 G00 X-12.50000 Y0.00000 ;\r\nN0002 G01 X-12.50000 Y-6.00000 ;"));
    assert!(iso.ends_with("N0040 G01 X-12.50000 Y0.00000 ;\r\nN0041 M02 ;\r\n"));
    // The zero-step die setup: the recorded setup of the post crate.
    let job = fs::read_to_string(output.join("DIE20.SBL")).unwrap();
    assert_eq!(
        job,
        include_str!("../../../post/tests/golden/agie-setup/DIE20.SBL")
    );
    assert!(job.contains("ID_PUNCH,c_die") && !job.contains("ID_PUNCH,c_punch"));
}

#[test]
fn an_outer_rim_an_external_pierce_or_an_incomplete_rim_cannot_publish_a_die() {
    for (case, script) in [
        (
            "outside boundary",
            SCRIPT.replace("=== \"hole\"", "=== \"outer\""),
        ),
        ("pierce in steel", SCRIPT.replace("[-5.5, 0]", "[-20, 0]")),
        (
            "incomplete rim",
            SCRIPT.replace("rim.chains.map", "rim.chains.slice(1).map"),
        ),
        ("unsupported height", SCRIPT.replace("dz: 20", "dz: 40")),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let result = run(directory.path(), &script, PLATE);
        assert!(!result.status.success(), "{case}");
        assert!(!directory.path().join("DIE20").exists(), "{case}");
    }
}

#[test]
fn a_blind_floor_under_the_selected_upper_rim_cannot_publish_a_die() {
    // Convert the synthetic through-hole into a blind pocket with a floor at Z4.
    // The fixture has four outer corners followed by the rim at each Z level.
    let vertices: Vec<_> = PLATE
        .lines()
        .filter(|line| line.starts_with("v "))
        .collect();
    let per_cap = vertices.len() / 2;
    let mut blind = String::new();
    for (i, line) in vertices.iter().enumerate() {
        if (4..per_cap).contains(&i) {
            let (xy, _) = line.rsplit_once(' ').unwrap();
            blind.push_str(&format!("{xy} 4\n"));
        } else {
            blind.push_str(&format!("{line}\n"));
        }
    }
    for line in PLATE.lines().filter(|line| line.starts_with("f ")) {
        let mut ids = line
            .split_whitespace()
            .skip(1)
            .map(|s| s.parse::<usize>().unwrap());
        // Keep the walls and upper cap. Replace the annular bottom cap.
        if ids.any(|v| v > per_cap) {
            blind.push_str(&format!("{line}\n"));
        }
    }
    blind.push_str("f 3 2 1\nf 4 3 1\n");
    for i in 6..per_cap {
        blind.push_str(&format!("f 5 {i} {}\n", i + 1));
    }
    let directory = tempfile::tempdir().unwrap();
    let result = run(directory.path(), SCRIPT, &blind);
    assert!(!result.status.success());
    assert!(!directory.path().join("DIE20").exists());
}
