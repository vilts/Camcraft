//! Pinning tests for `camcraft-gcode` RS-274/NGC and ISO 6983 3-axis milling parsing.

use camcraft_cl::cl::{ArcPlane, CycleKind, MoveKind};
use camcraft_gcode::{
    parse, parse_with, ArcCenterMode, CycleReturnMode, Dialect, DistanceMode, GcodeError,
    MotionMode, ParseOptions, PathControlMode, ProgramEnd, ReferencePositions, SpindleDir,
    StopKind, WorkOffset,
};
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::geometry::Point3;
use camcraft_geometry::geometry::units::{MM_PER_INCH, Unit};

fn approx(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn assert_pt_near(got: Point3, want: Point3, tol: f64) {
    assert!(
        approx(got.x, want.x, tol) && approx(got.y, want.y, tol) && approx(got.z, want.z, tol),
        "point mismatch: got {got:?}, want {want:?}"
    );
}

#[test]
fn modal_groups_track_and_sections_split_on_tool_changes() {
    let nc = "\
%
O0001 (TWO TOOL TEST)
G21
G17 G90 G91.1 G94 G55 G40 G49 G80
G61
T1 M6
G43 H1
S8000 M3
M8
G0 X10. Y20. Z5.
G1 Z-2. F200
G1 X30. F600
G91
G1 X5. Y-5.
G90
M9
M5
M1
T2 M6
G43 H2
G56
G64 P0.025
S12000 M4
M7
M88
G0 X0. Y0. Z10.
G4 P0.5
G1 Z-1. F300
M89
M9
M5
M30
%";
    // G55 and G56 origins differ: tool 2 starts at the last G55 point in G56 (#3657 review 3).
    let mut references = ReferencePositions::default();
    references.wcs_origins[1] = Some(Point3::new(100.0, 0.0, 0.0));
    references.wcs_origins[2] = Some(Point3::new(0.0, 50.0, -10.0));
    let opts = ParseOptions { dialect: Dialect::tormach(), references };
    let parsed = parse_with(nc, &opts).expect("valid two-tool program");

    assert_eq!(parsed.program_number, Some(1));
    assert_eq!(parsed.tools, vec![1, 2]);
    assert_eq!(parsed.ended, Some(ProgramEnd::M30));
    assert_eq!(parsed.sections.len(), 2);

    let s0 = &parsed.sections[0];
    assert_eq!(s0.tool_number, Some(1));
    assert_eq!(s0.work_offset, WorkOffset::G55);
    assert_eq!(s0.length_offset, Some(1));
    assert_eq!(s0.spindle_rpm, Some(8000.0));
    assert_eq!(s0.spindle_dir, SpindleDir::Cw);
    assert!(s0.toolpath.is_chained());
    assert_eq!(s0.toolpath.moves.len(), 4);
    assert_eq!(s0.move_lines, vec![10, 11, 12, 14]);
    assert_eq!(s0.toolpath.moves[0].kind, MoveKind::Rapid);
    assert_pt_near(s0.toolpath.moves[0].to, Point3::new(10.0, 20.0, 5.0), 1e-6);
    assert_eq!(s0.toolpath.moves[1].kind, MoveKind::Feed);
    assert_eq!(s0.toolpath.moves[1].feed, Some(200.0));
    assert_pt_near(s0.toolpath.moves[1].to, Point3::new(10.0, 20.0, -2.0), 1e-6);
    assert_pt_near(s0.toolpath.moves[2].to, Point3::new(30.0, 20.0, -2.0), 1e-6);
    assert_pt_near(s0.toolpath.moves[3].to, Point3::new(35.0, 15.0, -2.0), 1e-6);

    let s1 = &parsed.sections[1];
    assert_eq!(s1.tool_number, Some(2));
    assert_eq!(s1.work_offset, WorkOffset::G56);
    assert_eq!(s1.length_offset, Some(2));
    assert_eq!(s1.spindle_rpm, Some(12000.0));
    assert_eq!(s1.spindle_dir, SpindleDir::Ccw);
    assert!(s1.toolpath.is_chained());
    assert_eq!(s1.toolpath.moves.len(), 2);
    assert_eq!(s1.move_lines, vec![26, 28]);
    assert_pt_near(s1.toolpath.moves[0].from, Point3::new(135.0, -35.0, 8.0), 1e-6);
    assert_pt_near(s1.toolpath.moves[0].to, Point3::new(0.0, 0.0, 10.0), 1e-6);
    assert_pt_near(s1.toolpath.moves[1].to, Point3::new(0.0, 0.0, -1.0), 1e-6);

    assert_eq!(parsed.stops.len(), 1);
    assert_eq!(parsed.stops[0].kind, StopKind::OptionalStop);
    assert_eq!(parsed.stops[0].line, 18);
    assert_eq!(parsed.dwells.len(), 1);
    assert_eq!(parsed.dwells[0].line, 27);
    assert!(approx(parsed.dwells[0].seconds, 0.5, 1e-9));

    assert_eq!(parsed.line_for_move(0, 3), Some(14));
    assert_eq!(parsed.line_for_move(1, 1), Some(28));
    assert_eq!(parsed.line_map.line_for(0, 0), Some(10));

    assert_eq!(parsed.final_modal.units, Some(Unit::Millimeters));
    assert_eq!(parsed.final_modal.distance, DistanceMode::Absolute);
    assert_eq!(parsed.final_modal.arc_center, ArcCenterMode::Incremental);
    assert_eq!(parsed.final_modal.work_offset, WorkOffset::G56);
    assert_eq!(
        parsed.final_modal.path_control,
        PathControlMode::Continuous {
            tolerance_mm: Some(0.025)
        }
    );
}

#[test]
fn ijk_and_r_arcs_xy_xz_yz_and_full_circle() {
    let nc = "\
G21 G17 G90 G91.1 G94
G0 X10. Y0. Z0.
G3 X0. Y10. I-10. J0. F500
G18 G2 X10. Z-10. I0. K-10.
G19 G3 Y0. Z0. J0. K10.
G17 G0 X10. Y0. Z0.
G3 X0. Y10. R10. F400
G2 X0. Y-10. R10.
G90.1
G3 X-10. Y0. I0. J0.
G91.1
G3 X-10. Y0. I10. J0.
M30";
    let parsed = parse(nc).expect("valid arcs in XY, XZ, YZ");
    let s = &parsed.sections[0];
    let moves = &s.toolpath.moves;
    assert!(s.toolpath.is_chained());
    assert_eq!(moves.len(), s.move_lines.len());

    // Line 3: XY G3 CCW center (0, 0)
    let l3: Vec<_> = parsed.line_map.moves_on_line(3).collect();
    assert_eq!(l3.len(), 1);
    let a1 = moves[l3[0].1].arc.expect("line 3 arc");
    assert_eq!(a1.plane, ArcPlane::Xy);
    assert!(a1.ccw);
    assert_eq!(a1.center, Point2::new(0.0, 0.0));

    // Line 4: XZ G2 CW center (0, -10) in (X, Z) word order (Z-extremum may split)
    let l4: Vec<_> = parsed.line_map.moves_on_line(4).collect();
    assert!(!l4.is_empty());
    for (_, idx) in &l4 {
        let a = moves[*idx].arc.expect("line 4 XZ arc");
        assert_eq!(a.plane, ArcPlane::Xz);
        assert!(!a.ccw);
        assert_eq!(a.center, Point2::new(0.0, -10.0));
    }

    // Line 5: YZ G3 CCW center (10, 0) in (Y, Z) word order
    let l5: Vec<_> = parsed.line_map.moves_on_line(5).collect();
    assert!(!l5.is_empty());
    for (_, idx) in &l5 {
        let a = moves[*idx].arc.expect("line 5 YZ arc");
        assert_eq!(a.plane, ArcPlane::Yz);
        assert!(a.ccw);
        assert_eq!(a.center, Point2::new(10.0, 0.0));
    }

    // Line 7: XY G3 R+10 minor arc (10,0)->(0,10) => center (0, 0)
    let l7: Vec<_> = parsed.line_map.moves_on_line(7).collect();
    assert_eq!(l7.len(), 1);
    let a5 = moves[l7[0].1].arc.expect("line 7 R+ arc");
    assert_eq!(a5.plane, ArcPlane::Xy);
    assert!(a5.ccw);
    assert_eq!(a5.center, Point2::new(0.0, 0.0));

    // Line 10: G90.1 absolute center (0, 0)
    let l10: Vec<_> = parsed.line_map.moves_on_line(10).collect();
    assert_eq!(l10.len(), 1);
    let a_abs = moves[l10[0].1].arc.expect("line 10 G90.1 arc");
    assert_eq!(a_abs.center, Point2::new(0.0, 0.0));

    // Line 12: full circle I10 J0 from (-10, 0) => split into 2 half-circle arcs on line 12
    let l12: Vec<_> = parsed.line_map.moves_on_line(12).collect();
    assert_eq!(l12.len(), 2);
    let fc0 = moves[l12[0].1].arc.expect("full circle first half");
    let fc1 = moves[l12[1].1].arc.expect("full circle second half");
    assert_eq!(fc0.center, Point2::new(0.0, 0.0));
    assert_eq!(fc1.center, Point2::new(0.0, 0.0));
    assert_pt_near(moves[l12[0].1].to, Point3::new(10.0, 0.0, 0.0), 1e-6);
    assert_pt_near(moves[l12[1].1].to, Point3::new(-10.0, 0.0, 0.0), 1e-6);
}

#[test]
fn r_major_vs_minor_arc_centers_differ_as_expected() {
    let nc_minor = "G21 G17 G90 G94\nG0 X10. Y0. Z0.\nG3 X0. Y10. R10. F500\nM30\n";
    let nc_major = "G21 G17 G90 G94\nG0 X10. Y0. Z0.\nG3 X0. Y10. R-10. F500\nM30\n";
    let p_minor = parse(nc_minor).expect("minor arc");
    let p_major = parse(nc_major).expect("major arc");
    let c_minor = p_minor.sections[0].toolpath.moves[1].arc.expect("minor").center;
    let c_major = p_major.sections[0].toolpath.moves[1].arc.expect("major").center;
    assert!(approx(c_minor.x, 0.0, 1e-6) && approx(c_minor.y, 0.0, 1e-6));
    assert!(approx(c_major.x, 10.0, 1e-6) && approx(c_major.y, 10.0, 1e-6));
}

#[test]
fn canned_cycles_expand_into_move_chains_and_honor_g98_g99() {
    let nc = "\
G21 G17 G90 G94
T4 M6
S1500 M3
G0 X10. Y10. Z10.
G99
G81 X10. Y10. Z-5. R2. F120
X20. Y10.
G80
G0 Z10.
G98
G73 X30. Y10. Z-8. R2. Q3. F150
G80
G99
G82 X40. Y10. Z-4. R2. P500 F100
G83 X50. Y10. Z-7. R2. Q3. F90
G84 X60. Y10. Z-10. R2. F300
G86 X70. Y10. Z-6. R2. F110
G89 X80. Y10. Z-6. R2. P250 F110
G80
M30";
    let opts = ParseOptions {
        dialect: Dialect::fanuc(),
        ..Default::default()
    };
    let parsed = parse_with(nc, &opts).expect("canned cycles");
    let tp = &parsed.sections[0].toolpath;
    assert!(tp.is_chained());
    assert_eq!(tp.cycles.len(), 8);
    assert_eq!(tp.cycles[0].kind, CycleKind::Drill);
    assert_eq!(tp.cycles[1].kind, CycleKind::Drill);
    assert_eq!(tp.cycles[2].kind, CycleKind::Peck);
    assert_eq!(tp.cycles[3].kind, CycleKind::Drill);
    assert!(approx(tp.cycles[3].dwell, 0.5, 1e-6));
    assert_eq!(tp.cycles[4].kind, CycleKind::Deep);
    assert_eq!(tp.cycles[5].kind, CycleKind::Tap);
    assert_eq!(tp.cycles[6].kind, CycleKind::Bore);
    assert_eq!(tp.cycles[7].kind, CycleKind::BackBore);
    assert!(approx(tp.cycles[7].dwell, 0.25, 1e-6));

    // G99 on G81 returns to R=2.0
    let c0_end = tp.cycles[1].first_move;
    assert_pt_near(tp.moves[c0_end - 2].to, Point3::new(10.0, 10.0, 2.0), 1e-6);

    // G98 on G73 returns to initial Z=10.0 after final peck
    let g73_start = tp.cycles[2].first_move;
    let g82_hole_pos = tp.cycles[3].first_move;
    let g73_retract = tp.moves[g73_start..g82_hole_pos]
        .iter()
        .find(|m| m.kind == MoveKind::Rapid && approx(m.to.z, 10.0, 1e-6));
    assert!(g73_retract.is_some(), "G98 must retract G73 back to initial Z=10.0");

    // G84 taps with Feed down AND Feed up
    let tap_idx = tp.cycles[5].first_move;
    assert_eq!(tp.moves[tap_idx].kind, MoveKind::Feed);
    assert_pt_near(tp.moves[tap_idx].to, Point3::new(60.0, 10.0, -10.0), 1e-6);
    assert_eq!(tp.moves[tap_idx + 1].kind, MoveKind::Feed);
    assert_pt_near(tp.moves[tap_idx + 1].to, Point3::new(60.0, 10.0, 2.0), 1e-6);

    // G89 feeds down AND feeds up
    let g89_idx = tp.cycles[7].first_move;
    assert_eq!(tp.moves[g89_idx].kind, MoveKind::Plunge);
    assert_eq!(tp.moves[g89_idx + 1].kind, MoveKind::Feed);
    assert_pt_near(tp.moves[g89_idx + 1].to, Point3::new(80.0, 10.0, 2.0), 1e-6);

    assert_eq!(tp.moves.len(), parsed.sections[0].move_lines.len());
    assert_eq!(parsed.final_modal.motion, MotionMode::None);
    assert_eq!(parsed.final_modal.cycle_return, CycleReturnMode::RPlane);
}

#[test]
fn unit_scaling_g20_inches_to_mm_and_unscaled_s_t_p() {
    let nc = "\
G20 G17 G90 G91.1 G94
T2 M6
S2400 M3
G0 X1. Y2. Z0.5
G1 Z-0.25 F10.
G3 X2. Y3. I0. J1. F12.
G4 P1.5
G83 X3. Y4. Z-1. R0.1 Q0.25 F8.
G80
M30";
    let parsed = parse(nc).expect("inch program");
    let s = &parsed.sections[0];
    assert_eq!(s.spindle_rpm, Some(2400.0));
    assert!(approx(parsed.dwells[0].seconds, 1.5, 1e-9));

    let m0 = &s.toolpath.moves[0];
    assert_pt_near(
        m0.to,
        Point3::new(1.0 * MM_PER_INCH, 2.0 * MM_PER_INCH, 0.5 * MM_PER_INCH),
        1e-6,
    );
    let m1 = &s.toolpath.moves[1];
    assert_pt_near(
        m1.to,
        Point3::new(1.0 * MM_PER_INCH, 2.0 * MM_PER_INCH, -0.25 * MM_PER_INCH),
        1e-6,
    );
    assert!(approx(m1.feed.expect("feed"), 10.0 * MM_PER_INCH, 1e-6));

    let arc = s.toolpath.moves[2].arc.expect("inch arc");
    assert!(approx(arc.center.x, 1.0 * MM_PER_INCH, 1e-5));
    assert!(approx(arc.center.y, 3.0 * MM_PER_INCH, 1e-5));

    assert!(approx(s.toolpath.cycles[0].peck, 0.25 * MM_PER_INCH, 1e-6));
}

#[test]
fn reference_moves_g53_g28_g30_execute_when_configured_and_refuse_when_missing() {
    let nc_g53 = "G21 G90 G54\nG53 G0 Z0.\nM30\n";
    let err = parse(nc_g53).expect_err("G53 without reference origin must refuse");
    assert_eq!(err.line(), 2);
    assert!(err.to_string().contains("G53"), "{err}");

    let nc_g28 = "G21 G90\nG0 X10. Y20. Z5.\nG91 G28 Z0.\nG28 X0. Y0.\nM30\n";
    let err = parse(nc_g28).expect_err("G28 without reference position must refuse");
    assert_eq!(err.line(), 3);
    assert!(err.to_string().contains("G28"), "{err}");

    let nc_g30 = "G21 G90\nG0 X10. Y20. Z5.\nG30\nM30\n";
    let err = parse(nc_g30).expect_err("G30 without reference position must refuse");
    assert_eq!(err.line(), 3);
    assert!(err.to_string().contains("G30"), "{err}");

    let opts = ParseOptions {
        references: ReferencePositions {
            g28: Some(Point3::new(0.0, 0.0, 200.0)),
            g30: Some(Point3::new(50.0, 50.0, 180.0)),
            g53_origin: Some(Point3::new(100.0, 100.0, 100.0)),
            wcs_origins: [None; 6],
        },
        ..Default::default()
    };
    let ok_g53 = parse_with(nc_g53, &opts).expect("G53 with origin");
    assert_pt_near(
        ok_g53.sections[0].toolpath.moves[0].to,
        Point3::new(0.0, 0.0, -100.0),
        1e-6,
    );

    let ok_g28 = parse_with(nc_g28, &opts).expect("G28 with ref");
    let moves = &ok_g28.sections[0].toolpath.moves;
    assert_pt_near(moves[1].to, Point3::new(10.0, 20.0, 100.0), 1e-6);
    assert_pt_near(moves[2].to, Point3::new(-100.0, -100.0, 100.0), 1e-6);

    let ok_g30 = parse_with(nc_g30, &opts).expect("G30 with ref");
    assert_pt_near(
        ok_g30.sections[0].toolpath.moves[1].to,
        Point3::new(-50.0, -50.0, 80.0),
        1e-6,
    );
}

#[test]
fn unsupported_codes_and_bad_inputs_refuse_by_name_with_line_number() {
    let cases: &[(&str, usize, &str)] = &[
        ("G90\nG0 X10. Y10.\nM30\n", 2, "unit"),
        ("G21 G90\nG1 G41 X10. Y0. D1 F100\nM30\n", 2, "G41"),
        ("G21 G90\nG1 G42 X10. Y0. D1 F100\nM30\n", 2, "G42"),
        ("G21 G90\nG65 P9000 A1.\nM30\n", 2, "G65"),
        ("G21 G90\nM98 P100\nM30\n", 2, "M98"),
        ("G21 G90\n#100=5\nG0 X#100\nM30\n", 2, "#"),
        ("G21 G90\nG0 X0. Y0.\nG31 X10. F100\nM30\n", 3, "G31"),
        ("G21 G90\nG0 X0. Y0.\nG38.2 Z-5. F50\nM30\n", 3, "G38.2"),
        ("G21 G90\nG0 X10. Y0.\nG2 X10. Y0. R5. F100\nM30\n", 3, "R"),
        ("G21 G90\nG0 X0. A45.\nM30\n", 2, "A"),
        ("G21 G90\nG0 X0. B15.\nM30\n", 2, "B"),
        ("G21 G90\nG0 X0. C90.\nM30\n", 2, "C"),
        ("G21 G90\nG999 X10.\nM30\n", 2, "G999"),
        ("G21 G90\nQ42\nM30\n", 2, "Q"),
        ("G21 G90\nU10.\nM30\n", 2, "U"),
    ];
    for (nc, want_line, want_token) in cases {
        let err = parse(nc).expect_err(&format!("expected refusal for {nc:?}"));
        assert_eq!(err.line(), *want_line, "wrong line for {nc:?}: {err}");
        let msg = err.to_string();
        assert!(
            msg.contains(want_token),
            "error '{msg}' did not name '{want_token}' for {nc:?}"
        );
    }
    let e = parse("G21 G90\nG0 X10.\nG43.4 H1\n").expect_err("G43.4 refused");
    assert_eq!(e.line(), 3);
    assert!(matches!(e, GcodeError::UnsupportedCode { .. }));
}
