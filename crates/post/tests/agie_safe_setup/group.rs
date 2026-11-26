//! Setup-only punch group records from CamCraft's offline controller-checked synthetic punch pair (#2961).
//! Groups of three and nine punches from the CLI examples `punch-trio` and `punch-grid` (#2980).
use camcraft_post::agie_setup::{
    GroupPunch, MAX_PUNCH_WORKS, PunchGroup20, PunchSetup20, SetupError,
};

const TEN_STEMS: [&str; 10] = ["W1", "W2", "W3", "W4", "W5", "W6", "W7", "W8", "W9", "W10"];

/// The group `stem` of the examples `punch-trio` and `punch-grid`: one
/// threading hole 13 mm left of each centre, on 120 × 100 × 20 mm stock.
fn grid_files(stem: &str, centers: &[[i32; 2]]) -> camcraft_post::agie_setup::SetupFiles {
    let stems: Vec<String> = (1..=centers.len()).map(|n| format!("{stem}{n}")).collect();
    let punches: Vec<_> = stems
        .iter()
        .zip(centers)
        .map(|(iso_stem, [x, y])| GroupPunch {
            iso_stem,
            start_hole_um: [(x - 13) * 1000, y * 1000],
        })
        .collect();
    PunchGroup20 {
        stem,
        stock_min_um: [-60_000, -50_000],
        stock_max_um: [60_000, 50_000],
        height_um: 20_000,
        punches: &punches,
    }
    .encode()
    .unwrap()
}

#[test]
fn trio_and_grid_files_match_their_goldens_with_one_quality() {
    let trio = grid_files("TRIO20", &[[-32, 0], [0, 0], [32, 0]]);
    assert_eq!(
        trio.sbl.as_bytes(),
        include_bytes!("../golden/agie-setup/TRIO20.SBL")
    );
    assert_eq!(
        trio.sbr.as_bytes(),
        include_bytes!("../golden/agie-setup/TRIO20.SBR")
    );
    let centers: Vec<[i32; 2]> = [-32, 0, 32]
        .into_iter()
        .flat_map(|y| [-32, 0, 32].map(|x| [x, y]))
        .collect();
    assert_eq!(centers.len(), MAX_PUNCH_WORKS);
    let grid = grid_files("NINE20", &centers);
    assert_eq!(
        grid.sbl.as_bytes(),
        include_bytes!("../golden/agie-setup/NINE20.SBL")
    );
    assert_eq!(
        grid.sbr.as_bytes(),
        include_bytes!("../golden/agie-setup/NINE20.SBR")
    );
    for (files, count) in [(&trio, 3), (&grid, 9)] {
        // One quality object for the piece; each work refers to it (#2984).
        assert_eq!(files.sbl.matches("JE_CreateQuality(").count(), 1);
        assert_eq!(files.sbl.matches("ID_QUALITY,je_q1").count(), count);
        assert_eq!(files.sbl.matches("ASDELETE").count(), count);
        assert_eq!(files.sbr.lines().count(), count);
    }
    assert!(grid.controller_review.contains("CUT9 NINE209.ISO"));
}

const PUNCHES: [GroupPunch<'static>; 2] = [
    GroupPunch {
        iso_stem: "PAIR201",
        start_hole_um: [-30_000, 0],
    },
    GroupPunch {
        iso_stem: "PAIR202",
        start_hole_um: [1_000, 0],
    },
];

fn pair<'a>(punches: &'a [GroupPunch<'a>]) -> PunchGroup20<'a> {
    PunchGroup20 {
        stem: "PAIR20",
        stock_min_um: [-40_000, -30_000],
        stock_max_um: [40_000, 30_000],
        height_um: 20_000,
        punches,
    }
}

#[test]
fn group_files_match_the_zero_step_punch_pair() {
    let files = pair(&PUNCHES).encode().unwrap();
    assert_eq!(
        files.sbl.as_bytes(),
        include_bytes!("../golden/agie-setup/PAIR20.SBL")
    );
    assert_eq!(
        files.sbr.as_bytes(),
        include_bytes!("../golden/agie-setup/PAIR20.SBR")
    );
    let cuts: Vec<_> = files
        .sbl
        .lines()
        .filter(|l| l.contains("JE_GenerateCuts("))
        .collect();
    assert_eq!(
        cuts,
        [
            "  ok=JE_GenerateCuts(WORK,je_piece,\"PAIR20\",\"CUT1\",ASDELETE,NULL)",
            "  ok=JE_GenerateCuts(WORK,je_piece,\"PAIR20\",\"CUT2\",ASDELETE,NULL)",
        ]
    );
    assert!(files.controller_review.contains("245, 155, 140 and 134 µm"));
    assert!(
        files
            .controller_review
            .contains("CUT1 PAIR201.ISO, CUT2 PAIR202.ISO")
    );
}

#[test]
fn each_work_has_its_own_geometry_side_and_hole() {
    let sbl = pair(&PUNCHES).encode().unwrap().sbl;
    for (n, iso, x) in [(1, "PAIR201", "-30.000"), (2, "PAIR202", "1.000")] {
        let work = format!("\"PAIR20\",\"CUT{n}\"");
        for field in [
            format!("{work},NULL,ID_GEOMETRY,\"AGIE.USING_{iso}.ISO\""),
            format!("{work},NULL,ID_PUNCH,c_punch"),
            format!("{work},NULL,ID_QUALITY,je_q1"),
            format!("{work},NULL,ID_STP,\"stp1\""),
            format!("{work},\"stp1\",ID_POSX,\"{x}\""),
            format!("{work},\"stp1\",ID_POSY,\"0.000\""),
            format!("{work},\"stp1\",ID_DIAMETER,\"2.0\""),
        ] {
            assert_eq!(sbl.matches(&field).count(), 1, "{field}");
        }
    }
    // One quality and one group for the piece. Each work refers to the quality.
    assert_eq!(sbl.matches("JE_CreateQuality(").count(), 1);
    assert_eq!(sbl.matches("JE_Create(GROUP,").count(), 1);
}

/// The lines of one work, from its `JE_Create(WORK` to its `ASDELETE` call.
fn work_block<'a>(sbl: &'a str, work: &str) -> Vec<&'a str> {
    let lines: Vec<_> = sbl.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.contains(&format!("JE_Create(WORK,je_piece,\"PAIR20\",\"{work}\")")))
        .unwrap();
    let end = lines
        .iter()
        .position(|l| l.contains(&format!("\"{work}\",ASDELETE")))
        .unwrap();
    lines[start..=end].to_vec()
}

#[test]
fn each_work_of_a_group_has_the_records_of_a_single_punch() {
    let group = pair(&PUNCHES).encode().unwrap().sbl;
    let single = PunchSetup20 {
        stem: "PAIR20",
        stock_min_um: [-40_000, -30_000],
        stock_max_um: [40_000, 30_000],
        start_hole_um: [-30_000, 0],
        height_um: 20_000,
    }
    .encode()
    .unwrap()
    .sbl
    .replace("AGIE.USING_PAIR20.ISO", "AGIE.USING_PAIR201.ISO");
    let cut1 = work_block(&group, "CUT1");
    assert_eq!(cut1, work_block(&single, "CUT1"));
    let cut2: Vec<String> = cut1
        .iter()
        .map(|l| {
            l.replace("\"CUT1\"", "\"CUT2\"")
                .replace("PAIR201", "PAIR202")
                .replace("\"-30.000\"", "\"1.000\"")
        })
        .collect();
    assert_eq!(work_block(&group, "CUT2"), cut2);
    // The piece records before the first work, and the end after the last, are those of one punch.
    let head = |sbl: &str| sbl.split("  ok=JE_Create(WORK,").next().unwrap().to_owned();
    assert_eq!(head(&group), head(&single));
    let tail = |sbl: &str| sbl.rsplit("ASDELETE,NULL)").next().unwrap().to_owned();
    assert_eq!(tail(&group), tail(&single));
}

#[test]
fn a_group_refuses_its_unqualified_inputs() {
    // Ten distinct stems: one work above the largest control-checked group.
    let ten: Vec<_> = TEN_STEMS
        .iter()
        .zip(-5..)
        .map(|(iso_stem, i)| GroupPunch {
            iso_stem,
            start_hole_um: [i * 7_000, 0],
        })
        .collect();
    let repeated = [GroupPunch { ..PUNCHES[0] }, GroupPunch { ..PUNCHES[0] }];
    let reserved = [
        GroupPunch { ..PUNCHES[0] },
        GroupPunch {
            iso_stem: "LPT1",
            ..PUNCHES[1]
        },
    ];
    let outside = [
        GroupPunch { ..PUNCHES[0] },
        GroupPunch {
            start_hole_um: [39_500, 0],
            ..PUNCHES[1]
        },
    ];
    for (setup, expected) in [
        (pair(&PUNCHES[..1]), SetupError::GroupWorks),
        (pair(&ten), SetupError::GroupWorks),
        (pair(&repeated), SetupError::GroupWorks),
        (pair(&reserved), SetupError::InvalidStem),
        (pair(&outside), SetupError::HoleOutsideStock),
        (
            PunchGroup20 {
                stem: "CON",
                ..pair(&PUNCHES)
            },
            SetupError::InvalidStem,
        ),
        (
            PunchGroup20 {
                height_um: 40_000,
                ..pair(&PUNCHES)
            },
            SetupError::UnqualifiedGroupHeight,
        ),
        (
            PunchGroup20 {
                height_um: 15_000,
                ..pair(&PUNCHES)
            },
            SetupError::UnqualifiedGroupHeight,
        ),
        (
            PunchGroup20 {
                stock_max_um: [400_000, 30_000],
                ..pair(&PUNCHES)
            },
            SetupError::InvalidStock,
        ),
    ] {
        assert_eq!(setup.encode().unwrap_err(), expected, "{setup:?}");
    }
}
