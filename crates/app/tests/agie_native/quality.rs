//! The quality dispatch of app::posting::agie (#2966): a saved Wire Contour condition
//! selects the CCB035 work quality after a project reload, and only for one 20 mm polygon punch.
//! Provenance: CAMCraft's original synthetic examples in examples/agie.
use std::{fs, path::Path, process::Command};

const FINE: &str = include_str!("../../../../examples/agie/fine-punch.camscript");
const PUNCH_OBJ: &str = include_str!("../../../../examples/agie/punch.obj");

/// Copy `assets` into `examples/agie` of `dir`, write `script` there and run it.
fn run(dir: &Path, name: &str, script: &str, assets: &[(&str, &str)]) -> std::process::Output {
    let examples = dir.join("examples/agie");
    fs::create_dir_all(&examples).unwrap();
    for (file, text) in assets {
        fs::write(examples.join(file), text).unwrap();
    }
    fs::write(examples.join(name), script).unwrap();
    Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .current_dir(dir)
        .env("CAMCRAFT_NO_SEED", "1")
        .args(["script", &format!("examples/agie/{name}")])
        .output()
        .unwrap()
}

/// `script` with both the machine condition and the operation condition set to RA035.
fn fine(script: &str) -> String {
    script.replace("AGIE_QN2", "AGIE_BERCO_RA035")
}

#[test]
fn the_saved_condition_selects_the_exact_requested_quality() {
    let temp = tempfile::tempdir().unwrap();
    let script = FINE.replace(
        "cam.generate();",
        "cam.save(\"fine.camproj\");\ncam.load(\"fine.camproj\");\ncam.generate();",
    );
    let output = run(
        temp.path(),
        "fine-punch.camscript",
        &script,
        &[("punch.obj", PUNCH_OBJ)],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let dir = temp.path().join("FINE20");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 3);
    // The control-checked job of #2966 on the zero-step setup.
    for (name, golden) in [
        (
            "FINE20.ISO",
            include_str!("../golden/nc/agie-native/FINE20.ISO"),
        ),
        (
            "FINE20.SBR",
            include_str!("../golden/nc/agie-native/FINE20.SBR"),
        ),
        (
            "FINE20.SBL",
            include_str!("../golden/nc/agie-native/FINE20.SBL"),
        ),
    ] {
        assert_eq!(
            fs::read_to_string(dir.join(name)).unwrap(),
            golden,
            "{name}"
        );
    }
    // Only the one quality line differs from the control-checked CCB030 punch: CCB035 and
    // the speed flag that preselects technology estbca25nsn060b035020 (#2966).
    let base = include_str!("../golden/nc/agie-native/CCSEP20.SBL").replace("CCSEP20", "FINE20");
    assert_eq!(
        include_str!("../golden/nc/agie-native/FINE20.SBL"),
        base.replace(
            "\"CCB030\",\"0.30,1.50,1.00,F,F,F\"",
            "\"CCB035\",\"0.35,1.50,1.00,T,F,F\""
        )
    );
    for ext in ["ISO", "SBR"] {
        let base = match ext {
            "ISO" => include_str!("../golden/nc/agie-native/CCSEP20.ISO"),
            _ => include_str!("../golden/nc/agie-native/CCSEP20.SBR"),
        };
        assert_eq!(
            fs::read_to_string(dir.join(format!("FINE20.{ext}"))).unwrap(),
            base.replace("CCSEP20", "FINE20")
        );
    }
}

#[test]
fn ra035_refuses_unmeasured_heights_without_a_directory() {
    for height in ["15", "40", "80"] {
        let temp = tempfile::tempdir().unwrap();
        let obj = PUNCH_OBJ
            .replace(" 20\r\n", &format!(" {height}\r\n"))
            .replace(" 20\n", &format!(" {height}\n"));
        let script = FINE
            .replace("dz: 20", &format!("dz: {height}"))
            .replace("origin: [0, 0, 20]", &format!("origin: [0, 0, {height}]"))
            .replace("top_z: 20", &format!("top_z: {height}"));
        let output = run(
            temp.path(),
            "fine-punch.camscript",
            &script,
            &[("punch.obj", &obj)],
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success() && stderr.contains("CCB035 quality has no measured offset"),
            "{height}: {stderr}"
        );
        assert!(!temp.path().join("FINE20").exists());
    }
}

/// Script name, script, assets, job directory and the expected refusal text.
type Case = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static str,
    &'static str,
);

#[test]
fn ra035_refuses_a_group_an_arc_punch_a_die_and_a_cutoff() {
    let cases: [Case; 5] = [
        (
            "punch-pair.camscript",
            include_str!("../../../../examples/agie/punch-pair.camscript"),
            &[
                (
                    "pair-left.obj",
                    include_str!("../../../../examples/agie/pair-left.obj"),
                ),
                (
                    "pair-right.obj",
                    include_str!("../../../../examples/agie/pair-right.obj"),
                ),
            ],
            "PAIR20",
            "Select AGIE_QN2 for a punch group or a DXF arc punch",
        ),
        (
            "circle-punch.camscript",
            include_str!("../../../../examples/agie/circle-punch.camscript"),
            &[(
                "circle-punch.dxf",
                include_str!("../../../../examples/agie/circle-punch.dxf"),
            )],
            "CIRCLE20",
            "Select AGIE_QN2 for a punch group or a DXF arc punch",
        ),
        // A convex tangent loop (#2983) is an arc punch too.
        (
            "rounded-punch.camscript",
            include_str!("../../../../examples/agie/rounded-punch.camscript"),
            &[(
                "rounded-punch.dxf",
                include_str!("../../../../examples/agie/rounded-punch.dxf"),
            )],
            "ROUND20",
            "Select AGIE_QN2 for a punch group or a DXF arc punch",
        ),
        (
            "die-plate.camscript",
            include_str!("../../../../examples/agie/die-plate.camscript"),
            &[(
                "die-plate.obj",
                include_str!("../../../../examples/agie/die-plate.obj"),
            )],
            "DIE20",
            "qualified for one closed punch only",
        ),
        (
            "cutoff-lower.camscript",
            include_str!("../../../../examples/agie/cutoff-lower.camscript"),
            &[(
                "cutoff-lower.obj",
                include_str!("../../../../examples/agie/cutoff-lower.obj"),
            )],
            "LOWER20",
            "qualified for one closed punch only",
        ),
    ];
    for (name, script, assets, job, why) in cases {
        let temp = tempfile::tempdir().unwrap();
        // The unchanged example posts; the RA035 request is refused.
        let output = run(temp.path(), name, script, assets);
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let temp = tempfile::tempdir().unwrap();
        let output = run(temp.path(), name, &fine(script), assets);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success() && stderr.contains(why),
            "{name}: {stderr}"
        );
        assert!(!temp.path().join(job).exists(), "{name}");
    }
}

/// The punch pair with both machine conditions and `first` / `second` on its two works.
fn mixed_pair(first: &str, second: &str) -> String {
    let script = include_str!("../../../../examples/agie/punch-pair.camscript");
    let rows = "conditions: [{\n  id: \"AGIE_QN2\", role: \"rough\", gap: 0.005, wire_diameter: 0.25\n}, {\n  id: \"AGIE_BERCO_RA035\", role: \"rough\", gap: 0.005, wire_diameter: 0.25\n}]";
    let script = script.replacen(
        "conditions: [{\n  id: \"AGIE_QN2\", role: \"rough\", gap: 0.005, wire_diameter: 0.25\n}]",
        rows,
        1,
    );
    assert!(script.contains("AGIE_BERCO_RA035"), "the machine rows of punch-pair.camscript changed");
    let mut works = script.split("condition: \"AGIE_QN2\"");
    let (head, middle, tail) = (works.next().unwrap(), works.next().unwrap(), works.next().unwrap());
    assert!(works.next().is_none());
    format!("{head}condition: \"{first}\"{middle}condition: \"{second}\"{tail}")
}

/// #2984: the piece registers one quality for all works. A group that mixes the two
/// conditions names them in either work order and writes no job directory.
#[test]
fn a_group_with_mixed_wire_conditions_is_refused_in_either_order() {
    let assets: &[(&str, &str)] = &[
        ("pair-left.obj", include_str!("../../../../examples/agie/pair-left.obj")),
        ("pair-right.obj", include_str!("../../../../examples/agie/pair-right.obj")),
    ];
    for (first, second) in [("AGIE_QN2", "AGIE_BERCO_RA035"), ("AGIE_BERCO_RA035", "AGIE_QN2")] {
        let temp = tempfile::tempdir().unwrap();
        let output = run(temp.path(), "punch-pair.camscript", &mixed_pair(first, second), assets);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success()
                && stderr.contains(
                    "The works of a group must use the same wire condition. This group mixes AGIE_BERCO_RA035 and AGIE_QN2"
                ),
            "{first} then {second}: {stderr}"
        );
        assert!(!temp.path().join("PAIR20").exists(), "{first} then {second}");
    }
}
