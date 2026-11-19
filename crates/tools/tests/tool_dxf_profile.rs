//! DXF half-profile domain acceptance tests (#743).
//! Gate map (M4.4): covers tools::tool_profile_dxf, tools::spun, geometry::dxf.

use std::f64::consts::TAU;

use camcraft_geometry::dxf::{Primitive, Unit, parse};
use camcraft_tools::spun::SpunPoint;
use camcraft_tools::tool_profile_dxf::{dxf_to_spun, dxf_to_spun_with_unit};

fn dxf_wrap(units: i32, entities: &str) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{units}\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n"
    )
}

#[test]
fn dxf_major_bulge_arc_extent_is_preserved_and_axial_reversal_is_named() {
    for clockwise in [true, false] {
        // Same open profile in both directions: the major arc reaches z=65,
        // above its endpoints at z=55 and z=60. A minor-arc replacement loses material.
        let vertices = if clockwise {
            "10\n0\n20\n55\n42\n0\n\
             10\n10\n20\n55\n42\n-2.414213562373095\n\
             10\n15\n20\n60\n42\n0\n\
             10\n0\n20\n60\n42\n0\n"
        } else {
            "10\n0\n20\n60\n42\n0\n\
             10\n15\n20\n60\n42\n2.414213562373095\n\
             10\n10\n20\n55\n42\n0\n\
             10\n0\n20\n55\n42\n0\n"
        };
        let dxf = dxf_wrap(
            4,
            &format!(
                "0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n0\n21\n80\n\
                 0\nLWPOLYLINE\n8\nPROFILE\n90\n4\n70\n0\n{vertices}"
            ),
        );
        let source = parse(dxf.as_bytes()).expect("bulged profile parses");
        let (center, radius, start_angle, sweep_angle) = source
            .primitives
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::Arc {
                    center,
                    radius,
                    start_angle,
                    sweep_angle,
                } => Some((center, radius, start_angle, sweep_angle)),
                _ => None,
            })
            .expect("source contains the major arc");
        assert!((center.x - 10.0).abs() < 1e-9 && (center.y - 60.0).abs() < 1e-9);
        assert!((radius - 5.0).abs() < 1e-9);
        let expected_sweep = if clockwise { -0.75 * TAU } else { 0.75 * TAU };
        assert!((sweep_angle - expected_sweep).abs() < 1e-9);
        let source_top = (0..=96)
            .map(|i| center.y + radius * (start_angle + sweep_angle * i as f64 / 96.0).sin())
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((source_top - 65.0).abs() < 1e-9, "source arc reaches z=65");

        let err = dxf_to_spun(dxf.as_bytes())
            .expect_err("major arc reverses axially; accepting a z=60 top would truncate material");
        assert!(err.contains("runs back down the axis"), "{err}");
    }
}

#[test]
fn dxf_half_profile_error_refusals_never_panic_and_name_reasons() {
    // 1. No centreline
    let no_cl = dxf_wrap(
        4,
        "0\nLINE\n8\nPROFILE\n10\n0\n20\n20\n11\n10\n21\n20\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n20\n11\n10\n21\n40\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n40\n11\n0\n21\n40\n",
    );
    let err = dxf_to_spun(no_cl.as_bytes()).expect_err("no centreline");
    assert!(err.to_lowercase().contains("centreline"), "{err}");

    // 2. SPLINE entity refused
    let spline = dxf_wrap(
        4,
        "0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n0\n21\n50\n\
         0\nSPLINE\n8\nPROFILE\n",
    );
    let err = dxf_to_spun(spline.as_bytes()).expect_err("spline unsupported");
    assert!(err.contains("SPLINE"), "{err}");

    // 3. Crossing centreline (segments on both x > 0 and x < 0)
    let crossing = dxf_wrap(
        4,
        "0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n0\n21\n60\n\
         0\nLINE\n8\nPROFILE\n10\n0\n20\n20\n11\n10\n21\n20\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n20\n11\n-5\n21\n40\n\
         0\nLINE\n8\nPROFILE\n10\n-5\n20\n40\n11\n0\n21\n50\n",
    );
    let err = dxf_to_spun(crossing.as_bytes()).expect_err("crossing centreline");
    assert!(err.to_lowercase().contains("cross"), "{err}");

    // 4. Multiple centrelines
    let multi_cl = dxf_wrap(
        4,
        "0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n0\n21\n50\n\
         0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n0\n21\n60\n\
         0\nLINE\n8\nPROFILE\n10\n0\n20\n20\n11\n10\n21\n20\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n20\n11\n10\n21\n40\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n40\n11\n0\n21\n40\n",
    );
    assert!(dxf_to_spun(multi_cl.as_bytes()).is_err());

    // 5. Disconnected profile segments
    let disconnected = dxf_wrap(
        4,
        "0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n0\n21\n60\n\
         0\nLINE\n8\nPROFILE\n10\n0\n20\n20\n11\n10\n21\n20\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n30\n11\n0\n21\n40\n",
    );
    assert!(dxf_to_spun(disconnected.as_bytes()).is_err());

    // 6. Runs back down the axis
    let runs_back = dxf_wrap(
        4,
        "0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n0\n21\n60\n\
         0\nLINE\n8\nPROFILE\n10\n0\n20\n20\n11\n10\n21\n40\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n40\n11\n15\n21\n30\n\
         0\nLINE\n8\nPROFILE\n10\n15\n20\n30\n11\n0\n21\n50\n",
    );
    assert!(dxf_to_spun(runs_back.as_bytes()).is_err());

    // 7. Unitless DXF refused unless fallback unit supplied
    let unitless = dxf_wrap(
        0,
        "0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n0\n21\n60\n\
         0\nLINE\n8\nPROFILE\n10\n0\n20\n20\n11\n10\n21\n20\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n20\n11\n10\n21\n40\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n40\n11\n0\n21\n40\n",
    );
    assert!(dxf_to_spun(unitless.as_bytes()).is_err());
    assert!(dxf_to_spun_with_unit(unitless.as_bytes(), Some(Unit::Millimeters)).is_ok());
}

fn asymmetric_holder_dxf(start: (f64, f64), end: (f64, f64), units: i32, shift: f64) -> String {
    let mut entities = format!(
        "0\nLINE\n8\nCENTRELINE\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
        start.0, start.1, end.0, end.1,
    );
    // The shoulder distinguishes a reversed holder from a translated one.
    let points = [
        (0.0, 0.0),
        (20.0, 0.0),
        (20.0, 20.0),
        (10.0, 20.0),
        (10.0, 60.0),
        (0.0, 60.0),
    ];
    for pair in points.windows(2) {
        entities.push_str(&format!(
            "0\nLINE\n8\nPROFILE\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
            pair[0].0 + shift,
            pair[0].1 + shift,
            pair[1].0 + shift,
            pair[1].1 + shift,
        ));
    }
    dxf_wrap(units, &entities)
}

#[test]
fn dxf_centreline_direction_cannot_move_or_flip_the_holder() {
    let expected = [
        SpunPoint::line(0.0, 0.0),
        SpunPoint::line(0.0, 20.0),
        SpunPoint::line(20.0, 20.0),
        SpunPoint::line(20.0, 10.0),
        SpunPoint::line(60.0, 10.0),
        SpunPoint::line(60.0, 0.0),
    ];
    for shift in [0.0, -20.0, 35.0] {
        for reverse in [false, true] {
            let lower = (shift, shift);
            let upper = (shift, shift + 100.0);
            let (start, end) = if reverse {
                (upper, lower)
            } else {
                (lower, upper)
            };
            let bytes = asymmetric_holder_dxf(start, end, 4, shift);
            let profile = dxf_to_spun(bytes.as_bytes()).expect("Y-parallel centreline imports");
            assert_eq!(
                profile.points(),
                expected,
                "shift={shift}, reverse={reverse}"
            );
        }
    }
    // Extending the lower axis end deliberately defines a new origin. Reversing
    // that same LINE must still give the same documented axial offset.
    let offset: Vec<_> = expected
        .iter()
        .map(|p| SpunPoint::line(p.z + 20.0, p.r))
        .collect();
    for (start, end) in [((0.0, -20.0), (0.0, 100.0)), ((0.0, 100.0), (0.0, -20.0))] {
        let bytes = asymmetric_holder_dxf(start, end, 4, 0.0);
        let profile = dxf_to_spun(bytes.as_bytes()).expect("lower axis end defines zero");
        assert_eq!(profile.points(), offset);
    }
}

#[test]
fn dxf_centreline_not_parallel_to_y_is_refused_by_name() {
    for (start, end) in [
        ((0.0, 0.0), (100.0, 0.0)),
        ((0.0, 0.0), (1.0, 100.0)),
        ((0.0, 0.0), (1e-9, 100.0)),
        ((1.0, 100.0), (0.0, 0.0)),
    ] {
        let bytes = asymmetric_holder_dxf(start, end, 4, 0.0);
        let err = dxf_to_spun(bytes.as_bytes()).expect_err("non-Y centreline must refuse");
        assert!(err.contains("centreline must be parallel to Y"), "{err}");
    }
}
