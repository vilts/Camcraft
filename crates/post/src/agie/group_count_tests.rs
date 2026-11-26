//! Punch groups of two to nine works (#2980): one ISO, resource, work and hole for each punch,
//! one quality object for the piece, and the clear band over every pair of works.
//! Provenance: CamCraft's original synthetic rectangular punch on a 70 × 40 mm grid.
use super::die_group_tests::opening;
use super::tests::punch;
use super::*;

/// The rectangular punch of `tests` moved to grid cell `i` (three columns).
fn cell(i: usize) -> AgieJob {
    let x = (i % 3) as f64 * 70.0 - 70.0;
    let y = (i / 3) as f64 * 40.0 - 40.0;
    let mut job = punch();
    for p in &mut job.contour {
        p[0] += x;
        p[1] += y;
    }
    job.start = Some([x - 30.0, y]);
    job.stock_min = [-125.0, -80.0, -20.0];
    job.stock_max = [125.0, 80.0, 0.0];
    job
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn every_group_size_has_its_own_file_work_and_hole_and_one_quality() {
    let temp = tempfile::tempdir().unwrap();
    for count in 2..=MAX_PUNCH_WORKS {
        let jobs: Vec<_> = (0..count).map(cell).collect();
        let dir = temp.path().join(count.to_string());
        let job = export_group_with_review(&jobs, &dir, "PUNCH", 1.0).unwrap();
        assert_eq!(job.files.len(), count + 2);
        let sbl = std::fs::read_to_string(dir.join("PUNCH.SBL")).unwrap();
        let map = std::fs::read_to_string(dir.join("PUNCH.SBR")).unwrap();
        assert_eq!(map.lines().count(), count);
        // One quality object for the piece; each work refers to it (#2984).
        assert_eq!(sbl.matches("JE_CreateQuality(").count(), 1, "{count}");
        assert_eq!(sbl.matches("ID_QUALITY,je_q1").count(), count);
        assert_eq!(sbl.matches("JE_Create(WORK,").count(), count);
        assert_eq!(sbl.matches("ID_PUNCH,c_punch").count(), count);
        assert_eq!(sbl.matches("ID_SEPCUT,\"1.0\"").count(), count);
        for n in 1..=count {
            let alias = format!("PUNCH{n}.ISO");
            assert!(dir.join(&alias).is_file());
            assert!(map.contains(&format!("AGIE.USING_{alias} IMPORT {alias};")));
            assert!(sbl.contains(&format!(
                "\"CUT{n}\",NULL,ID_GEOMETRY,\"AGIE.USING_{alias}\""
            )));
            let x = ((n - 1) % 3) as f64 * 70.0 - 100.0;
            assert!(sbl.contains(&format!("\"CUT{n}\",\"stp1\",ID_POSX,\"{x:.3}\"")));
        }
    }
}

#[test]
fn a_tenth_punch_or_a_third_die_opening_writes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let ten: Vec<_> = (0..=MAX_PUNCH_WORKS).map(cell).collect();
    let three_dies = vec![opening(-40.), opening(0.), opening(40.)];
    for (jobs, name) in [(ten, "TOOMANY"), (three_dies, "DIES")] {
        let dir = temp.path().join(name);
        let why = export_group_with_review(&jobs, &dir, name, 1.0).unwrap_err();
        assert!(why.contains("2 to 9 independent closed punches"), "{why}");
        assert!(!dir.exists());
    }
}

/// The control can cut every profile before any separation, so the first
/// and the last work must clear each other even with a clear work between.
#[test]
fn a_collision_of_works_that_are_not_neighbours_names_the_pair() {
    let temp = tempfile::tempdir().unwrap();
    let jobs = [cell(0), cell(1), cell(0)];
    let dir = temp.path().join("TRIO20");
    let why = export_group_with_review(&jobs, &dir, "TRIO20", 1.0).unwrap_err();
    assert!(
        why.contains("TRIO201 and TRIO203 share a cutting area"),
        "{why}"
    );
    assert!(why.contains("every pair of works"), "{why}");
    assert!(!dir.exists());
}
