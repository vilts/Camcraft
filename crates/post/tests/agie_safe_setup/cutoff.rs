//! Setup-only Classic Cut 2S open cut from CamCraft's offline controller-checked synthetic cutoff (#2962).
use camcraft_post::agie_setup::{CutoffSetup20, CutoffSide, PunchSetup20, SetupError};

/// The job of `examples/agie/cutoff-lower.camscript`: 80 × 60 × 20 mm stock, line Y−6.
fn lower() -> CutoffSetup20<'static> {
    CutoffSetup20 {
        stem: "LOWER20",
        stock_min_um: [-40_000, -30_000],
        stock_max_um: [40_000, 30_000],
        side: CutoffSide::Left,
        entry_um: [-44_000, -3_000],
        exit_um: [44_000, -3_000],
        height_um: 20_000,
    }
}

#[test]
fn cutoff_setup_matches_the_control_checked_zero_step_open_cut() {
    let files = lower().encode().unwrap();
    assert_eq!(
        files.sbl.as_bytes(),
        include_bytes!("../golden/agie-setup/LOWER20.SBL")
    );
    assert_eq!(files.sbr, "AGIE.USING_LOWER20.ISO IMPORT LOWER20.ISO;\r\n");
    let cuts: Vec<_> = files
        .sbl
        .lines()
        .filter(|l| l.contains("JE_GenerateCuts("))
        .collect();
    assert_eq!(
        cuts,
        ["  ok=JE_GenerateCuts(WORK,je_piece,\"LOWER20\",\"CUT1\",ASDELETE,NULL)"]
    );
    for field in [
        "ID_PUNCH,c_left,",
        "ID_SEPCUT,\"0.0\",",
        "ID_EXITMODE,c_entper,",
        "ID_STPENTRY,\"stp1\",",
        "ID_STPOPEN,c_work,",
        "ID_STPEXIT,\"stp2\",",
        "ID_STPOPEX,c_work,",
        "\"stp1\",ID_POSX,\"-44.000\",",
        "\"stp1\",ID_POSY,\"-3.000\",",
        "\"stp2\",ID_POSX,\"44.000\",",
        "\"stp2\",ID_POSY,\"-3.000\",",
    ] {
        assert!(files.sbl.contains(field), "{field}");
    }
    // No hole entry type, no closed-cycle start fields, no separation.
    for absent in [
        "ID_ENTRY,",
        "ID_STP,",
        "ID_STPMODE,",
        "ID_SEPCUT,\"1.0\"",
        "ASAGIE",
    ] {
        assert!(!files.sbl.contains(absent), "{absent}");
    }
    assert!(
        files
            .controller_review
            .contains("entry point and exit point to the setup")
    );
    assert!(
        files
            .controller_review
            .contains("Check offsets of 245, 155, 140 and 134 µm")
    );
    assert!(files.controller_review.ends_with(
        " Confirm the c_left side, no separation cut, the entry stp1 and the exit stp2 in air, \
         and one stock crossing for each pass. Keep both pieces clamped through all passes."
    ));
}

#[test]
fn right_is_the_left_setup_with_the_other_side() {
    let left = lower().encode().unwrap();
    let right = CutoffSetup20 {
        side: CutoffSide::Right,
        ..lower()
    }
    .encode()
    .unwrap();
    assert_eq!(
        right.sbl.replace("ID_PUNCH,c_right", "ID_PUNCH,c_left"),
        left.sbl
    );
    assert!(right.controller_review.contains("Confirm the c_right side"));
}

#[test]
fn the_open_cut_shares_the_closed_setup_but_for_its_endpoints() {
    // The same stock as a punch with a hole: only the separation, the
    // exit mode, the start keys, the side and the second point differ.
    let open = lower().encode().unwrap().sbl;
    let closed = PunchSetup20 {
        stem: "LOWER20",
        stock_min_um: [-40_000, -30_000],
        stock_max_um: [40_000, 30_000],
        start_hole_um: [-30_000, 0],
        height_um: 20_000,
    }
    .encode()
    .unwrap()
    .sbl;
    let common = |sbl: &str| -> Vec<String> {
        sbl.lines()
            .filter(|l| {
                !l.contains("stp")
                    && !l.contains("ID_STP")
                    && !l.contains("ID_SEPCUT")
                    && !l.contains("ID_PUNCH")
            })
            .filter(|l| !l.contains("ID_ENTRY,") && !l.contains("ID_EXITMODE"))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(common(&open), common(&closed));
}

#[test]
fn endpoints_in_stock_on_one_side_or_out_of_travel_write_no_setup() {
    for (entry, exit) in [
        ([-44_000, -3_000], [40_000, -3_000]),  // exit on the face
        ([-44_000, -3_000], [0, -3_000]),       // exit in the stock
        ([-44_000, -3_000], [-44_000, 3_000]),  // both on one side
        ([-44_000, -3_000], [310_000, -3_000]), // past the 350 mm stroke
    ] {
        let setup = CutoffSetup20 {
            entry_um: entry,
            exit_um: exit,
            ..lower()
        };
        assert_eq!(
            setup.encode().unwrap_err(),
            SetupError::CutoffEndpoints,
            "{entry:?} {exit:?}"
        );
    }
    for mm in [15, 40, 80] {
        let setup = CutoffSetup20 {
            height_um: mm * 1000,
            ..lower()
        };
        assert_eq!(
            setup.encode().unwrap_err(),
            SetupError::UnqualifiedCutoffHeight
        );
    }
    let bad = CutoffSetup20 {
        stem: "lower",
        ..lower()
    };
    assert_eq!(bad.encode().unwrap_err(), SetupError::InvalidStem);
}
