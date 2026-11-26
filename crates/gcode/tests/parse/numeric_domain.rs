//! Numeric domain (#3657 review 2): overflow after unit conversion, incremental addition or the arc radius square is refused by line; caller options are validated.
//! Provenance: AGENTS.md Prime Directive (refuse, never emit changed geometry); IEEE 754 binary64 range.

use camcraft_gcode::{GcodeError, ParseOptions, ReferencePositions, WorkOffset, MAX_ABS_MM};
use camcraft_geometry::geometry::Point3;

use crate::{parse_in_time, refusal};

fn out_of_range_on(e: &GcodeError, line: usize) -> bool {
    matches!(e, GcodeError::OutOfRange { line: l, .. } if *l == line)
}

/// A decimal integer with `digits` digits: finite as f64, far outside any machine.
fn huge(digits: usize) -> String {
    format!("1{}.", "0".repeat(digits - 1))
}

#[test]
fn huge_finite_axis_words_refuse_by_line() {
    // 304 digits: finite as f64, but the micron lattice (x 1e6) is infinite.
    let e = refusal(&format!("G21 G90\nG0 X{}\n", huge(304)));
    assert!(out_of_range_on(&e, 2), "{e:?}");
    let e = refusal(&format!("G20 G90\nG0 X1.\nG0 Y{}\n", huge(306)));
    assert!(out_of_range_on(&e, 3), "inch conversion overflow: {e:?}");
}

#[test]
fn incremental_sum_past_the_domain_refuses() {
    let e = refusal("G21 G91\nG0 X900000.\nG0 X900000.\n");
    assert!(out_of_range_on(&e, 3), "{e:?}");
}

#[test]
fn huge_arc_radius_refuses_instead_of_a_straight_move() {
    let e = refusal(&format!("G21 G90\nG0 X0 Y0\nG2 X10. Y0 R{} F100\n", huge(201)));
    assert!(out_of_range_on(&e, 3), "{e:?}");
    let e = refusal(&format!("G21 G90 G90.1\nG0 X0 Y0\nG2 X10. Y0 I{} J0 F100\n", huge(200)));
    assert!(out_of_range_on(&e, 3), "absolute centre: {e:?}");
}

#[test]
fn huge_feed_after_inch_conversion_refuses() {
    let e = refusal(&format!("G20 G90\nG1 X1. F{}\n", huge(308)));
    assert!(out_of_range_on(&e, 2), "{e:?}");
}

#[test]
fn values_inside_the_domain_still_parse() {
    let lim = MAX_ABS_MM - 1.0;
    let nc = format!("G21 G90\nG0 X{lim} Y-{lim} Z1.\nG1 X0 F100\n");
    let p = parse_in_time(&nc, &ParseOptions::default()).expect("inside the domain");
    assert_eq!(p.sections[0].toolpath.moves.len(), 2);
}

#[test]
fn bad_caller_options_refuse_before_line_one() {
    let mut bad = Vec::new();
    let mut o = ParseOptions::default();
    o.dialect.initial_position = Point3::new(f64::NAN, 0.0, 0.0);
    bad.push(("dialect.initial_position", o));
    let mut o = ParseOptions::default();
    o.dialect.arc_radius_tolerance_mm = -0.1;
    bad.push(("dialect.arc_radius_tolerance_mm", o));
    let mut o = ParseOptions::default();
    o.dialect.chip_break_retract_mm = f64::INFINITY;
    bad.push(("dialect.chip_break_retract_mm", o));
    let mut refs = ReferencePositions::zero();
    refs.wcs_origins[WorkOffset::G57.index()] = Some(Point3::new(0.0, 1e300, 0.0));
    bad.push(("references.wcs_origins[3]", ParseOptions { references: refs, ..Default::default() }));
    let mut refs = ReferencePositions::zero();
    refs.g28 = Some(Point3::new(0.0, 0.0, f64::NEG_INFINITY));
    bad.push(("references.g28", ParseOptions { references: refs, ..Default::default() }));
    for (want, opts) in bad {
        let e = parse_in_time("G21 G90\nG0 X1.\n", &opts).expect_err(want);
        assert!(matches!(&e, GcodeError::InvalidOptions { field, .. } if field == want), "{want}: {e:?}");
        assert_eq!(e.line(), 0);
    }
}
