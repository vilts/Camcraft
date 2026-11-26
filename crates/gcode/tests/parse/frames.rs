//! Coordinate frames (#3657 review 3): a work-offset change moves the cursor into the new frame; G28/G30 use the machine origin of the active offset.
//! Provenance: work and machine coordinates from RS-274/NGC (NISTIR 6556 §3.2.2) and the LinuxCNC G-code reference (G28, G30, G54-G59).

use camcraft_cl::cl::MoveKind;
use camcraft_gcode::{GcodeError, ParseOptions, ReferencePositions, WorkOffset};
use camcraft_geometry::geometry::Point3;

use crate::parse_in_time;

fn with_origins(origins: &[(WorkOffset, Point3)]) -> ParseOptions {
    let mut references = ReferencePositions::default();
    for (w, p) in origins {
        references.wcs_origins[w.index()] = Some(*p);
    }
    ParseOptions { references, ..Default::default() }
}

fn near(a: Point3, b: Point3) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && (a.z - b.z).abs() < 1e-9
}

/// The review input: G54 at machine X0, G55 at machine X100. `G0 X10` in each
/// frame is a 100 mm move, not a dropped one.
#[test]
fn g54_to_g55_keeps_the_machine_move() {
    let opts = with_origins(&[
        (WorkOffset::G54, Point3::ZERO),
        (WorkOffset::G55, Point3::new(100.0, 0.0, 0.0)),
    ]);
    let nc = "G21 G90 G54\nG0 X10.\nG55\nG0 X10.\nG0 Y5.\n";
    let p = parse_in_time(nc, &opts).expect("two frames");
    assert_eq!(p.sections.len(), 2);
    assert_eq!(p.sections[1].work_offset, WorkOffset::G55);
    let m = &p.sections[1].toolpath.moves;
    assert_eq!(m.len(), 2);
    assert!(near(m[0].from, Point3::new(-90.0, 0.0, 0.0)), "{:?}", m[0]);
    assert!(near(m[0].to, Point3::new(10.0, 0.0, 0.0)));
    assert!((m[0].to.x - m[0].from.x - 100.0).abs() < 1e-9);
    // Omitted axis words keep the cursor of the new frame.
    assert!(near(m[1].to, Point3::new(10.0, 5.0, 0.0)));
    assert_eq!(p.sections[1].move_lines, vec![4, 5]);
}

/// Two nonzero origins on all three axes; the omitted axes move by the offset difference.
#[test]
fn nonzero_origins_move_the_cursor_on_every_axis() {
    let opts = with_origins(&[
        (WorkOffset::G56, Point3::new(10.0, 20.0, -30.0)),
        (WorkOffset::G57, Point3::new(-5.0, 7.0, 2.0)),
    ]);
    let nc = "G21 G90 G56\nG0 X1. Y2. Z3.\nG57 G0 Z0.\n";
    let p = parse_in_time(nc, &opts).expect("two frames");
    let m = &p.sections[1].toolpath.moves[0];
    // Machine (11, 22, -27) in G57 is (16, 15, -29); only Z is written.
    assert!(near(m.from, Point3::new(16.0, 15.0, -29.0)), "{m:?}");
    assert!(near(m.to, Point3::new(16.0, 15.0, 0.0)));
}

#[test]
fn offset_change_after_motion_without_origins_refuses() {
    let nc = "G21 G90 G54\nG0 X10.\nG55\nG0 X10.\n";
    let e = parse_in_time(nc, &ParseOptions::default()).expect_err("unknown frames");
    assert!(matches!(e, GcodeError::MissingReferencePosition { line: 3, .. }), "{e:?}");
    // Before the first move the start point is stated in the first motion's frame.
    let p = parse_in_time("G21 G90 G55\nG0 X10.\n", &ParseOptions::default()).expect("no motion yet");
    assert_eq!(p.sections[0].work_offset, WorkOffset::G55);
}

#[test]
fn g28_and_g30_use_the_machine_origin_of_the_active_offset() {
    let mut opts = with_origins(&[
        (WorkOffset::G54, Point3::new(50.0, 20.0, -30.0)),
        (WorkOffset::G55, Point3::new(-10.0, 0.0, 5.0)),
    ]);
    opts.references.g28 = Some(Point3::new(0.0, 0.0, 100.0));
    opts.references.g30 = Some(Point3::new(200.0, 100.0, 50.0));
    let nc = "G21 G90 G54\nG0 X1. Y1. Z1.\nG28 Z5.\nG55\nG30\n";
    let p = parse_in_time(nc, &opts).expect("reference returns");
    let s0 = &p.sections[0].toolpath.moves;
    assert!(near(s0[1].to, Point3::new(1.0, 1.0, 5.0)), "G28 via point: {:?}", s0[1]);
    assert!(near(s0[2].to, Point3::new(1.0, 1.0, 130.0)), "G28 Z only: {:?}", s0[2]);
    let s1 = &p.sections[1].toolpath.moves;
    assert_eq!(s1[0].kind, MoveKind::Rapid);
    assert!(near(s1[0].to, Point3::new(210.0, 100.0, 45.0)), "G30 in G55: {:?}", s1[0]);
}

#[test]
fn g28_without_a_known_work_origin_refuses() {
    let mut opts = ParseOptions::default();
    opts.references.g28 = Some(Point3::new(0.0, 0.0, 100.0));
    let e = parse_in_time("G21 G90\nG0 X1.\nG28\n", &opts).expect_err("origin unknown");
    assert!(matches!(e, GcodeError::MissingReferencePosition { line: 3, .. }), "{e:?}");
}
