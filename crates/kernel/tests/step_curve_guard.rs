//! Pins the curve guard of the solidcore STEP reader (#917): an edge or a
//! swept surface on a B-spline curve that breaks at a repeated interior knot,
//! or that has a weight that is not positive, is refused by name before the
//! topology is read; a repeated knot with no break still imports.
//! Fixtures: the tracked box.step with one line or one plane swapped for a
//! hand-written spline (ISO 10303-42 entities).

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::import_step;
use solidcore::kernel::CancelToken;

const SPLINE: &str = "B_SPLINE_CURVE_WITH_KNOTS('', 1, (#158, #200, #201, #159), .UNSPECIFIED., .F., .F., (2, 2, 2), (0.0, 0.5, 1.0), .UNSPECIFIED.)";
const RATIONAL: &str = "(BOUNDED_CURVE() B_SPLINE_CURVE(1, (#158, #200, #201, #159), .UNSPECIFIED., .F., .F.) B_SPLINE_CURVE_WITH_KNOTS((2, 2, 2), (0.0, 0.5, 1.0), .UNSPECIFIED.) CURVE() GEOMETRIC_REPRESENTATION_ITEM() RATIONAL_B_SPLINE_CURVE((0.0, 0.0, 0.0, 0.0)) REPRESENTATION_ITEM(''))";

/// box.step with edge curve #110 (or plane #85, as a swept surface) on
/// `spline`; the two inner control points are at x `a` and `b`.
fn fixture(spline: &str, swept: bool, a: f64, b: f64) -> Vec<u8> {
    let mut text = include_str!("../../../testdata/models/box.step").replace("\r\n", "\n");
    let end = "ENDSEC;\nEND-ISO";
    if swept {
        text = text.replace(
            "#85 = PLANE('', #86);",
            "#85 = SURFACE_OF_LINEAR_EXTRUSION('', #202, #128);",
        );
        text = text.replace(end, &format!("#202 = {spline};\n{end}"));
    } else {
        text = text.replace("#110 = LINE('', #111, #112);", &format!("#110 = {spline};"));
    }
    text.replace(
        end,
        &format!(
            "#200 = CARTESIAN_POINT('', ({a:?}, 0.0, 0.0));\n#201 = CARTESIAN_POINT('', ({b:?}, 0.0, 0.0));\n{end}"
        ),
    )
    .into_bytes()
}

fn refusal(bytes: &[u8]) -> String {
    match import_step(&mut SolidcoreKernel::default(), bytes, &CancelToken::new()) {
        Ok(_) => panic!("the file must be refused"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn an_edge_spline_that_breaks_at_a_repeated_knot_is_refused_by_name() {
    let msg = refusal(&fixture(SPLINE, false, 0.3, 0.7));
    for word in ["edge #", "curve #110", "knot 0.5", "repeats 2 times", "degree 1", "subdivision"] {
        assert!(msg.contains(word), "{word:?} missing in: {msg}");
    }
}

#[test]
fn a_swept_surface_on_a_breaking_spline_is_refused_by_name() {
    let msg = refusal(&fixture(SPLINE, true, 0.3, 0.7));
    for word in ["swept surface #85", "curve #202", "knot 0.5", "subdivision"] {
        assert!(msg.contains(word), "{word:?} missing in: {msg}");
    }
}

#[test]
fn a_spline_with_a_zero_weight_is_refused_by_name() {
    let msg = refusal(&fixture(RATIONAL, false, 0.3, 0.7));
    for word in ["edge #", "curve #110", "weight 0", "not positive", "subdivision"] {
        assert!(msg.contains(word), "{word:?} missing in: {msg}");
    }
}

#[test]
fn a_repeated_knot_with_no_break_still_imports() {
    let read = import_step(
        &mut SolidcoreKernel::default(),
        &fixture(SPLINE, false, 0.5, 0.5),
        &CancelToken::new(),
    );
    assert!(read.is_ok(), "a continuous spline must import: {:?}", read.err());
}
