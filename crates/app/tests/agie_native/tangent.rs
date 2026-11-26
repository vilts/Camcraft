//! A selected convex tangent DXF loop punch through the public CLI and the native post (#2983).
//! Provenance: the original examples/agie tangent drawings and the control-checked CCSEP20 setup.
use super::dxf::{run, setup};
use std::fs;

const ROUNDED_SCRIPT: &str = include_str!("../../../../examples/agie/rounded-punch.camscript");
const ROUNDED_DXF: &str = include_str!("../../../../examples/agie/rounded-punch.dxf");
const THREE_SCRIPT: &str = include_str!("../../../../examples/agie/three-corner-punch.camscript");
const THREE_DXF: &str = include_str!("../../../../examples/agie/three-corner-punch.dxf");

#[test]
fn a_mixed_radius_loop_posts_each_nominal_arc_and_the_checked_setup() {
    let dir = tempfile::tempdir().unwrap();
    let script = ROUNDED_SCRIPT.replace("rounded-punch", "racetrack");
    let output = run(dir.path(), &script, ROUNDED_DXF);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let job = dir.path().join("ROUND20");
    assert_eq!(
        fs::read(job.join("ROUND20.ISO")).unwrap(),
        include_bytes!("../golden/nc/agie-native/ROUND20.ISO").as_slice()
    );
    assert_eq!(
        fs::read_to_string(job.join("ROUND20.SBL")).unwrap(),
        setup("ROUND20", ["0.000", "-21.000"])
    );
    assert_eq!(
        fs::read_to_string(job.join("ROUND20.SBR")).unwrap(),
        "AGIE.USING_ROUND20.ISO IMPORT ROUND20.ISO;\r\n"
    );
}

#[test]
fn an_oblique_tangent_side_joins_three_distinct_arcs() {
    let dir = tempfile::tempdir().unwrap();
    let script = THREE_SCRIPT.replace("three-corner-punch", "racetrack");
    let output = run(dir.path(), &script, THREE_DXF);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let job = dir.path().join("THREEARC");
    assert_eq!(
        fs::read(job.join("THREEARC.ISO")).unwrap(),
        include_bytes!("../golden/nc/agie-native/THREEARC.ISO").as_slice()
    );
    assert_eq!(
        fs::read_to_string(job.join("THREEARC.SBL")).unwrap(),
        setup("THREEARC", ["12.000", "-12.000"])
    );
}

#[test]
fn a_short_inner_offgrid_edge_or_folded_loop_publishes_nothing() {
    let script = ROUNDED_SCRIPT.replace("rounded-punch", "racetrack");
    let flat = "LINE\n8\nprofile\n10\n-16\n20\n-14\n11\n14\n21\n-14";
    assert!(ROUNDED_DXF.contains(flat));
    // These semicircles have the two ends of the flat side.
    let arc = "ARC\n8\nprofile\n10\n-1\n20\n-14\n40\n15\n50\n";
    for (script, drawing, why) in [
        (
            script.replace("[0, -21]", "[-15, -21]"),
            ROUNDED_DXF.to_owned(),
            "at least 3 mm",
        ),
        (
            script.replace("[0, -21]", "[0, -12]"),
            ROUNDED_DXF.to_owned(),
            "lies inside the profile",
        ),
        (
            script.replace("position: [0, 0]", "position: [0.000003, 0]"),
            ROUNDED_DXF.to_owned(),
            "five-decimal",
        ),
        (
            script.replace("position: [0, 0]", "position: [19.8, 0]"),
            ROUNDED_DXF.to_owned(),
            "stock",
        ),
        (
            script.clone(),
            ROUNDED_DXF.replace(flat, &format!("{arc}0\n51\n180")),
            "convex loop",
        ),
        // #3937: the lower half circle reaches Y-29 and encloses the hole
        // X0 Y-21. The arc door of a selected chain refuses that hole first.
        (
            script.clone(),
            ROUNDED_DXF.replace(flat, &format!("{arc}180\n51\n360")),
            "lies inside the profile",
        ),
        // With the hole outside the half circle, the arc door refuses the
        // sharp inside corner at X14 Y-14 before the native door can.
        (
            script.replace("[0, -21]", "[-19, -24]"),
            ROUNDED_DXF.replace(flat, &format!("{arc}180\n51\n360")),
            "an inside corner or slot of the profile near X14.000 Y-14.000",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = run(dir.path(), &script, &drawing);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success() && stderr.contains(why),
            "{why}: {stderr}"
        );
        assert!(!dir.path().join("ROUND20").exists());
    }
}
