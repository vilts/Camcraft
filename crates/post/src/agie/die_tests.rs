//! Die openings on an original faceted slot: the mirrored entry and stock
//! margin, the flipped reach check, the 20 mm rule and the c_die setup (#2957).
use super::tests::punch;
use super::*;

/// The 38-sided slot of `examples/agie/die-plate.obj`: straight walls at
/// X±12.5 from Y-6 to Y6, joined by faceted semicircles of radius 12.5 with
/// 10° facets. Counter-clockwise.
pub(super) fn slot() -> Vec<[f64; 2]> {
    let arc = |cy: f64, from: i32| {
        (0..=18).map(move |k| {
            let a = f64::from(from + 10 * k).to_radians();
            [12.5 * a.cos(), cy + 12.5 * a.sin()]
        })
    };
    arc(6.0, 0).chain(arc(-6.0, 180)).collect()
}

fn die() -> AgieJob {
    AgieJob {
        side: AgieSide::Die,
        contour: slot(),
        start: Some([-5.5, 0.]),
        stock_min: [-35., -35., -20.],
        stock_max: [35., 35., 0.],
    }
}

fn points(job: &AgieJob) -> Result<Vec<[f64; 2]>, String> {
    contour::prepare(job, "DIE20").map(|(points, _)| points)
}

fn signed_area(ring: &[[f64; 2]]) -> f64 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

#[test]
fn a_die_enters_the_nearest_wall_from_inside_and_keeps_counter_clockwise_iso() {
    let prepared = points(&die()).unwrap();
    // The seam is the foot of the 7 mm inward lead on the wall X-12.5.
    assert_eq!(prepared[0], [-12.5, 0.]);
    // Counter-clockwise from the seam: down the wall first, up it last.
    assert_eq!(prepared[1], [-12.5, -6.]);
    assert_eq!(prepared[prepared.len() - 1], [-12.5, 6.]);
    assert_eq!(prepared.len(), 39);
    assert!(signed_area(&prepared) > 0.0);
}

#[test]
fn a_die_hole_too_close_to_one_wall_enters_the_opposite_wall() {
    // 1.5 mm from the left wall: the hole fits, but the lead is short.
    let job = AgieJob {
        start: Some([-11., 0.]),
        ..die()
    };
    assert_eq!(points(&job).unwrap()[0], [12.5, 0.]);
}

#[test]
fn a_die_hole_must_lie_completely_inside_the_opening() {
    for start in [[-20., 0.], [-11.6, 0.], [0., 18.]] {
        let job = AgieJob {
            start: Some(start),
            ..die()
        };
        let why = points(&job).unwrap_err();
        assert!(why.contains("inside the die opening"), "{start:?}: {why}");
    }
    // Inside, but no straight wall has 3 mm of room on both sides of the foot.
    let job = AgieJob {
        start: Some([0., 17.]),
        ..die()
    };
    assert!(points(&job).unwrap_err().contains("inward lead"));
}

#[test]
fn the_same_ring_with_an_inside_hole_is_refused_as_a_punch() {
    let job = AgieJob {
        side: AgieSide::Punch,
        ..die()
    };
    assert!(points(&job).is_err());
    // As a punch, the slot passes with an exterior hole.
    let job = AgieJob {
        start: Some([-20., 0.]),
        ..job
    };
    assert_eq!(points(&job).unwrap()[0], [-12.5, 0.]);
}

#[test]
fn the_reach_check_flips_every_opening_corner_to_an_inside_corner() {
    // The square-cornered punch passes, but its rectangle as a die opening
    // has four inside corners of the plate. The rough pass cannot reach them.
    let mut job = punch();
    job.side = AgieSide::Die;
    job.start = Some([0., 0.]);
    assert!(points(&job).unwrap_err().contains("0.002 mm"));
    // A hexagon has 60° turns. Its 0.245 mm rough miter leaves 0.038 mm.
    let hexagon = (0..6)
        .map(|k| {
            let a = f64::from(60 * k).to_radians();
            [15. * a.cos(), 15. * a.sin()]
        })
        .collect();
    let job = AgieJob {
        contour: hexagon,
        start: Some([0., 0.]),
        ..die()
    };
    assert!(points(&job).unwrap_err().contains("0.002 mm"));
}

#[test]
fn a_reentrant_die_opening_is_refused() {
    let mut contour = slot();
    // Push the middle of the right wall inward: a reentrant notch.
    contour.push([10., 0.]);
    let job = AgieJob { contour, ..die() };
    assert!(points(&job).unwrap_err().contains("convex"));
}

#[test]
fn a_die_needs_the_rough_pass_and_wire_margin_to_the_stock() {
    // 0.3 mm of plate beside the left wall is less than the 0.4 mm margin.
    let job = AgieJob {
        stock_min: [-12.8, -35., -20.],
        ..die()
    };
    assert!(points(&job).unwrap_err().contains("0.4 mm"));
    let job = AgieJob {
        stock_min: [-12.9, -35., -20.],
        ..die()
    };
    assert!(points(&job).is_ok());
}

#[test]
fn a_die_requires_the_qualified_20_mm_plate() {
    for height in [15., 40., 80.] {
        let job = AgieJob {
            stock_min: [-35., -35., -height],
            ..die()
        };
        assert!(points(&job).unwrap_err().contains("20 mm"), "{height}");
        // The same heights are qualified for a punch.
        let job = AgieJob {
            side: AgieSide::Punch,
            start: Some([-20., 0.]),
            ..job
        };
        assert!(points(&job).is_ok(), "{height}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_die_publishes_the_zero_step_setup_with_the_die_side() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("DIE20");
    let exported = export_with_review(&die(), &out, "DIE20", 1.0).unwrap();
    let iso = std::fs::read_to_string(out.join("DIE20.ISO")).unwrap();
    assert!(iso.starts_with("( DIE20.ISO )\r\nN0001 G00 X-12.50000 Y0.00000 ;\r\n"));
    assert!(iso.contains("N0002 G01 X-12.50000 Y-6.00000 ;\r\n"));
    assert!(iso.ends_with("N0040 G01 X-12.50000 Y0.00000 ;\r\nN0041 M02 ;\r\n"));
    let sbl = std::fs::read_to_string(out.join("DIE20.SBL")).unwrap();
    assert!(sbl.contains("ID_PUNCH,c_die") && !sbl.contains("c_punch"));
    // Apart from the side, the die setup is the zero-step punch setup.
    let punch = punch_setup_of_die();
    assert_eq!(sbl.replace("c_die", "c_punch"), punch);
    assert!(exported.controller_review.contains("c_die"));
    assert!(exported.controller_review.contains("245, 155, 140 and 134"));
    // The die keeps the 1.0 mm separation rule of the setup.
    let refused = temp.path().join("NOSEP");
    assert!(export(&die(), &refused, "NOSEP", 0.0).is_err());
    assert!(!refused.exists());
}

fn punch_setup_of_die() -> String {
    crate::agie_setup::PunchSetup20 {
        stem: "DIE20",
        stock_min_um: [-35_000, -35_000],
        stock_max_um: [35_000, 35_000],
        start_hole_um: [-5_500, 0],
        height_um: 20_000,
    }
    .encode()
    .unwrap()
    .sbl
}
