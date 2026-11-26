//! Native arcs of a DXF circle or racetrack punch (#2022).
use super::round_profile::{ArcBlock, iso, prepare};
use super::tests::punch;
use super::*;
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::wire_round_profile::{RoundProfile, Spine, WireProfile};

const U: f64 = 100_000.0;

fn grid(p: [f64; 2]) -> [i64; 2] {
    [(p[0] * U).round() as i64, (p[1] * U).round() as i64]
}

fn round_job(start: [f64; 2]) -> AgieJob {
    AgieJob {
        contour: Vec::new(),
        start: Some(start),
        ..punch()
    }
}

fn circle(center: [f64; 2], radius: f64) -> RoundProfile {
    RoundProfile {
        spine: Spine::Circle(Point2::new(center[0], center[1])),
        radius,
    }
}

/// The R10 racetrack of examples/agie/racetrack.camscript, in setup XY.
fn racetrack() -> RoundProfile {
    RoundProfile {
        spine: Spine::Stadium([Point2::new(-6., 1.), Point2::new(10., 1.)]),
        radius: 10.,
    }
}

#[test]
fn qualified_circle_radii_and_cardinal_entries_keep_the_exact_diameter() {
    let center = [2.0, -3.0];
    for radius in [6.0, 8.375, 10.0, 15.0] {
        for axis in [0, 1] {
            for sign in [-1.0, 1.0] {
                let mut hole = center;
                let mut seam = center;
                let mut opposite = center;
                hole[axis] += sign * (radius + 7.0);
                seam[axis] += sign * radius;
                opposite[axis] -= sign * radius;
                let (blocks, setup) =
                    prepare(&round_job(hole), &circle(center, radius), "CIRCLE").unwrap();
                assert_eq!(setup.start_hole_um, hole.map(|v| (v * 1000.0) as i32));
                let c = Some(grid(center));
                assert_eq!(
                    blocks,
                    [
                        ArcBlock { end: grid(seam), center: None },
                        ArcBlock { end: grid(opposite), center: c },
                        ArcBlock { end: grid(seam), center: c },
                    ]
                );
            }
        }
    }
}

#[test]
fn the_circle_iso_has_two_g03_semicircles_with_incremental_centres() {
    let (blocks, _) = prepare(&round_job([-17., 0.]), &circle([0., 0.], 10.), "CIRCLE20").unwrap();
    assert_eq!(
        iso("CIRCLE20.ISO", &blocks),
        "( CIRCLE20.ISO )\r\n\
         N0001 G00 X-10.00000 Y0.00000 ;\r\n\
         N0002 G03 X10.00000 Y0.00000 I10.00000 J0.00000 ;\r\n\
         N0003 G03 X-10.00000 Y0.00000 I-10.00000 J0.00000 ;\r\n\
         N0004 M02 ;\r\n"
    );
}

#[test]
fn the_racetrack_iso_enters_a_straight_and_keeps_both_cap_centres() {
    let (blocks, _) = prepare(&round_job([2., -16.]), &racetrack(), "RACE20").unwrap();
    assert_eq!(
        iso("RACE20.ISO", &blocks),
        "( RACE20.ISO )\r\n\
         N0001 G00 X2.00000 Y-9.00000 ;\r\n\
         N0002 G01 X10.00000 Y-9.00000 ;\r\n\
         N0003 G03 X10.00000 Y11.00000 I0.00000 J10.00000 ;\r\n\
         N0004 G01 X-6.00000 Y11.00000 ;\r\n\
         N0005 G03 X-6.00000 Y-9.00000 I0.00000 J-10.00000 ;\r\n\
         N0006 G01 X2.00000 Y-9.00000 ;\r\n\
         N0007 M02 ;\r\n"
    );
    // The hole on the other side enters the return straight.
    let (blocks, _) = prepare(&round_job([2., 16.]), &racetrack(), "RACE20").unwrap();
    assert_eq!(blocks[0].end, grid([2., 11.]));
    assert_eq!(blocks[2], ArcBlock { end: grid([-6., -9.]), center: Some(grid([-6., 1.])) });
}

#[test]
fn the_arc_envelope_needs_the_rough_pass_and_wire_inside_the_stock() {
    // The punch stock is X-40..40 Y-30..30. A cap extremum at Y-29.6 leaves
    // exactly 0.4 mm; at Y-29.61 it leaves less.
    let at = |y: f64| prepare(&round_job([-34., y + 10.]), &circle([-20., y + 10.], 10.), "C");
    assert!(at(-29.6).is_ok());
    let error = at(-29.61).unwrap_err();
    assert!(error.contains("0.4 mm of stock beyond the arc extrema"), "{error}");
}

#[test]
fn unqualified_shapes_entries_and_plates_are_refused() {
    let refused = |job: AgieJob, shape: RoundProfile| prepare(&job, &shape, "C").is_err();
    // Circle radius outside 6..15 mm, an oblique or short lead, an off-grid centre.
    assert!(refused(round_job([-15.99, 0.]), circle([0., 0.], 5.99)));
    assert!(refused(round_job([-25., 0.]), circle([0., 0.], 15.01)));
    assert!(refused(round_job([-17., 0.001]), circle([0., 0.], 10.)));
    assert!(refused(round_job([-12., 0.]), circle([0., 0.], 10.)));
    assert!(refused(round_job([-17., 0.]), circle([0., 0.000001], 10.)));
    // A racetrack that is not parallel to X or Y, or a hole near a cap.
    let oblique = RoundProfile {
        spine: Spine::Stadium([Point2::new(-6., 1.), Point2::new(10., 2.)]),
        radius: 10.,
    };
    assert!(refused(round_job([2., -16.]), oblique));
    assert!(refused(round_job([8., -16.]), racetrack()));
    // A die or a 40 mm plate has no control check for native arcs.
    let die = AgieJob { side: AgieSide::Die, ..round_job([-17., 0.]) };
    assert!(refused(die, circle([0., 0.], 10.)));
    let tall = AgieJob { stock_min: [-40., -30., -40.], ..round_job([-17., 0.]) };
    assert!(refused(tall, circle([0., 0.], 10.)));
}

#[test]
fn a_refused_arc_job_publishes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("CIRCLE20");
    let job = round_job([-17., 0.]);
    let shape = circle([0., 0.], 10.);
    for separation in [0.0, 1.5] {
        assert!(export_round_with_review(&job, &WireProfile::Round(shape), &out, "CIRCLE20", separation).is_err());
        assert!(!out.exists());
    }
    let shape = circle([0., 0.], 5.0);
    assert!(export_round_with_review(&job, &WireProfile::Round(shape), &out, "CIRCLE20", 1.0).is_err());
    assert!(!out.exists());
    let files = export_round_with_review(&job, &WireProfile::Round(circle([0., 0.], 10.)), &out, "CIRCLE20", 1.0)
        .unwrap()
        .files;
    assert_eq!(files.len(), 3);
}
