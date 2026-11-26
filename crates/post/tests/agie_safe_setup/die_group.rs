//! Setup-only die group records of two through-openings in one retained plate, from CamCraft's
//! offline controller-checked synthetic die pair (#2977).
use camcraft_post::agie_setup::{GroupPunch, PunchGroup20, PunchSetup20, SetupError};

const OPENINGS: [GroupPunch<'static>; 2] = [
    GroupPunch {
        iso_stem: "DUODI201",
        start_hole_um: [-35_500, 0],
    },
    GroupPunch {
        iso_stem: "DUODI202",
        start_hole_um: [24_500, 0],
    },
];

fn plate<'a>(openings: &'a [GroupPunch<'a>]) -> PunchGroup20<'a> {
    PunchGroup20 {
        stem: "DUODI20",
        stock_min_um: [-65_000, -35_000],
        stock_max_um: [65_000, 35_000],
        height_um: 20_000,
        punches: openings,
    }
}

/// The control imported these bytes, showed `CCB030[0]` and the die side on
/// both works, and gave `CCB030[4]` and no formal error after TECCUT.
#[test]
fn die_group_files_match_the_control_checked_die_pair() {
    let files = plate(&OPENINGS).encode_die().unwrap();
    assert_eq!(
        files.sbl.as_bytes(),
        include_bytes!("../golden/agie-setup/DUODI20.SBL")
    );
    assert_eq!(
        files.sbr.as_bytes(),
        include_bytes!("../golden/agie-setup/DUODI20.SBR")
    );
    let review = &files.controller_review;
    assert!(review.contains("CUT1 DUODI201.ISO, CUT2 DUODI202.ISO"));
    assert!(review.contains("245, 155, 140 and 134 µm"));
    assert!(review.contains("Confirm c_die on each work"));
    assert!(review.contains("no cutting move crosses the plate between the openings"));
    assert!(review.contains("Support each slug"));
    assert!(!review.contains("c_punch"));
}

#[test]
fn each_opening_has_its_own_geometry_die_side_quality_and_hole() {
    let sbl = plate(&OPENINGS).encode_die().unwrap().sbl;
    for (n, iso, x) in [(1, "DUODI201", "-35.500"), (2, "DUODI202", "24.500")] {
        let work = format!("\"DUODI20\",\"CUT{n}\"");
        for field in [
            format!("{work},NULL,ID_GEOMETRY,\"AGIE.USING_{iso}.ISO\""),
            format!("{work},NULL,ID_PUNCH,c_die"),
            format!("{work},NULL,ID_QUALITY,je_q1"),
            format!("{work},NULL,ID_SEPCUT,\"1.0\""),
            format!("{work},\"stp1\",ID_POSX,\"{x}\""),
            format!("{work},\"stp1\",ID_POSY,\"0.000\""),
            format!("{work},\"stp1\",ID_DIAMETER,\"2.0\""),
        ] {
            assert_eq!(sbl.matches(&field).count(), 1, "{field}");
        }
    }
    // One CCB030 quality for the piece. Both works refer to it.
    assert_eq!(sbl.matches("JE_CreateQuality(").count(), 1);
    assert!(sbl.contains("JE_CreateQuality(je_piece,\"CCB030\","));
    assert!(!sbl.contains("c_punch"));
}

/// Apart from the side, the die group is the punch group of the same works.
#[test]
fn the_die_group_differs_from_the_punch_group_only_in_the_side() {
    let die = plate(&OPENINGS).encode_die().unwrap().sbl;
    let punch = plate(&OPENINGS).encode().unwrap().sbl;
    assert_eq!(die.matches("ID_PUNCH,c_die").count(), 2);
    assert_eq!(die.replace("ID_PUNCH,c_die", "ID_PUNCH,c_punch"), punch);
}

/// The single die job of #2957 keeps its bytes (DIE20).
#[test]
fn a_single_die_keeps_its_zero_step_setup() {
    let files = PunchSetup20 {
        stem: "DIE20",
        stock_min_um: [-35_000, -35_000],
        stock_max_um: [35_000, 35_000],
        start_hole_um: [-5_500, 0],
        height_um: 20_000,
    }
    .encode_die()
    .unwrap();
    assert_eq!(
        files.sbl.as_bytes(),
        include_bytes!("../golden/agie-setup/DIE20.SBL")
    );
}

#[test]
fn a_die_group_refuses_other_heights_counts_and_repeated_stems() {
    // Three distinct openings: the control checked die groups of two only.
    let third = GroupPunch {
        iso_stem: "DUODI203",
        ..OPENINGS[1]
    };
    let three = [OPENINGS[0], OPENINGS[1], third];
    let repeated = [OPENINGS[0], OPENINGS[0]];
    for (group, expected) in [
        (plate(&OPENINGS[..1]), SetupError::GroupWorks),
        (plate(&three), SetupError::GroupWorks),
        (plate(&repeated), SetupError::GroupWorks),
        (
            PunchGroup20 {
                height_um: 15_000,
                ..plate(&OPENINGS)
            },
            SetupError::UnqualifiedGroupHeight,
        ),
    ] {
        assert_eq!(group.encode_die().unwrap_err(), expected);
    }
    let outside = [
        OPENINGS[0],
        GroupPunch {
            iso_stem: "DUODI202",
            start_hole_um: [64_500, 0],
        },
    ];
    assert_eq!(
        plate(&outside).encode_die().unwrap_err(),
        SetupError::HoleOutsideStock
    );
}
