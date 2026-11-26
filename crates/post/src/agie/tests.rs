//! Native Agie bundle integrity, nominal seam, corner clearance and no-clobber checks on original punches.
use super::*;

pub(super) fn punch() -> AgieJob {
    AgieJob {
        side: AgieSide::Punch,
        contour: vec![[-20., -10.], [20., -10.], [20., 10.], [-20., 10.]],
        start: Some([-30., 0.]),
        stock_min: [-40., -30., -20.],
        stock_max: [40., 30., 0.],
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_punch_links_its_three_files_and_uses_the_uncompensated_contour() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("new-job");
    let paths = export(&punch(), &out, "PUNCH40", 1.0).unwrap();
    assert_eq!(paths.len(), 3);
    let iso = std::fs::read_to_string(out.join("PUNCH40.ISO")).unwrap();
    assert_eq!(iso.lines().count(), 8);
    assert!(iso.contains("G00 X-20.00000 Y0.00000 ;\r\n"));
    assert!(iso.contains("G01 X20.00000 Y10.00000 ;\r\n"));
    assert!(iso.ends_with("M02 ;\r\n"));
    let map = std::fs::read_to_string(out.join("PUNCH40.SBR")).unwrap();
    assert_eq!(map, "AGIE.USING_PUNCH40.ISO IMPORT PUNCH40.ISO;\r\n");
    let script = std::fs::read_to_string(out.join("PUNCH40.SBL")).unwrap();
    for field in [
        "AGIE.USING_PUNCH40.ISO",
        "ID_PUNCH,c_punch",
        "ID_SEPCUT,\"1.0\"",
        "ID_DIAMETER,\"2.0\"",
        "ID_POSX,\"-30.000\"",
        "0.30,1.50,1.00,F,F,F",
        "ASDELETE",
    ] {
        assert!(script.contains(field), "{field}");
    }
    assert!(!script.contains("ASAGIE"));
    assert!(export(&punch(), &out, "PUNCH40", 1.0).is_err());
    assert_eq!(
        std::fs::read_to_string(out.join("PUNCH40.ISO")).unwrap(),
        iso
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn final_rename_cannot_replace_an_empty_directory() {
    let temp = tempfile::tempdir().unwrap();
    let from = temp.path().join("stage");
    let to = temp.path().join("destination");
    std::fs::create_dir(&from).unwrap();
    std::fs::create_dir(&to).unwrap();
    assert!(rename_new(&from, &to).is_err());
    assert!(from.is_dir());
    assert!(to.is_dir());
}

#[test]
fn stock_hole_and_entry_are_validated_before_output() {
    let mut job = punch();
    job.start = Some([0., 0.]);
    assert!(contour::prepare(&job, "PUNCH40").is_err());
    job.start = Some([-20.5, 0.]);
    assert!(contour::prepare(&job, "PUNCH40").is_err());
    job.start = Some([-39.5, 0.]);
    assert!(contour::prepare(&job, "PUNCH40").is_err());
    job = punch();
    job.contour.swap(0, 1);
    assert!(contour::prepare(&job, "PUNCH40").is_err());
    job = punch();
    job.stock_min[2] = -12.;
    assert!(contour::prepare(&job, "PUNCH40").is_err());
    assert!(!valid_name("A/B"));
    assert!(!valid_name("NUL"));
}

/// The contour that `prepare` posts, without the setup half.
pub(super) fn points(job: &AgieJob) -> Result<Vec<[f64; 2]>, String> {
    contour::prepare(job, "PUNCH40").map(|(points, _)| points)
}

#[test]
fn separation_needs_three_millimetres_on_each_side_of_the_entry() {
    let mut job = punch();
    job.start = Some([-30., 9.5]);
    assert!(
        points(&job)
            .unwrap_err()
            .contains("straight face on both sides")
    );
    job.start = Some([-30., 7.]);
    assert!(points(&job).is_ok());
}

/// A five-sided convex punch with a 3:4 oblique entry face. `left_top_x`
/// moves the top-left vertex, which sets the slope of that face.
fn pentagon(left_top_x: f64) -> AgieJob {
    AgieJob {
        contour: vec![
            [-20., -10.],
            [16., -10.],
            [22., -2.],
            [18., 10.],
            [left_top_x, 10.],
        ],
        start: Some([-20.5, 6.]),
        ..punch()
    }
}

#[test]
fn oblique_punch_preserves_nominal_vertices_and_perpendicular_seam() {
    let mut job = pentagon(-5.);
    let prepared = points(&job).unwrap();
    assert_eq!(prepared[0], [-12.5, 0.]);
    assert_eq!(&prepared[1..], job.contour.as_slice());
    job.contour.reverse();
    assert_eq!(points(&job).unwrap(), prepared);

    // 3.125 mm along the 3:4 face, even though each coordinate difference
    // is smaller than 3 mm. A 2.5 mm length cannot hold the transition.
    job.start = Some([-26.125, -1.5]);
    assert_eq!(points(&job).unwrap()[0], [-18.125, -7.5]);
    job.start = Some([-26.5, -2.]);
    assert!(points(&job).is_err());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn oblique_punch_posts_the_zero_step_setup_and_its_faces() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("POLY20");
    let job = pentagon(-5.);
    let exported = export_with_review(&job, &out, "POLY20", 1.0).unwrap();
    assert_eq!(exported.files.len(), 3);
    let iso = std::fs::read_to_string(out.join("POLY20.ISO")).unwrap();
    assert_eq!(
        iso,
        "( POLY20.ISO )\r\n\
         N0001 G00 X-12.50000 Y0.00000 ;\r\n\
         N0002 G01 X-20.00000 Y-10.00000 ;\r\n\
         N0003 G01 X16.00000 Y-10.00000 ;\r\n\
         N0004 G01 X22.00000 Y-2.00000 ;\r\n\
         N0005 G01 X18.00000 Y10.00000 ;\r\n\
         N0006 G01 X-5.00000 Y10.00000 ;\r\n\
         N0007 G01 X-12.50000 Y0.00000 ;\r\n\
         N0008 M02 ;\r\n"
    );
    let script = std::fs::read_to_string(out.join("POLY20.SBL")).unwrap();
    for field in [
        "AGIE.USING_POLY20.ISO",
        "ID_SEPCUT,\"1.0\"",
        "ID_POSX,\"-20.500\"",
        "ID_POSY,\"6.000\"",
        "ASDELETE",
    ] {
        assert!(script.contains(field), "{field}");
    }
    assert!(!script.contains("ASAGIE"));
    // The oblique punch keeps the 1.0 mm separation rule of the setup.
    let refused = temp.path().join("NOSEP");
    let why = export_with_review(&job, &refused, "NOSEP", 0.0).unwrap_err();
    assert!(why.contains("1.0 mm"), "{why}");
    assert!(!refused.exists());
}

#[test]
fn oblique_seam_rounds_to_the_exterior_without_moving_nominal_vertices() {
    let mut job = pentagon(-6.);
    let prepared = points(&job).unwrap();
    assert_eq!(prepared[0], [-12.64765, 0.50336]);
    assert_eq!(&prepared[1..], job.contour.as_slice());
    job.contour.reverse();
    assert_eq!(points(&job).unwrap(), prepared);

    // Nearest-coordinate rounding here would cut inside the punch.
    job.start = job.start.map(|[x, _]| [x, 6.003]);
    assert_eq!(points(&job).unwrap()[0], [-12.64625, 0.50537]);
}

#[test]
fn outward_rounding_cannot_consume_the_required_lead_clearance() {
    let mut job = punch();
    job.contour = vec![
        [-19.99933, -10.],
        [16., -10.],
        [22., -2.],
        [18., 10.],
        [-5.99933, 10.],
    ];
    job.start = Some([-15.5, 1.659]);
    // The exact approach measures 3.000002716 mm. Every exterior seam
    // inside the ISO rounding budget leaves less than 3 mm to a new face.
    let why = points(&job).unwrap_err();
    assert!(why.contains("0.00001 mm ISO grid"), "{why}");
}

#[test]
fn acute_corner_clearance_uses_the_largest_resident_offset_at_the_selected_height() {
    let mut job = punch();
    job.contour = vec![[-20., -10.], [30., 0.], [-20., 10.]];
    // The miter at X30 grows with the rough offset. All three stock bounds
    // pass the 0.4 mm straight-face check; only the height-specific miter distinguishes them.
    for (height, right, fits) in [
        (15., 31.430, true),
        (20., 31.430, true),
        (40., 31.430, false),
        (40., 31.480, true),
        (80., 31.480, false),
        (80., 31.500, true),
    ] {
        job.stock_min[2] = -height;
        job.stock_max[0] = right;
        let result = points(&job);
        if fits {
            result.unwrap();
        } else {
            assert!(result.unwrap_err().contains("rough corner"));
        }
    }
}

#[test]
fn only_a_measured_qn2_thickness_prepares_a_punch() {
    let mut job = punch();
    for height in [12., 19.999, 20.001, 25., 39.999, 40.001, 60., 100.] {
        job.stock_min[2] = -height;
        let why = points(&job).unwrap_err();
        assert!(
            why.contains("stock thickness") && why.contains("15, 20, 40, 80"),
            "{why}"
        );
        assert!(!why.contains("Agie"), "{why}");
    }
    for height in [15., 20., 40., 80.] {
        job.stock_min[2] = -height;
        let (_, setup) = contour::prepare(&job, "PUNCH40").unwrap();
        assert_eq!(setup.height_um, height as i32 * 1000);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_measured_height_changes_only_the_setup_height_fields() {
    let temp = tempfile::tempdir().unwrap();
    let read = |files: &[std::path::PathBuf]| -> Vec<String> {
        files
            .iter()
            .map(|f| std::fs::read_to_string(f).unwrap())
            .collect()
    };
    let base = export_with_review(&punch(), &temp.path().join("H20"), "PUNCH", 1.0).unwrap();
    let base_text = read(&base.files);
    for height in [15., 40., 80.] {
        let mut job = punch();
        job.stock_min[2] = -height;
        let dest = temp.path().join(format!("H{height}"));
        let posted = export_with_review(&job, &dest, "PUNCH", 1.0).unwrap();
        let text = read(&posted.files);
        // Files are ISO, SBR, SBL. The nominal geometry does not depend on the height.
        assert_eq!(text[0], base_text[0]);
        assert_eq!(text[1], base_text[1]);
        let changed: Vec<_> = text[2]
            .lines()
            .zip(base_text[2].lines())
            .filter(|(a, b)| a != b)
            .map(|(a, _)| a)
            .collect();
        assert_eq!(changed.len(), 3, "{changed:?}");
        assert!(
            changed
                .iter()
                .all(|l| l.contains("ID_HEIGHT") || l.contains("ID_POSPOSZ"))
        );
        assert!(
            posted
                .controller_review
                .contains(&format!("and {height} mm."))
        );
    }
}

#[test]
fn an_acute_miter_needs_more_stock_than_a_round_corner() {
    let mut job = punch();
    job.contour = vec![[-20., -10.], [30., 0.], [-20., 10.]];
    job.stock_max[0] = 30.4;
    let why = points(&job).unwrap_err();
    assert!(why.contains("rough corner"), "{why}");
    job.stock_max[0] = 32.;
    assert!(points(&job).is_ok());

    job.contour = vec![[-20., -10.], [20., -10.], [0., 0.], [20., 10.], [-20., 10.]];
    // The (20, -10) - (0, 0) - (20, 10) notch opens at 53 degrees: the rough
    // offset cannot reach its reentrant vertex within 0.002 mm.
    let why = points(&job).unwrap_err();
    assert!(why.contains("reentrant corner"), "{why}");
}

#[test]
fn every_face_keeps_the_straight_stock_margin() {
    // The rectangle refusal of main stays: a face closer than 0.4 mm to the stock.
    let mut job = punch();
    job.stock_max[1] = 10.3;
    let why = points(&job).unwrap_err();
    assert!(why.contains("0.4 mm of stock"), "{why}");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn stock_origin_and_fractional_setup_coordinates_are_refused_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("REFUSED");
    let mut displaced = punch();
    for vertex in &mut displaced.contour {
        vertex[0] += 340.0;
    }
    displaced.start = displaced.start.map(|[x, y]| [x + 340.0, y]);
    displaced.stock_min[0] += 340.0;
    displaced.stock_max[0] += 340.0;
    let mut fractional = punch();
    fractional.start = fractional.start.map(|[x, _]| [x, 0.0004]);
    for (job, why) in [
        (displaced, "stock and work origin"),
        (fractional, "0.001 mm"),
    ] {
        let error = export_with_review(&job, &out, "REFUSED", 1.0).unwrap_err();
        assert!(error.contains(why), "{error}");
        assert!(!out.exists());
    }
}
