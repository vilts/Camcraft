//! The Ra 0.35 quality request (#2966): one 20 mm polygon punch only, and CCB030 bytes unchanged.
use super::tests::punch;
use super::*;
use crate::agie_setup::{PunchSetup20, SetupError};

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn ccb035_changes_only_the_quality_line_of_a_20_mm_punch() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("BASE");
    let fine = temp.path().join("FINE");
    let review_030 = export_with_review(&punch(), &base, "PUNCH20", 1.0).unwrap();
    let review_035 =
        export_punch_with_review(&punch(), Quality::Ccb035, &fine, "PUNCH20", 1.0).unwrap();
    for name in ["PUNCH20.ISO", "PUNCH20.SBR"] {
        assert_eq!(read(&fine, name), read(&base, name), "{name}");
    }
    let old = "JE_CreateQuality(je_piece,\"CCB030\",\"0.30,1.50,1.00,F,F,F\",je_q1)";
    // The speed flag makes the control preselect the Ra 0.35 technology (#2966).
    let new = "JE_CreateQuality(je_piece,\"CCB035\",\"0.35,1.50,1.00,T,F,F\",je_q1)";
    let sbl = read(&fine, "PUNCH20.SBL");
    assert_eq!(sbl, read(&base, "PUNCH20.SBL").replace(old, new));
    // One quality object, zero steps, and no other quality name.
    assert_eq!(sbl.matches("JE_CreateQuality").count(), 1);
    assert!(sbl.contains("ASDELETE") && !sbl.contains("ASAGIE"));
    assert_eq!(
        review_035.controller_review,
        review_030
            .controller_review
            .replace("CCB030 requests Ra 0.30", "CCB035 requests Ra 0.35")
            .replace(
                "three disabled options",
                "the speed option set, the other two options disabled"
            )
            .replace(
                "Assign the matching TECCUT row.",
                "TECCUT must show the header TYPE:VS and preselect technology \
                 estbca25nsn060b035020 with 5 passes. If not, stop. Assign it."
            )
            .replace("245, 155, 140 and 134", "234, 189, 154, 139 and 129")
    );
    assert!(
        review_030
            .controller_review
            .contains("Check offsets of 245, 155, 140 and 134 µm in order")
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn ccb030_through_the_quality_export_keeps_every_byte() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("BASE");
    let same = temp.path().join("SAME");
    let a = export_with_review(&punch(), &base, "PUNCH20", 1.0).unwrap();
    let b = export_punch_with_review(&punch(), Quality::Ccb030, &same, "PUNCH20", 1.0).unwrap();
    for name in ["PUNCH20.ISO", "PUNCH20.SBR", "PUNCH20.SBL"] {
        assert_eq!(read(&same, name), read(&base, name), "{name}");
    }
    assert_eq!(a.controller_review, b.controller_review);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn ccb035_refuses_other_heights_sides_and_separations_without_files() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("FINE20");
    let refuse = |job: &AgieJob, separation: f64| {
        let error =
            export_punch_with_review(job, Quality::Ccb035, &out, "FINE20", separation).unwrap_err();
        assert!(!out.exists(), "{error}");
        error
    };
    // 15, 40 and 80 mm are measured for CCB030 only.
    for height in [15., 19.999, 20.001, 40., 80.] {
        let mut job = punch();
        job.stock_min[2] = -height;
        let error = refuse(&job, 1.0);
        assert!(
            error.contains("CCB035 quality has no measured offset")
                && error.ends_with("Supported thicknesses in mm are 20"),
            "{error}"
        );
    }
    for side in [AgieSide::Die, AgieSide::Left, AgieSide::Right] {
        let job = AgieJob { side, ..punch() };
        assert!(refuse(&job, 1.0).contains("one closed punch only"));
    }
    assert!(refuse(&punch(), 0.0).contains("separationLengthMm"));
    let mut job = punch();
    job.stock_max[2] = 1.0;
    assert!(refuse(&job, 1.0).contains("Fine+ Ra0.35 profile requires stock top Z0"));
}

#[test]
fn the_setup_encoder_holds_ccb035_to_20_mm() {
    let at = |height_um| PunchSetup20 {
        stem: "FINE20",
        stock_min_um: [-40_000, -30_000],
        stock_max_um: [40_000, 30_000],
        start_hole_um: [-30_000, 0],
        height_um,
    };
    for height_um in [15_000, 40_000, 80_000, 25_000] {
        assert_eq!(
            at(height_um).encode_quality(Quality::Ccb035).unwrap_err(),
            SetupError::UnqualifiedQualityHeight
        );
    }
    let base = at(20_000).encode().unwrap();
    let same = at(20_000).encode_quality(Quality::Ccb030).unwrap();
    assert_eq!((same.sbl, same.sbr), (base.sbl, base.sbr));
    assert!(at(20_000).encode_quality(Quality::Ccb035).is_ok());
}

#[test]
fn ccb035_has_five_measured_passes_and_keeps_the_0_4_mm_margin() {
    // Technology estbca25nsn060b035020 on the control (#2966, 25 Nov): main cut and four trims.
    assert_eq!(
        Quality::Ccb035.offsets_um(20_000),
        Some(&[234, 189, 154, 139, 129][..])
    );
    assert_eq!(Quality::Ccb035.rough_um(20_000), Some(234));
    assert_eq!(Quality::Ccb030.rough_um(20_000), Some(245));
    // 0.234 + 0.125 and 0.245 + 0.125 both round up to 0.4 mm.
    assert_eq!(contour::stock_margin(234), contour::stock_margin(245));
}
