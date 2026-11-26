//! Die groups (#2977): two through-openings of one retained plate, each with its own ISO, die
//! side and hole, and the clear band of plate between them.
//! Provenance: CamCraft's original faceted slots of `examples/agie/die-pair.obj`.
use super::die_tests::slot;
use super::*;

/// The slot of `die_tests` centred at X`center`, threaded 5.5 mm left of
/// its centre, on the 130 × 70 mm stock of the die pair.
pub(super) fn opening(center: f64) -> AgieJob {
    AgieJob {
        side: AgieSide::Die,
        contour: slot().into_iter().map(|[x, y]| [x + center, y]).collect(),
        start: Some([center - 5.5, 0.]),
        stock_min: [-65., -35., -20.],
        stock_max: [65., 35., 0.],
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_die_pair_writes_an_iso_a_die_work_and_a_hole_for_each_opening() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("DUODI20");
    let jobs = [opening(-30.), opening(30.)];
    let job = export_group_with_review(&jobs, &out, "DUODI20", 1.0).unwrap();
    let names: Vec<_> = job
        .files
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        ["DUODI201.ISO", "DUODI202.ISO", "DUODI20.SBR", "DUODI20.SBL"]
    );
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 4);
    // The control-checked setup of the die pair (camcraft-post agie_safe_setup::die_group).
    let sbl = std::fs::read(out.join("DUODI20.SBL")).unwrap();
    assert_eq!(
        sbl,
        include_bytes!("../../tests/golden/agie-setup/DUODI20.SBL")
    );
    let sbr = std::fs::read(out.join("DUODI20.SBR")).unwrap();
    assert_eq!(
        sbr,
        include_bytes!("../../tests/golden/agie-setup/DUODI20.SBR")
    );
    // Each ISO enters its own left wall from inside its own opening.
    for (n, wall) in [(1, "-42.50000"), (2, "17.50000")] {
        let iso = std::fs::read_to_string(out.join(format!("DUODI20{n}.ISO"))).unwrap();
        assert!(iso.starts_with(&format!(
            "( DUODI20{n}.ISO )\r\nN0001 G00 X{wall} Y0.00000 ;\r\n"
        )));
        assert!(iso.ends_with(&format!("N0040 G01 X{wall} Y0.00000 ;\r\nN0041 M02 ;\r\n")));
    }
    assert!(job.controller_review.contains("Confirm c_die on each work"));
    assert!(job.controller_review.contains("Support each slug"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_single_die_through_the_group_export_keeps_its_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let die = AgieJob {
        stock_min: [-35., -35., -20.],
        stock_max: [35., 35., 0.],
        ..opening(0.)
    };
    let one = export_group_with_review(
        std::slice::from_ref(&die),
        &temp.path().join("A"),
        "DIE20",
        1.0,
    )
    .unwrap();
    let two = export_with_review(&die, &temp.path().join("B"), "DIE20", 1.0).unwrap();
    assert_eq!(one.controller_review, two.controller_review);
    for name in ["DIE20.ISO", "DIE20.SBR", "DIE20.SBL"] {
        let a = std::fs::read(temp.path().join("A").join(name)).unwrap();
        let b = std::fs::read(temp.path().join("B").join(name)).unwrap();
        assert_eq!(a, b, "{name}");
    }
    assert_eq!(
        std::fs::read(temp.path().join("A/DIE20.SBL")).unwrap(),
        include_bytes!("../../tests/golden/agie-setup/DIE20.SBL")
    );
}

#[test]
fn the_same_opening_twice_or_a_hole_in_the_other_opening_writes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let mut pierce = opening(30.);
    pierce.start = Some([-35.5, 0.]);
    for (jobs, expected) in [
        ([opening(-30.), opening(-30.)], "clear band"),
        ([opening(-30.), pierce], "inside the die opening"),
    ] {
        let dir = temp.path().join("DUODI20");
        let why = export_group_with_review(&jobs, &dir, "DUODI20", 1.0).unwrap_err();
        assert!(why.contains(expected), "{why}");
        assert!(!dir.exists());
    }
}

/// The band of plate between the openings must clear both cutting areas:
/// each opening, its hole and lead, the rough corner and the wire.
#[test]
fn the_band_between_openings_includes_the_lead_rough_corner_and_wire() {
    let temp = tempfile::tempdir().unwrap();
    // The walls X-17.5 and X(center - 12.5) leave 7 mm of plate at X2 and
    // 6.5 mm at X1.5. Two 3 mm leads, two rough corners and the wire close the band.
    for (center, ok) in [(2.0, true), (1.5, false), (-5.0, false)] {
        let dir = temp
            .path()
            .join(format!("D{}", (center * 10.0) as i32 + 100));
        let result =
            export_group_with_review(&[opening(-30.), opening(center)], &dir, "DUODI20", 1.0);
        assert_eq!(result.is_ok(), ok, "X{center}: {:?}", result.err());
        if !ok {
            assert!(result.unwrap_err().contains("clear band"), "X{center}");
            assert!(!dir.exists());
        }
    }
}

#[test]
fn a_die_group_refuses_another_height_or_no_separation() {
    let temp = tempfile::tempdir().unwrap();
    let thick = |center| AgieJob {
        stock_min: [-65., -35., -40.],
        ..opening(center)
    };
    for (jobs, separation, expected) in [
        ([thick(-30.), thick(30.)], 1.0, "20 mm"),
        ([opening(-30.), opening(30.)], 0.0, "1.0 mm"),
    ] {
        let dir = temp.path().join("DUODI20");
        let why = export_group_with_review(&jobs, &dir, "DUODI20", separation).unwrap_err();
        assert!(why.contains(expected), "{why}");
        assert!(!dir.exists());
    }
}
