//! Punch groups (#2961): two works, their own ISO and hole, and the clear band between them.
//! Provenance: CamCraft's original synthetic rectangular and five-sided punches.
use super::tests::punch;
use super::*;

fn left() -> AgieJob {
    AgieJob {
        contour: vec![[-24., -8.], [-10., -8.], [-10., 8.], [-24., 8.]],
        start: Some([-30., 0.]),
        ..punch()
    }
}

fn right(start_x: f64) -> AgieJob {
    AgieJob {
        contour: vec![[7., -7.], [20., -7.], [24., -2.], [24., 9.], [7., 9.]],
        start: Some([start_x, 0.]),
        ..punch()
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_punch_pair_writes_an_iso_a_resource_line_a_work_and_a_hole_for_each_punch() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("PAIR20");
    let job = export_group_with_review(&[left(), right(1.)], &out, "PAIR20", 1.0).unwrap();
    let names: Vec<_> = job
        .files
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        ["PAIR201.ISO", "PAIR202.ISO", "PAIR20.SBR", "PAIR20.SBL"]
    );
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 4);
    let map = std::fs::read_to_string(out.join("PAIR20.SBR")).unwrap();
    assert_eq!(
        map,
        "AGIE.USING_PAIR201.ISO IMPORT PAIR201.ISO;\r\nAGIE.USING_PAIR202.ISO IMPORT PAIR202.ISO;\r\n"
    );
    let sbl = std::fs::read_to_string(out.join("PAIR20.SBL")).unwrap();
    assert_eq!(sbl.matches("JE_CreateQuality(").count(), 1);
    assert_eq!(sbl.matches("JE_Create(GROUP,").count(), 1);
    assert_eq!(sbl.matches("ID_QUALITY,je_q1").count(), 2);
    assert_eq!(sbl.matches("ID_PUNCH,c_punch").count(), 2);
    assert_eq!(sbl.matches("ID_SEPCUT,\"1.0\"").count(), 2);
    assert_eq!(sbl.matches("ASDELETE").count(), 2);
    assert!(!sbl.contains("ASAGIE"));
    for (n, hole, wall) in [(1, "-30.000", "-24.00000"), (2, "1.000", "7.00000")] {
        let context = format!("\"PAIR20\",\"CUT{n}\"");
        assert!(sbl.contains(&format!(
            "{context},NULL,ID_GEOMETRY,\"AGIE.USING_PAIR20{n}.ISO\""
        )));
        assert!(sbl.contains(&format!("{context},\"stp1\",ID_POSX,\"{hole}\"")));
        let iso = std::fs::read_to_string(out.join(format!("PAIR20{n}.ISO"))).unwrap();
        assert!(iso.starts_with(&format!(
            "( PAIR20{n}.ISO )\r\nN0001 G00 X{wall} Y0.00000 ;\r\n"
        )));
    }
    assert!(
        job.controller_review
            .contains("CUT1 PAIR201.ISO, CUT2 PAIR202.ISO")
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_single_work_through_the_group_export_keeps_its_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let one = export_group_with_review(&[punch()], &temp.path().join("A"), "CCSEP20", 1.0).unwrap();
    let two = export_with_review(&punch(), &temp.path().join("B"), "CCSEP20", 1.0).unwrap();
    assert_eq!(one.files.len(), 3);
    assert_eq!(one.controller_review, two.controller_review);
    for name in ["CCSEP20.ISO", "CCSEP20.SBR", "CCSEP20.SBL"] {
        let a = std::fs::read(temp.path().join("A").join(name)).unwrap();
        let b = std::fs::read(temp.path().join("B").join(name)).unwrap();
        assert_eq!(a, b, "{name}");
    }
    let sbl = std::fs::read(temp.path().join("A/CCSEP20.SBL")).unwrap();
    assert_eq!(
        sbl,
        include_bytes!("../../tests/golden/agie-setup/CCSEP20.SBL")
    );
}

#[test]
fn a_group_rejects_an_unqualified_height_or_reserved_geometry_alias() {
    let temp = tempfile::tempdir().unwrap();
    for (height, name, expected) in [
        (-40., "PAIR", "20 mm"),
        (-20., "LPT", "non-reserved"),
        (-20., "ABCDEFGH", "seven"),
    ] {
        let mut a = left();
        let mut b = right(1.);
        a.stock_min[2] = height;
        b.stock_min[2] = height;
        let dir = temp.path().join(name);
        let why = export_group_with_review(&[a, b], &dir, name, 1.0).unwrap_err();
        assert!(why.contains(expected), "{why}");
        assert!(!dir.exists());
    }
}

#[test]
fn a_group_refuses_a_die_two_works_on_one_area_another_stock_or_no_separation() {
    let temp = tempfile::tempdir().unwrap();
    let die = AgieJob {
        side: AgieSide::Die,
        ..right(1.)
    };
    let mut wide = right(1.);
    wide.stock_max[0] = 41.;
    for (jobs, separation, expected) in [
        (vec![left(), die], 1.0, "closed punches"),
        (vec![left(), right(1.), right(1.)], 1.0, "PAIR2 and PAIR3 share"),
        (vec![left(), wide], 1.0, "same 20 mm stock"),
        (vec![left(), right(1.)], 0.0, "1.0 mm"),
    ] {
        let dir = temp.path().join("PAIR");
        let why = export_group_with_review(&jobs, &dir, "PAIR", separation).unwrap_err();
        assert!(why.contains(expected), "{why}");
        assert!(!dir.exists());
    }
}

/// The clear band includes each rough corner (at least the 0.245 mm rough
/// offset of 20 mm stock), not only the 3 mm lead and the 0.125 mm wire radius.
#[test]
fn the_band_between_punches_includes_the_rough_corner_lead_and_wire() {
    let temp = tempfile::tempdir().unwrap();
    // Lead and wire only would leave a 0.3 mm band at X-3.45. The rough
    // corners of both punches close it.
    for (start_x, ok) in [(-2.0, true), (-3.45, false), (-5.0, false), (-32.0, false)] {
        let dir = temp.path().join(format!("P{}", (start_x * -100.0) as i32));
        let result = export_group_with_review(&[left(), right(start_x)], &dir, "PAIR20", 1.0);
        assert_eq!(result.is_ok(), ok, "X{start_x}: {:?}", result.err());
        if !ok {
            assert!(result.unwrap_err().contains("clear band"), "X{start_x}");
            assert!(!dir.exists());
        }
    }
}
