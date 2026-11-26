//! Malformed SPLINE and ELLIPSE data is refused by name and never panics or hangs (#1865),
//! and a drawing whose curves were read for a smaller unit than the applied one is refused.
//! Provenance: hand-built records from the public DXF group codes; a fixed-seed mutation loop.

use camcraft_geometry::dxf::{Unit, parse};
use camcraft_geometry::sketch_data::{SketchData, SketchError};

fn wrap(entity: &str) -> String {
    format!("0\nSECTION\n2\nENTITIES\n{entity}0\nENDSEC\n0\nEOF\n")
}

/// A valid clamped cubic with four control points; `edit` changes its text.
fn spline(edit: impl Fn(String) -> String) -> String {
    wrap(&edit(
        "0\nSPLINE\n8\n0\n71\n3\n72\n8\n73\n4\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n\
         10\n0\n20\n0\n10\n3\n20\n5\n10\n7\n20\n5\n10\n10\n20\n0\n"
            .to_owned(),
    ))
}

fn refused(text: &str, kind: &str, words: &str) {
    let e = parse(text.as_bytes()).expect_err(words);
    assert!(e.0.starts_with(kind) && e.0.contains(words), "{e}");
}

#[test]
fn malformed_splines_are_refused_by_name() {
    assert!(parse(spline(|s| s).as_bytes()).is_ok());
    refused(&spline(|s| s.replace("40\n1\n40\n1\n40\n1\n40\n1\n", "40\n1\n40\n0.5\n40\n1\n40\n1\n")), "SPLINE", "non-decreasing");
    refused(&spline(|s| s.replace("72\n8\n", "72\n7\n")), "SPLINE", "more knot values");
    refused(&spline(|s| s.replace("72\n8\n", "72\n9\n")), "SPLINE", "does not match groups 72 and 73");
    refused(&spline(|s| s.replace("71\n3\n", "71\n0\n")), "SPLINE", "degree must be 1 to 11");
    refused(&spline(|s| s.replace("71\n3\n", "71\n12\n")), "SPLINE", "degree must be 1 to 11");
    refused(&spline(|s| s.replace("71\n3\n", "71\n-3\n")), "SPLINE", "degree must be 1 to 11");
    refused(&spline(|s| s.replace("73\n4\n", "73\n2000000000\n")), "SPLINE", "exceeds limit");
    refused(&spline(|s| s.replace("73\n4\n", "73\n-1\n")), "SPLINE", "negative count");
    refused(&spline(|s| s.replace("40\n0\n40\n0\n40\n0\n40\n0\n", "40\n0\n40\n0\n40\n0\n40\n1\n")), "SPLINE", "empty domain");
    let weights = |w: &str| spline(move |s| s.replace("10\n3\n20\n5\n", &format!("10\n3\n20\n5\n41\n{w}\n")));
    refused(&weights("0"), "SPLINE", "weight count");
    let all = |w: &str| {
        spline(move |s| {
            s.replace("20\n0\n10\n3", "20\n0\n41\n1\n10\n3")
                .replace("20\n5\n10\n7", &format!("20\n5\n41\n{w}\n10\n7"))
                .replace("20\n5\n10\n10", "20\n5\n41\n1\n10\n10")
                .replace("10\n10\n20\n0\n", "10\n10\n20\n0\n41\n1\n")
        })
    };
    assert!(parse(all("2").as_bytes()).is_ok(), "{:?}", parse(all("2").as_bytes()));
    refused(&all("0"), "SPLINE", "weights must be finite and positive");
    refused(&all("-1"), "SPLINE", "weights must be finite and positive");
    refused(&all("1e9"), "SPLINE", "factor of 1e6");
    refused(&spline(|s| s.replace("10\n7\n20\n5\n", "10\n7\n20\n5\n30\n1\n").replace("10\n3\n20\n5\n", "10\n3\n20\n5\n30\n0\n")), "SPLINE", "one Z plane");
    refused(&spline(|s| s.replace("10\n7\n20\n5\n", "10\n7\n")), "SPLINE", "no Y coordinate");
    refused(
        &wrap("0\nSPLINE\n8\n0\n71\n1\n72\n6\n73\n4\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n2\n10\n0\n20\n0\n10\n1\n20\n0\n10\n2\n20\n0\n10\n3\n20\n0\n"),
        "SPLINE",
        "multiplicity",
    );
    refused(&wrap("0\nSPLINE\n8\n0\n71\n3\n74\n2\n11\n0\n21\n0\n11\n5\n21\n5\n"), "SPLINE", "fit points without control points");
    refused(&wrap("0\nSPLINE\n8\n0\n"), "SPLINE", "missing group 71");
    refused(&spline(|s| s.replace("10\n3\n", "10\n1e300\n")), "SPLINE", "coordinate outside");
    // A cusp: the curve doubles back on itself where the speed is zero.
    let cusp = wrap(
        "0\nSPLINE\n8\n0\n71\n2\n72\n6\n73\n3\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n\
         10\n0\n20\n0\n10\n10\n20\n0\n10\n0\n20\n0.000001\n",
    );
    if let Err(e) = parse(cusp.as_bytes()) {
        assert!(e.0.starts_with("SPLINE"), "{e}");
    }
}

#[test]
fn spline_control_points_use_a_per_point_z_default() {
    let drawing = |first_z, last_z| {
        wrap(&format!(
            "0\nSPLINE\n71\n1\n72\n4\n73\n2\n40\n0\n40\n0\n40\n1\n40\n1\n\
             10\n0\n20\n0\n{first_z}10\n10\n20\n0\n{last_z}"
        ))
    };

    let zero = "30\n0\n";
    let elevated = "30\n1\n";
    refused(&drawing(elevated, ""), "SPLINE", "one Z plane");
    refused(&drawing("", elevated), "SPLINE", "one Z plane");
    for (first, last) in [
        ("", ""),
        (zero, ""),
        ("", zero),
        (zero, zero),
        (elevated, elevated),
    ] {
        assert!(parse(drawing(first, last).as_bytes()).is_ok());
    }
}

#[test]
fn spline_control_points_refuse_orphan_and_duplicate_z() {
    refused(
        &spline(|s| s.replace("10\n0\n20\n0\n", "30\n0\n10\n0\n20\n0\n")),
        "SPLINE",
        "Z before X and Y",
    );
    refused(
        &spline(|s| s.replace("10\n3\n20\n5\n", "10\n3\n30\n0\n20\n5\n")),
        "SPLINE",
        "Z before X and Y",
    );
    refused(
        &spline(|s| s.replace("10\n3\n20\n5\n", "10\n3\n20\n5\n30\n0\n30\n0\n")),
        "SPLINE",
        "duplicate Z",
    );
}

#[test]
fn spline_fit_points_use_a_per_point_z_default() {
    // Fit points only: one explicit Z of 1 and one omitted Z (the default 0).
    let fits = |first_z, last_z| {
        wrap(&format!(
            "0\nSPLINE\n71\n3\n74\n2\n11\n0\n21\n0\n{first_z}11\n5\n21\n5\n{last_z}"
        ))
    };
    refused(&fits("31\n1\n", ""), "SPLINE", "fit points are not in one Z plane");
    refused(&fits("", "31\n1\n"), "SPLINE", "fit points are not in one Z plane");
    // Control points and fit points: all in one plane, explicit or omitted.
    let both = |control_z: &str, first_z: &str, last_z: &str| {
        wrap(&format!(
            "0\nSPLINE\n71\n1\n72\n4\n73\n2\n74\n2\n40\n0\n40\n0\n40\n1\n40\n1\n\
             10\n0\n20\n0\n{control_z}10\n10\n20\n0\n{control_z}\
             11\n0\n21\n0\n{first_z}11\n10\n21\n0\n{last_z}"
        ))
    };
    let elevation = |text: String| parse(text.as_bytes()).expect("reads").elevation;
    assert_eq!(elevation(both("", "", "")), Some(0.0));
    assert_eq!(elevation(both("30\n2\n", "31\n2\n", "31\n2\n")), Some(2.0));
    refused(&both("30\n2\n", "31\n2\n", ""), "SPLINE", "fit points are not in one Z plane");
    refused(&both("30\n2\n", "31\n1\n", "31\n1\n"), "SPLINE", "fit points are not in one Z plane");
}

#[test]
fn spline_fit_points_refuse_orphan_and_duplicate_z() {
    let fits = |points: &str| wrap(&format!("0\nSPLINE\n71\n3\n74\n2\n{points}"));
    refused(&fits("31\n0\n11\n0\n21\n0\n11\n5\n21\n5\n"), "SPLINE", "Z before X and Y in a fit point");
    refused(&fits("11\n0\n31\n0\n21\n0\n11\n5\n21\n5\n"), "SPLINE", "Z before X and Y in a fit point");
    refused(&fits("11\n0\n21\n0\n31\n0\n31\n0\n11\n5\n21\n5\n"), "SPLINE", "duplicate Z in a fit point");
    refused(&fits("21\n0\n11\n0\n11\n5\n21\n5\n"), "SPLINE", "Y before X in a fit point");
    refused(&fits("11\n0\n11\n5\n21\n5\n"), "SPLINE", "fit point has no Y coordinate");
}

#[test]
fn malformed_ellipses_are_refused_by_name() {
    let ellipse = |edit: &dyn Fn(&str) -> String| {
        wrap(&edit("0\nELLIPSE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n0\n40\n0.5\n41\n0\n42\n6.283185307179586\n"))
    };
    assert!(parse(ellipse(&|s| s.to_owned()).as_bytes()).is_ok());
    refused(&ellipse(&|s| s.replace("40\n0.5\n", "40\n0\n")), "ELLIPSE", "axis ratio");
    refused(&ellipse(&|s| s.replace("40\n0.5\n", "40\n1.5\n")), "ELLIPSE", "axis ratio");
    refused(&ellipse(&|s| s.replace("40\n0.5\n", "")), "ELLIPSE", "missing group 40");
    refused(&ellipse(&|s| s.replace("11\n10\n21\n0\n", "11\n0\n21\n0\n")), "ELLIPSE", "radius must be finite and positive");
    refused(&ellipse(&|s| s.replace("42\n6.283185307179586\n", "42\n0\n")), "ELLIPSE", "zero-length");
    refused(&ellipse(&|s| s.replace("42\n6.283185307179586\n", "42\n1e300\n")), "ELLIPSE", "plain angle range");
    refused(&ellipse(&|s| format!("{s}31\n2\n")), "ELLIPSE", "drawing plane");
    refused(&ellipse(&|s| format!("{s}230\n-1\n")), "ELLIPSE", "extrusion");
    refused(&ellipse(&|s| s.replace("10\n0\n", "10\nNaN\n")), "ELLIPSE", "numeric");
    // A needle: the tips turn on a radius far under the tolerance. It reads or is refused
    // by name, within the work budget.
    if let Err(e) = parse(ellipse(&|s| s.replace("40\n0.5\n", "40\n1e-9\n")).as_bytes()) {
        assert!(e.0.starts_with("ELLIPSE"), "{e}");
    }
}

#[test]
fn mutated_curve_records_never_panic() {
    // One drawing in millimetres with both curves; each mutation changes one value.
    let base = spline(|s| s + "0\nELLIPSE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n0\n40\n0.5\n41\n0\n42\n3\n")
        .replace("0\nSECTION\n2\nENTITIES\n", "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    assert!(parse(base.as_bytes()).is_ok(), "{:?}", parse(base.as_bytes()));
    let lines: Vec<&str> = base.lines().collect();
    let values = ["0", "-1", "1e308", "-1e-308", "NaN", "inf", "3", "1e9", "0.5", "x", "", "2147483647"];
    let mut seed: u64 = 0x1865;
    for _ in 0..300 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let at = (seed >> 33) as usize % lines.len();
        let value = values[(seed >> 13) as usize % values.len()];
        let mut mutated: Vec<&str> = lines.clone();
        if at % 2 == 1 {
            mutated[at] = value;
        }
        let text = mutated.join("\n") + "\n";
        let _ = parse(text.as_bytes());
    }
}

#[test]
fn curves_read_for_a_smaller_unit_than_the_applied_one_are_refused() {
    let ellipse = wrap("0\nELLIPSE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n0\n40\n0.5\n41\n0\n42\n6.283185307179586\n");
    // No $INSUNITS: read for inches, the largest unit the application lets a caller state.
    let doc = parse(ellipse.as_bytes()).expect("reads");
    assert!(SketchData::from_dxf(doc.clone(), Some(Unit::Inches), None).is_ok());
    assert!(SketchData::from_dxf(doc.clone(), Some(Unit::Millimeters), None).is_ok());
    let refused = SketchData::from_dxf(doc, Some(Unit::Meters), None).expect_err("looser than the bound");
    assert!(matches!(refused, SketchError::CurveTolerance { .. }), "{refused}");
    assert!(refused.to_string().contains("$INSUNITS"), "{refused}");
}
