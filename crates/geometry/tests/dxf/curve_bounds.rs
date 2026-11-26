//! A long non-rational SPLINE reads in linear time, and a near-round ELLIPSE is a circle only
//! while the circle is within the curve tolerance (#1865 review, optional items).
//! Provenance: hand-built records from the public DXF group codes.

use camcraft_geometry::dxf::{CURVE_TOLERANCE_MM, Primitive, parse};
use std::time::{Duration, Instant};

fn mm(entity: &str) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{entity}0\nENDSEC\n0\nEOF\n"
    )
}

/// A degree-1 SPLINE through the points (x, 0) with all Z omitted.
fn polyline_spline(xs: &[f64]) -> String {
    let n = xs.len();
    let mut s = format!("0\nSPLINE\n8\n0\n71\n1\n72\n{}\n73\n{n}\n40\n0\n", n + 2);
    for i in 0..n {
        s += &format!("40\n{i}\n");
    }
    s += &format!("40\n{}\n", n - 1);
    for x in xs {
        s += &format!("10\n{x}\n20\n0\n");
    }
    s
}

#[test]
fn a_long_non_rational_spline_reads_in_linear_time() {
    // 40,000 collinear control points: one span each. A weight scan for each span made this
    // quadratic (14.6 s in a debug build; the review read 32,000 such points in 14.7 s).
    let xs: Vec<f64> = (0..40_000).map(|i| f64::from(i) * 0.01).collect();
    let text = mm(&polyline_spline(&xs));
    let started = Instant::now();
    let doc = parse(text.as_bytes()).expect("reads");
    let took = started.elapsed();
    assert!(!doc.primitives.is_empty());
    assert!(took < Duration::from_secs(5), "{took:?}");
}

fn ellipse(radius: f64, ratio: &str) -> String {
    format!("0\nELLIPSE\n8\n0\n10\n0\n20\n0\n11\n{radius}\n21\n0\n40\n{ratio}\n41\n0\n42\n6.283185307179586\n")
}

fn is_circle(p: &Primitive) -> bool {
    matches!(p, Primitive::Circle { .. })
}

#[test]
fn a_near_round_ellipse_is_a_circle_only_within_the_tolerance() {
    // Exactly round: the exact circle, and no curve tolerance is used.
    let doc = parse(mm(&ellipse(10.0, "1")).as_bytes()).expect("reads");
    assert_eq!(doc.primitives.len(), 1);
    assert!(is_circle(&doc.primitives[0].primitive));
    assert_eq!(doc.curve_tolerance, None);
    // 10 mm at ratio 1 - 5e-5: the circle is 0.0005 mm off, within the bound. It is a
    // circle, and the document records the tolerance.
    let doc = parse(mm(&ellipse(10.0, "0.99995")).as_bytes()).expect("reads");
    assert_eq!(doc.primitives.len(), 1);
    assert!(is_circle(&doc.primitives[0].primitive));
    assert_eq!(doc.curve_tolerance, Some(CURVE_TOLERANCE_MM));
    // 10 mm at ratio 1 - 2e-4: the circle would be 0.002 mm off. Arcs, not a circle.
    let doc = parse(mm(&ellipse(10.0, "0.9998")).as_bytes()).expect("reads");
    assert!(doc.primitives.len() > 1);
    assert!(doc.primitives.iter().all(|p| !is_circle(&p.primitive)));
    // The review case: radius 1e10 mm at ratio 0.9999999999995 is 0.005 mm off a circle.
    // It reads as arcs or is refused by name, never as the circle.
    match parse(mm(&ellipse(1e10, "0.9999999999995")).as_bytes()) {
        Ok(doc) => assert!(doc.primitives.iter().all(|p| !is_circle(&p.primitive))),
        Err(e) => assert!(e.0.starts_with("ELLIPSE"), "{e}"),
    }
}
