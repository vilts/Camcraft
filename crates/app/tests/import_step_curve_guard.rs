//! Provenance: issue #1545 adversarial STEP fixtures derived from our box.step.
//! Gate map (M4.4): covers app::geometry::import::step, app::geometry::import::step_guard.

use camcraft_app::geometry::import::{self, step, Format};
use std::sync::mpsc;
use std::time::Duration;

fn fixture(swept: bool, rational: bool) -> Vec<u8> {
    let mut text = include_str!("../../../testdata/models/box.step").to_owned();
    let spline = "B_SPLINE_CURVE_WITH_KNOTS('', 1, (#158, #200, #201, #159), .UNSPECIFIED., .F., .F., (2, 2, 2), (0.0, 0.5, 1.0), .UNSPECIFIED.)".to_owned();
    let spline = if rational {
        "(BOUNDED_CURVE() B_SPLINE_CURVE(1, (#158, #200, #201, #159), .UNSPECIFIED., .F., .F.) B_SPLINE_CURVE_WITH_KNOTS((2, 2, 2), (0.0, 0.5, 1.0), .UNSPECIFIED.) CURVE() GEOMETRIC_REPRESENTATION_ITEM() RATIONAL_B_SPLINE_CURVE((0.0, 0.0, 0.0, 0.0)) REPRESENTATION_ITEM(''))".to_owned()
    } else {
        spline
    };
    if swept {
        text = text.replace("#85 = PLANE('', #86);", "#85 = SURFACE_OF_LINEAR_EXTRUSION('', #202, #128);");
        text = text.replace("ENDSEC;\nEND-ISO", &format!("#202 = {spline};\nENDSEC;\nEND-ISO"));
    } else {
        text = text.replace("#110 = LINE('', #111, #112);", &format!("#110 = {spline};"));
    }
    text = text.replace("ENDSEC;\nEND-ISO", "#200 = CARTESIAN_POINT('', (0.3, 0.0, 0.0));\n#201 = CARTESIAN_POINT('', (0.7, 0.0, 0.0));\nENDSEC;\nEND-ISO");
    text.into_bytes()
}

fn refusal(swept: bool, rational: bool) -> String {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = step::with_face_eval_cap(100_000, || import::parse_model(&fixture(swept, rational), Format::Step));
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_secs(20)).expect("curve subdivision must finish within 20 seconds") {
        Err(error) => error.to_string(),
        Ok(mesh) => panic!("a discontinuous curve must be refused, got {} facets", mesh.facets.len()),
    }
}

#[test]
fn discontinuous_edge_curve_is_refused_by_name() {
    let error = refusal(false, false);
    assert!(error.contains("edge") && error.contains("subdivision"), "{error}");
}

#[test]
fn discontinuous_swept_curve_is_refused_by_name() {
    let error = refusal(true, false);
    assert!(error.contains("swept") && error.contains("subdivision"), "{error}");
}

#[test]
fn zero_weight_edge_curve_is_refused_by_name() {
    let error = refusal(false, true);
    assert!(error.contains("edge") && error.contains("subdivision"), "{error}");
}
