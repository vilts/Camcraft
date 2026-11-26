//! Setup-only Classic Cut 2S reference records from CamCraft's offline controller-checked synthetic punch.
//! Gate map: covers camcraft_post::agie_setup.
use camcraft_post::agie_setup::{PunchSetup20, SetupError};

#[path = "agie_safe_setup/cutoff.rs"]
mod cutoff;
#[path = "agie_safe_setup/die_group.rs"]
mod die_group;
#[path = "agie_safe_setup/group.rs"]
mod group;

fn punch() -> PunchSetup20<'static> {
    PunchSetup20 {
        stem: "CCSEP20",
        stock_min_um: [-40_000, -30_000],
        stock_max_um: [40_000, 30_000],
        start_hole_um: [-30_000, 0],
        height_um: 20_000,
    }
}

#[test]
fn setup_files_match_the_control_checked_zero_step_punch() {
    let files = punch().encode().unwrap();
    assert_eq!(
        files.sbl.as_bytes(),
        include_bytes!("golden/agie-setup/CCSEP20.SBL")
    );
    assert_eq!(
        files.sbr.as_bytes(),
        include_bytes!("golden/agie-setup/CCSEP20.SBR")
    );
    let cuts: Vec<_> = files
        .sbl
        .lines()
        .filter(|l| l.contains("JE_GenerateCuts("))
        .collect();
    assert_eq!(
        cuts,
        ["  ok=JE_GenerateCuts(WORK,je_piece,\"CCSEP20\",\"CUT1\",ASDELETE,NULL)"]
    );
    // The 20 mm review text is the text of the control-checked reference job.
    assert_eq!(
        files.controller_review,
        concat!(
            "Import the ISO into ISO Sys and keep its SBR beside the SBL during script import. ",
            "Select the newly created piece. Earlier pieces can retain their steps on repeat import. ",
            "Match this work's ISO resource, geometry and start hole to the setup. Confirm zero steps. ",
            "Verify steel 0001, Berco Cut 0.25 and 20 mm. CCB030 requests Ra 0.30, Te 1.50, ",
            "Tkm 1.00 and three disabled options. Check the effective values and both visible options ",
            "on this work. Assign the matching TECCUT row. Check offsets of 245, 155, 140 and ",
            "134 µm in order before GRAFICHECK and FORMALCHECK.",
        )
    );
}

#[test]
fn each_measured_qn2_height_sets_piece_work_and_review_offsets() {
    for (mm, height, offsets) in [
        (15, "15.000", "245, 155, 140 and 134 µm"),
        (20, "20.000", "245, 155, 140 and 134 µm"),
        (40, "40.000", "260, 170, 140 and 130 µm"),
        (80, "80.000", "266, 176, 143 and 133 µm"),
    ] {
        let setup = PunchSetup20 {
            height_um: mm * 1000,
            ..punch()
        };
        let files = setup.encode().unwrap();
        for context in ["PIECE,SETATTR,je_piece,NULL,NULL,NULL", "WORK,SETATTR"] {
            let field = format!("ID_HEIGHT,\"{height}\",je_dim");
            assert!(
                files
                    .sbl
                    .lines()
                    .any(|l| l.contains(context) && l.contains(&field)),
                "{mm} mm {context}"
            );
        }
        assert!(
            files
                .sbl
                .contains(&format!("ID_POSPOSZ,\"-{height}\",je_dim"))
        );
        assert!(files.controller_review.contains(&format!("and {mm} mm.")));
        assert!(files.controller_review.contains(offsets), "{mm} mm");
        // Only the height fields change between heights.
        let reference = punch().encode().unwrap().sbl;
        let changed = files
            .sbl
            .lines()
            .zip(reference.lines())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(changed, if mm == 20 { 0 } else { 3 }, "{mm} mm");
    }
}

#[test]
fn an_unmeasured_qn2_height_writes_no_setup() {
    for height_um in [
        0, -20_000, 12_000, 19_999, 20_001, 25_000, 39_999, 60_000, 100_000,
    ] {
        let setup = PunchSetup20 {
            height_um,
            ..punch()
        };
        assert_eq!(
            setup.encode().unwrap_err(),
            SetupError::UnqualifiedHeight,
            "{height_um}"
        );
    }
}

#[test]
fn a_named_resource_and_signed_micron_coordinates_round_trip() {
    let setup = PunchSetup20 {
        stem: "DEMO2026",
        stock_min_um: [-2_010, -1_001],
        stock_max_um: [1_001, 1_000],
        start_hole_um: [-1, -1],
        height_um: 20_000,
    };
    let files = setup.encode().unwrap();
    assert_eq!(
        files.sbr,
        "AGIE.USING_DEMO2026.ISO IMPORT DEMO2026.ISO;\r\n"
    );
    for field in [
        "ID_NOMEOBJ,\"DEMO2026\",je_dim",
        "ID_GEOMETRY,\"AGIE.USING_DEMO2026.ISO\",je_dim",
        "ID_DIML,\"3.011\",je_dim",
        "ID_DIMB,\"2.001\",je_dim",
        "ID_POSPOSY,\"-1.001\",je_dim",
        "\"stp1\",ID_POSX,\"-0.001\",je_dim",
    ] {
        assert!(files.sbl.contains(field), "{field}");
    }
}

#[test]
fn device_names_paths_and_script_text_cannot_enter_an_agie_record() {
    for stem in [
        "",
        "9PART",
        "ABCDEFGHI",
        "con",
        "CON",
        "COM1",
        "LPT9",
        "../ISO",
        "X.ISO",
        "X\"\r\n",
        "Aé",
    ] {
        let setup = PunchSetup20 { stem, ..punch() };
        assert_eq!(
            setup.encode().unwrap_err(),
            SetupError::InvalidStem,
            "{stem:?}"
        );
    }
}

#[test]
fn stock_and_full_start_hole_must_fit_the_setup_span() {
    let setup = PunchSetup20 {
        stock_min_um: [-175_000, -125_000],
        stock_max_um: [175_000, 125_000],
        start_hole_um: [174_000, -124_000],
        ..punch()
    };
    assert!(setup.encode().is_ok());
    for (min, max, hole, error) in [
        (
            [-175_001, -125_000],
            [175_000, 125_000],
            [0, 0],
            SetupError::InvalidStock,
        ),
        (
            [-175_000, -125_001],
            [175_000, 125_000],
            [0, 0],
            SetupError::InvalidStock,
        ),
        ([0, 0], [0, 2_000], [0, 1_000], SetupError::InvalidStock),
        (
            [349_000, 0],
            [351_000, 2_000],
            [350_000, 1_000],
            SetupError::InvalidStock,
        ),
        (
            [i32::MIN, 0],
            [i32::MAX, 10_000],
            [0, 5_000],
            SetupError::InvalidStock,
        ),
        (
            setup.stock_min_um,
            setup.stock_max_um,
            [174_001, 0],
            SetupError::HoleOutsideStock,
        ),
        (
            setup.stock_min_um,
            setup.stock_max_um,
            [0, -124_001],
            SetupError::HoleOutsideStock,
        ),
        (
            setup.stock_min_um,
            setup.stock_max_um,
            [i32::MAX, 0],
            SetupError::HoleOutsideStock,
        ),
    ] {
        let bad = PunchSetup20 {
            stock_min_um: min,
            stock_max_um: max,
            start_hole_um: hole,
            ..setup
        };
        assert_eq!(bad.encode().unwrap_err(), error);
    }
}

/// The die job of `examples/agie/die-plate.camscript` (#2957).
fn die() -> PunchSetup20<'static> {
    PunchSetup20 {
        stem: "DIE20",
        stock_min_um: [-35_000, -35_000],
        stock_max_um: [35_000, 35_000],
        start_hole_um: [-5_500, 0],
        height_um: 20_000,
    }
}

#[test]
fn die_setup_is_the_zero_step_punch_setup_with_the_die_side() {
    let files = die().encode_die().unwrap();
    assert_eq!(
        files.sbl.as_bytes(),
        include_bytes!("golden/agie-setup/DIE20.SBL")
    );
    assert_eq!(files.sbr, "AGIE.USING_DIE20.ISO IMPORT DIE20.ISO;\r\n");
    // Only the work side differs from the zero-step punch of the same setup.
    let punch = die().encode().unwrap();
    assert_eq!(
        files.sbl.replace("ID_PUNCH,c_die", "ID_PUNCH,c_punch"),
        punch.sbl
    );
    assert!(
        files
            .controller_review
            .starts_with(punch.controller_review.as_str())
    );
    assert!(files.controller_review.ends_with(
        " Confirm the c_die side and that each offset is inside the opening. \
         Support the slug before the control releases the separation section."
    ));
}

#[test]
fn die_setup_is_refused_on_every_thickness_but_20_mm() {
    for mm in [15, 40, 80, 25] {
        let setup = PunchSetup20 {
            height_um: mm * 1000,
            ..die()
        };
        assert_eq!(
            setup.encode_die().unwrap_err(),
            SetupError::UnqualifiedDieHeight,
            "{mm} mm"
        );
    }
    let bad_stem = PunchSetup20 {
        stem: "die20",
        ..die()
    };
    assert_eq!(bad_stem.encode_die().unwrap_err(), SetupError::InvalidStem);
}
