//! Reentrant punch corners on original notched punches: the rough reach, the
//! join spacing, the exterior entry and the zero-step setup at each height.
use super::tests::{points, punch};
use super::*;
use crate::agie_setup::QN2_OFFSETS_UM;

/// The 40 × 20 mm punch with a 20 mm wide V notch in its top face. The notch
/// tip is `depth` mm below the top face, at X0.
fn notched(depth: f64) -> AgieJob {
    let mut job = punch();
    job.contour
        .splice(3..3, [[10., 10.], [0., 10. - depth], [-10., 10.]]);
    job
}

#[test]
fn a_broad_notch_keeps_its_nominal_corner_and_requires_an_exterior_entry() {
    let mut job = notched(1.);
    let prepared = points(&job).unwrap();
    assert_eq!(prepared[0], [-20., 0.]);
    assert_eq!(&prepared[1..], job.contour.as_slice());
    // The drilled hole cannot sit in the retained right-hand shoulder.
    job.start = Some([15., 9.7]);
    assert!(points(&job).unwrap_err().contains("start hole"));
    // This hole is outside, but the extension of the locally visible sloping
    // face crosses the other bank. Entry requires a supporting exterior face.
    job.start = Some([5., 16.]);
    assert!(points(&job).unwrap_err().contains("start hole"));
}

#[test]
fn a_gentle_but_narrow_notch_cannot_hold_the_rough_corner_joins() {
    let mut job = punch();
    job.contour
        .splice(3..3, [[0.4, 10.], [0., 9.98], [-0.4, 10.]]);
    let why = points(&job).unwrap_err();
    assert!(why.contains("segment"), "{why}");
}

#[test]
fn the_rough_offset_is_the_largest_resident_offset_at_every_measured_height() {
    // The reentrant reach and the feature spacing use the rough offset as the
    // bound for all four passes. That is only true while it is the largest.
    for (mm, offsets) in QN2_OFFSETS_UM {
        assert_eq!(offsets.iter().max(), Some(&offsets[0]), "{mm} mm");
    }
}

#[test]
fn reentrant_corner_reach_uses_the_rough_offset_of_the_selected_thickness() {
    // At a 1.251 mm deep notch the miter of the rough offset passes the
    // nominal tip by 0.00191 mm at 245 µm (15 and 20 mm). At 260 µm (40 mm)
    // and 266 µm (80 mm) it passes by more than the 0.002 mm limit.
    let mut job = notched(1.251);
    for (height, fits) in [(15., true), (20., true), (40., false), (80., false)] {
        job.stock_min[2] = -height;
        let result = points(&job);
        if fits {
            assert_eq!(&result.unwrap()[1..], job.contour.as_slice());
        } else {
            let why = result.unwrap_err();
            assert!(why.contains("reentrant corner"), "{height} mm: {why}");
            assert!(!why.contains("Agie"), "{why}");
        }
    }
    // The 1 mm notch passes at every measured height.
    let mut job = notched(1.);
    for (mm, _) in QN2_OFFSETS_UM {
        job.stock_min[2] = -f64::from(mm);
        points(&job).unwrap();
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn notched_punch_posts_the_zero_step_setup_and_its_nominal_notch() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("NOTCH20");
    let job = notched(1.);
    let exported = export_with_review(&job, &out, "NOTCH20", 1.0).unwrap();
    assert_eq!(exported.files.len(), 3);
    let iso = std::fs::read_to_string(out.join("NOTCH20.ISO")).unwrap();
    assert_eq!(
        iso,
        "( NOTCH20.ISO )\r\n\
         N0001 G00 X-20.00000 Y0.00000 ;\r\n\
         N0002 G01 X-20.00000 Y-10.00000 ;\r\n\
         N0003 G01 X20.00000 Y-10.00000 ;\r\n\
         N0004 G01 X20.00000 Y10.00000 ;\r\n\
         N0005 G01 X10.00000 Y10.00000 ;\r\n\
         N0006 G01 X0.00000 Y9.00000 ;\r\n\
         N0007 G01 X-10.00000 Y10.00000 ;\r\n\
         N0008 G01 X-20.00000 Y10.00000 ;\r\n\
         N0009 G01 X-20.00000 Y0.00000 ;\r\n\
         N0010 M02 ;\r\n"
    );
    // The setup is the same zero-step punch setup as for a convex punch.
    let script = std::fs::read_to_string(out.join("NOTCH20.SBL")).unwrap();
    let convex = temp.path().join("CONVEX");
    export_with_review(&punch(), &convex, "NOTCH20", 1.0).unwrap();
    assert_eq!(
        script,
        std::fs::read_to_string(convex.join("NOTCH20.SBL")).unwrap()
    );
    for field in ["ID_PUNCH,c_punch", "ID_SEPCUT,\"1.0\"", "ASDELETE"] {
        assert!(script.contains(field), "{field}");
    }
    assert!(!script.contains("ASAGIE"));
    assert!(exported.controller_review.contains("245, 155, 140 and 134"));
    // The notched punch keeps the 1.0 mm separation rule of the setup.
    let refused = temp.path().join("NOSEP");
    let why = export_with_review(&job, &refused, "NOSEP", 0.0).unwrap_err();
    assert!(why.contains("1.0 mm"), "{why}");
    assert!(!refused.exists());
}
