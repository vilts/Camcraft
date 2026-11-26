//! Open-cut geometry, separation and zero-step setup checks (#2962).
use super::tests::punch;
use super::*;

/// The `examples/agie/cutoff-lower.camscript` job: 80 × 60 × 20 mm stock, cut along Y−6.
fn lower() -> AgieJob {
    AgieJob {
        side: AgieSide::Left,
        contour: vec![[-40., -6.], [40., -6.]],
        start: None,
        stock_min: [-40., -30., -20.],
        stock_max: [40., 30., 0.],
    }
}

#[test]
fn the_open_cut_includes_air_tails_wire_and_work_origin_in_travel() {
    let mut job = punch();
    job.side = AgieSide::Right;
    job.contour = vec![[6., -120.], [6., 120.]];
    job.stock_min[1] = -120.;
    job.stock_max[1] = 120.;
    job.start = None;
    let (geometry, setup) = cutoff::prepare(&job, "CUT20").unwrap();
    assert_eq!(geometry.line, [[6., -124.], [6., 124.]]);
    assert_eq!(setup.entry_um, [9_000, -124_000]);
    assert_eq!(setup.exit_um, [9_000, 124_000]);
    // A 242 mm bar fits the 250 mm Y stroke. The two 4 mm tails
    // would put the wire centre on its travel limits.
    job.stock_min[1] = -121.;
    job.stock_max[1] = 121.;
    job.contour = vec![[6., -121.], [6., 121.]];
    assert!(
        cutoff::prepare(&job, "CUT20")
            .err()
            .unwrap()
            .contains("travel")
    );
    job.stock_min[1] = -120.;
    job.stock_max[1] = 120.;
    job.contour = vec![[39., -120.], [39., 120.]];
    assert!(
        cutoff::prepare(&job, "CUT20")
            .err()
            .unwrap()
            .contains("corners")
    );
    // A displaced WCS can put the controller's travel-to-origin outside the stroke.
    job.stock_min[0] = 180.;
    job.stock_max[0] = 250.;
    job.contour = vec![[210., -120.], [210., 120.]];
    assert!(
        cutoff::prepare(&job, "CUT20")
            .err()
            .unwrap()
            .contains("origin")
    );
}

#[test]
fn left_and_right_put_the_approach_on_the_waste_side_of_the_directed_line() {
    let (left, setup) = cutoff::prepare(&lower(), "LOWER20").unwrap();
    assert_eq!(left.line, [[-44., -6.], [44., -6.]]);
    assert_eq!(setup.entry_um, [-44_000, -3_000]);
    assert_eq!(setup.exit_um, [44_000, -3_000]);
    assert_eq!(setup.side, crate::agie_setup::CutoffSide::Left);
    let right = AgieJob {
        side: AgieSide::Right,
        contour: vec![[-40., 6.], [40., 6.]],
        ..lower()
    };
    let (cut, setup) = cutoff::prepare(&right, "UPPER20").unwrap();
    assert_eq!(cut.line, [[-44., 6.], [44., 6.]]);
    assert_eq!(setup.entry_um, [-44_000, 3_000]);
    assert_eq!(setup.exit_um, [44_000, 3_000]);
    assert_eq!(setup.side, crate::agie_setup::CutoffSide::Right);
    // A start hole, another thickness or a closed side is not an open cut.
    for job in [
        AgieJob {
            start: Some([0., 0.]),
            ..lower()
        },
        AgieJob {
            stock_min: [-40., -30., -40.],
            ..lower()
        },
        AgieJob {
            side: AgieSide::Punch,
            ..lower()
        },
    ] {
        assert!(cutoff::prepare(&job, "LOWER20").is_err());
    }
}

#[test]
fn an_open_cut_posts_zero_steps_and_no_separation_and_a_closed_cut_keeps_one_millimetre() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("LOWER20");
    let posted = export_with_review(&lower(), &out, "LOWER20", 0.0).unwrap();
    let iso = std::fs::read_to_string(out.join("LOWER20.ISO")).unwrap();
    assert_eq!(
        iso,
        "( LOWER20.ISO )\r\nN0001 G00 X-44.00000 Y-6.00000 ;\r\nN0002 G01 X44.00000 Y-6.00000 ;\r\nN0003 M02 ;\r\n"
    );
    let sbl = std::fs::read_to_string(out.join("LOWER20.SBL")).unwrap();
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
        assert!(sbl.contains(field), "{field}");
    }
    assert!(!sbl.contains("ID_ENTRY,") && !sbl.contains("ASAGIE"));
    assert!(posted.controller_review.contains("Confirm the c_left side"));
    // The open cut refuses a separation. The closed contour still refuses zero.
    let refused = temp.path().join("SEP");
    let why = export_with_review(&lower(), &refused, "SEP", 1.0).unwrap_err();
    assert!(why.contains("zero for an open cutoff"), "{why}");
    let why = export_with_review(&punch(), &refused, "SEP", 0.0).unwrap_err();
    assert!(why.contains("1.0 mm for this closed contour"), "{why}");
    assert!(!refused.exists());
}
