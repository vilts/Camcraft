//! Additional dialect preset, vertical-plane R-arc, and M-code tests for `camcraft-gcode`.
//! Provenance: verifies 3-axis dialect conventions and G18/G19 R-radius arcs against RS-274/NGC and CamCraft post formats.

use camcraft_cl::cl::{ArcPlane, CycleKind, MoveKind};
use camcraft_gcode::{
    parse, parse_with, Dialect, DwellUnit, ParseOptions, PathControlMode,
};
use camcraft_geometry::geo2d::Point2;

#[test]
fn vertical_plane_r_arcs_xz_and_yz_minor_and_major() {
    let nc = "\
G21 G90 G94
G0 X10. Y0. Z0.
G18 G2 X0. Z10. R10. F400
G0 X0. Y10. Z0.
G19 G3 Y0. Z10. R10.
G0 X10. Y0. Z0.
G18 G2 X0. Z10. R-10.
M30";
    let parsed = parse(nc).expect("XZ and YZ R-arcs");
    let moves = &parsed.sections[0].toolpath.moves;

    let xz_minor = parsed
        .line_map
        .moves_on_line(3)
        .map(|(_, idx)| moves[idx].arc.expect("XZ minor arc"))
        .collect::<Vec<_>>();
    assert!(!xz_minor.is_empty());
    assert_eq!(xz_minor[0].plane, ArcPlane::Xz);
    assert_eq!(xz_minor[0].center, Point2::new(0.0, 0.0));

    let yz_minor = parsed
        .line_map
        .moves_on_line(5)
        .map(|(_, idx)| moves[idx].arc.expect("YZ minor arc"))
        .collect::<Vec<_>>();
    assert!(!yz_minor.is_empty());
    assert_eq!(yz_minor[0].plane, ArcPlane::Yz);
    assert_eq!(yz_minor[0].center, Point2::new(0.0, 0.0));

    let xz_major = parsed
        .line_map
        .moves_on_line(7)
        .map(|(_, idx)| moves[idx].arc.expect("XZ major arc"))
        .collect::<Vec<_>>();
    assert!(!xz_major.is_empty());
    assert_eq!(xz_major[0].plane, ArcPlane::Xz);
    assert_eq!(xz_major[0].center, Point2::new(10.0, 10.0));
}

#[test]
fn dialects_dwell_units_ream_and_m6_tool_order() {
    let nc = "\
; compact Tormach/grblHAL style with M6 before T and G85 ream
G21 G90 G94 G64 P0.025
M6 T3
S1800 M3 M88
G00 X10. Y20. Z5.
G01X15.Y20.F250
G04 P1.5
G85 X25. Y20. Z-12. R2. F120
G80
M89 M5 M9
M30";

    let tormach_opts = ParseOptions {
        dialect: Dialect::tormach(),
        ..Default::default()
    };
    let p_tormach = parse_with(nc, &tormach_opts).expect("tormach dialected program");
    assert_eq!(p_tormach.tools, vec![3]);
    assert_eq!(p_tormach.sections[0].tool_number, Some(3));
    assert_eq!(
        p_tormach.final_modal.path_control,
        PathControlMode::Continuous {
            tolerance_mm: Some(0.025)
        }
    );
    assert!((p_tormach.dwells[0].seconds - 1.5).abs() < 1e-9);
    let cyc = p_tormach.sections[0].toolpath.cycles[0];
    assert_eq!(cyc.kind, CycleKind::Ream);
    let first = cyc.first_move;
    assert_eq!(p_tormach.sections[0].toolpath.moves[first].kind, MoveKind::Plunge);
    assert_eq!(p_tormach.sections[0].toolpath.moves[first + 1].kind, MoveKind::Feed);
    assert!(!p_tormach.final_modal.coolant.through_spindle);

    let fanuc_opts = ParseOptions {
        dialect: Dialect::fanuc(),
        ..Default::default()
    };
    assert_eq!(fanuc_opts.dialect.g4_dwell_unit, DwellUnit::Milliseconds);
    let p_fanuc = parse_with(nc, &fanuc_opts).expect("fanuc dialected program");
    assert!((p_fanuc.dwells[0].seconds - 0.0015).abs() < 1e-9);
}
