//! Native inspection contour and bundle tests using original synthetic arcs.
//! Gate map (M4.4): covers post::agie_setup::inspection, post::agie_setup::publish.
use camcraft_geometry::dxf::Primitive;
use camcraft_geometry::geo2d::Point2;
use camcraft_post::agie_setup::{InspectionProfile, InspectionSegment};
use std::f64::consts::PI;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn notched() -> Vec<InspectionSegment> {
    let points = [
        p(-9.0, -6.0),
        p(9.0, -6.0),
        p(9.0, 4.0),
        p(2.0, 4.0),
        p(-2.0, 4.0),
        p(-9.0, 4.0),
    ];
    points
        .iter()
        .enumerate()
        .map(|(i, &start)| InspectionSegment {
            start,
            end: points[(i + 1) % points.len()],
            arc: (i == 3).then_some((p(0.0, 4.0), -PI)),
        })
        .collect()
}

#[test]
fn native_inspection_writes_one_closed_nominal_loop_and_zero_steps() {
    let mut profile = InspectionProfile::new(&notched(), p(0.0, -1.0)).unwrap();
    profile
        .check_other(
            1,
            &Primitive::Circle {
                center: p(0.0, 0.0),
                radius: 18.0,
            },
        )
        .unwrap();
    profile
        .check_other(
            2,
            &Primitive::Circle {
                center: p(13.0, 0.0),
                radius: 1.0,
            },
        )
        .unwrap();
    assert!(
        profile
            .check_other(
                3,
                &Primitive::Line {
                    start: p(-15.0, 0.0),
                    end: p(15.0, 0.0)
                }
            )
            .is_err()
    );
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("NOMINAL");
    let files = profile.export(&dest).unwrap();
    assert_eq!(files.len(), 3);
    let iso = std::fs::read_to_string(&files[0]).unwrap();
    assert!(iso.contains("G02 X-2.00000 Y4.00000 I-2.00000 J0.00000 ;\r\n"));
    assert!(iso.ends_with("N0008 M02 ;\r\n"));
    let sbl = std::fs::read_to_string(&files[2]).unwrap();
    assert_eq!(sbl.matches("JE_GenerateCuts(").count(), 1);
    for word in [
        "ASDELETE",
        "ID_PUNCH,c_left",
        "ID_DIAMETER,\"3.0\"",
        "ID_SEPCUT,\"0\"",
        "ID_ENTRYMODE,c_entper",
        "ID_DIML,\"0\"",
        "ID_DIMB,\"0\"",
    ] {
        assert!(sbl.contains(word), "{word}");
    }
    let receipt = camcraft_post::agie_setup::INSPECTION_RECEIPT;
    assert!(receipt.contains("perpendicular controller lead default"));
    assert!(receipt.contains("physical stock Z"));
    assert!(receipt.contains("travel without stated limits are unverified"));
    assert!(!sbl.contains("ASCREATE"));
    std::fs::write(&files[0], "KEEP").unwrap();
    assert!(profile.export(&dest).is_err());
    assert_eq!(std::fs::read_to_string(&files[0]).unwrap(), "KEEP");
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
}

#[test]
fn native_inspection_refuses_unverified_geometry_before_publication() {
    for hole in [p(0.0, 1.0), p(9.0, 0.0), p(-24.0, 0.0), p(f64::NAN, 0.0)] {
        assert!(InspectionProfile::new(&notched(), hole).is_err());
    }
    let mut spans = notched();
    spans[1].start.x += 0.001;
    assert!(InspectionProfile::new(&spans, p(0.0, -1.0)).is_err());
    let tmp = tempfile::tempdir().unwrap();
    let profile = InspectionProfile::new(&notched(), p(0.0, -1.0)).unwrap();
    let bad_name = tmp.path().join("TOO-LONG-NAME");
    assert!(profile.export(&bad_name).is_err());
    assert!(!bad_name.exists());
}
