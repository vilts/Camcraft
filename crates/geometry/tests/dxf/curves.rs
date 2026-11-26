//! SPLINE and ELLIPSE read as tangent arcs (#1865): the output keeps the stated two-sided
//! bound against an independent evaluation of each curve, joins end to end, and turns
//! smoothly at each joint. INSERT and POINT no longer stop the import.
//! Provenance: hand-built fixtures from the public DXF group codes; the test's own
//! Cox-de Boor evaluator (The NURBS Book, A2.2) and the parametric ellipse.

use camcraft_geometry::dxf::{CURVE_TOLERANCE_MM, Primitive, parse};
use camcraft_geometry::geo2d::Point2;
use std::f64::consts::TAU;

const SPLINE: &str = include_str!("spline.dxf");
const ELLIPSE: &str = include_str!("ellipse.dxf");

/// The point of `p` at length fraction `f` (0..=1).
fn along(p: &Primitive, f: f64) -> Point2 {
    match *p {
        Primitive::Line { start, end } => Point2::new(start.x + (end.x - start.x) * f, start.y + (end.y - start.y) * f),
        Primitive::Arc { center, radius, start_angle, sweep_angle } => {
            let a = start_angle + sweep_angle * f;
            Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        }
        Primitive::Circle { center, radius } => {
            let a = TAU * f;
            Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        }
    }
}

/// The unit direction of travel of `p` at fraction `f`.
fn heading(p: &Primitive, f: f64) -> (f64, f64) {
    let (a, b) = (along(p, (f - 1e-7).max(0.0)), along(p, (f + 1e-7).min(1.0)));
    let l = (b.x - a.x).hypot(b.y - a.y);
    ((b.x - a.x) / l, (b.y - a.y) / l)
}

fn dist_to_segment(q: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (q.x - a.x - dx * t).hypot(q.y - a.y - dy * t)
}

fn dist_to_polyline(q: Point2, poly: &[Point2]) -> f64 {
    poly.windows(2).map(|w| dist_to_segment(q, w[0], w[1])).fold(f64::INFINITY, f64::min)
}

/// The exact distance from `q` to `p`.
fn dist_to(q: Point2, p: &Primitive) -> f64 {
    match *p {
        Primitive::Line { start, end } => dist_to_segment(q, start, end),
        Primitive::Arc { center, radius, start_angle, sweep_angle } => {
            let phi = (q.y - center.y).atan2(q.x - center.x);
            let turned = if sweep_angle >= 0.0 { (phi - start_angle).rem_euclid(TAU) } else { (start_angle - phi).rem_euclid(TAU) };
            if turned <= sweep_angle.abs() {
                ((q.x - center.x).hypot(q.y - center.y) - radius).abs()
            } else {
                let (a, b) = (along(p, 0.0), along(p, 1.0));
                (q.x - a.x).hypot(q.y - a.y).min((q.x - b.x).hypot(q.y - b.y))
            }
        }
        Primitive::Circle { center, radius } => ((q.x - center.x).hypot(q.y - center.y) - radius).abs(),
    }
}

/// Both one-sided distances, read densely: true curve samples to the output (exact), and
/// output samples to the true curve as a dense polyline (whose own sag is under 10 nm here).
fn hausdorff(curve: &[Point2], out: &[Primitive]) -> f64 {
    let mut worst: f64 = 0.0;
    for q in curve {
        worst = worst.max(out.iter().map(|p| dist_to(*q, p)).fold(f64::INFINITY, f64::min));
    }
    for p in out {
        for i in 0..=16 {
            worst = worst.max(dist_to_polyline(along(p, i as f64 / 16.0), curve));
        }
    }
    worst
}

/// The non-rational B-spline of `pts` by the Cox-de Boor recursion, independent of the reader.
fn bspline(deg: usize, knots: &[f64], pts: &[(f64, f64)], u: f64) -> Point2 {
    fn basis(i: usize, p: usize, k: &[f64], u: f64) -> f64 {
        if p == 0 {
            let last = u == k[k.len() - 1] && k[i] < k[i + 1] && k[i + 1] == k[k.len() - 1];
            return if (k[i] <= u && u < k[i + 1]) || last { 1.0 } else { 0.0 };
        }
        let a = if k[i + p] > k[i] { (u - k[i]) / (k[i + p] - k[i]) * basis(i, p - 1, k, u) } else { 0.0 };
        let b = if k[i + p + 1] > k[i + 1] { (k[i + p + 1] - u) / (k[i + p + 1] - k[i + 1]) * basis(i + 1, p - 1, k, u) } else { 0.0 };
        a + b
    }
    let (mut x, mut y) = (0.0, 0.0);
    for (i, (px, py)) in pts.iter().enumerate() {
        let n = basis(i, deg, knots, u);
        x += n * px;
        y += n * py;
    }
    Point2::new(x, y)
}

/// Each joint meets within 1 nm and turns by under 1e-6 radians.
fn smooth_closed_or_open(out: &[Primitive], closed: bool) {
    let count = if closed { out.len() } else { out.len() - 1 };
    for i in 0..count {
        let (a, b) = (&out[i], &out[(i + 1) % out.len()]);
        let (p, q) = (along(a, 1.0), along(b, 0.0));
        assert!((p.x - q.x).hypot(p.y - q.y) < 1e-6, "joint {i} gap: {p:?} {q:?}");
        let (u, v) = (heading(a, 1.0), heading(b, 0.0));
        let turn = (u.0 * v.1 - u.1 * v.0).atan2(u.0 * v.0 + u.1 * v.1).abs();
        assert!(turn < 1e-5, "joint {i} turns {turn} rad");
    }
}

#[test]
fn closed_cubic_spline_reads_as_tangent_arcs_within_the_bound() {
    let doc = parse(SPLINE.as_bytes()).expect("spline reads");
    let out: Vec<Primitive> = doc.primitives.iter().map(|p| p.primitive.clone()).collect();
    assert!(out.iter().all(|p| matches!(p, Primitive::Arc { .. } | Primitive::Line { .. })), "{out:?}");
    assert!(out.iter().filter(|p| matches!(p, Primitive::Arc { .. })).count() >= 4, "{out:?}");
    assert!(out.len() < 400, "{} segments: arcs should stay few", out.len());
    assert!(doc.primitives.iter().all(|p| p.layer == "outline"));
    assert_eq!(doc.curve_tolerance, Some(CURVE_TOLERANCE_MM));
    smooth_closed_or_open(&out, true);
    let knots = [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0];
    let pts = [(10.0, 0.0), (10.0, 10.0), (-10.0, 10.0), (-10.0, -10.0), (10.0, -10.0), (10.0, 0.0)];
    let curve: Vec<Point2> = (0..=8_000).map(|i| bspline(3, &knots, &pts, 3.0 * i as f64 / 8_000.0)).collect();
    let d = hausdorff(&curve, &out);
    assert!(d <= CURVE_TOLERANCE_MM, "Hausdorff {d} mm over the {CURVE_TOLERANCE_MM} mm bound");
}

#[test]
fn rational_quarter_circle_spline_becomes_arcs_on_its_circle() {
    let w = std::f64::consts::FRAC_1_SQRT_2;
    let text = format!(
        "0\nSECTION\n2\nENTITIES\n0\nSPLINE\n8\n0\n70\n12\n71\n2\n72\n6\n73\n3\n74\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n\
         10\n10\n20\n0\n30\n0\n41\n1\n10\n10\n20\n10\n30\n0\n41\n{w}\n10\n0\n20\n10\n30\n0\n41\n1\n0\nENDSEC\n0\nEOF\n"
    );
    let doc = parse(text.as_bytes()).expect("rational spline reads");
    assert!(doc.primitives.len() <= 8, "{:?}", doc.primitives);
    for p in &doc.primitives {
        for i in 0..=16 {
            let q = along(&p.primitive, i as f64 / 16.0);
            let r = q.x.hypot(q.y);
            // Unitless: read for inches, so the bound in drawing units is 0.001 / 25.4.
            assert!((r - 10.0).abs() <= CURVE_TOLERANCE_MM / 25.4, "{r}");
        }
    }
    smooth_closed_or_open(&doc.primitives.iter().map(|p| p.primitive.clone()).collect::<Vec<_>>(), false);
}

#[test]
fn degree_one_spline_is_its_control_polygon() {
    let text = "0\nSECTION\n2\nENTITIES\n0\nSPLINE\n8\n0\n71\n1\n72\n5\n73\n3\n40\n0\n40\n0\n40\n1\n40\n2\n40\n2\n\
                10\n0\n20\n0\n10\n5\n20\n0\n10\n5\n20\n5\n0\nENDSEC\n0\nEOF\n";
    let doc = parse(text.as_bytes()).expect("polyline spline reads");
    let lines: Vec<_> = doc.primitives.iter().map(|p| p.primitive.clone()).collect();
    assert_eq!(
        lines,
        vec![
            Primitive::Line { start: Point2::new(0.0, 0.0), end: Point2::new(5.0, 0.0) },
            Primitive::Line { start: Point2::new(5.0, 0.0), end: Point2::new(5.0, 5.0) },
        ]
    );
}

#[test]
fn ellipses_read_as_tangent_arcs_and_a_round_one_stays_a_circle() {
    let doc = parse(ELLIPSE.as_bytes()).expect("ellipses read");
    let by = |layer: &str| -> Vec<Primitive> { doc.primitives.iter().filter(|p| p.layer == layer).map(|p| p.primitive.clone()).collect() };
    assert_eq!(by("round"), vec![Primitive::Circle { center: Point2::new(-40.0, 0.0), radius: 6.0 }]);
    // The full ellipse: 10 x 5 about the origin, closed and smooth.
    let full = by("outline");
    assert!(full.len() >= 8 && full.len() < 200, "{} segments", full.len());
    smooth_closed_or_open(&full, true);
    let curve: Vec<Point2> = (0..=8_000).map(|i| {
        let t = TAU * i as f64 / 8_000.0;
        Point2::new(10.0 * t.cos(), 5.0 * t.sin())
    }).collect();
    let d = hausdorff(&curve, &full);
    assert!(d <= CURVE_TOLERANCE_MM, "ellipse Hausdorff {d}");
    // The half: major axis (0, 8) about (40, 0), ratio 0.25, parameters 0.5 to 3.0.
    let half = by("half");
    smooth_closed_or_open(&half, false);
    let curve: Vec<Point2> = (0..=4_000).map(|i| {
        let t = 0.5 + 2.5 * i as f64 / 4_000.0;
        // M = (0, 8), m = 0.25 * (-8, 0).
        Point2::new(40.0 - 2.0 * t.sin(), 8.0 * t.cos())
    }).collect();
    let d = hausdorff(&curve, &half);
    assert!(d <= CURVE_TOLERANCE_MM, "elliptic arc Hausdorff {d}");
}

#[test]
fn a_slender_ellipse_keeps_the_bound_at_its_tight_ends() {
    // Ratio 0.05: the radius of curvature at the major ends is 0.025 mm.
    let text = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
        0\nELLIPSE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n0\n40\n0.05\n41\n0\n42\n6.283185307179586\n0\nENDSEC\n0\nEOF\n";
    let doc = parse(text.as_bytes()).expect("slender ellipse reads");
    let out: Vec<Primitive> = doc.primitives.iter().map(|p| p.primitive.clone()).collect();
    smooth_closed_or_open(&out, true);
    let curve: Vec<Point2> = (0..=16_000).map(|i| {
        // Denser near the ends: t = s + 0.45 sin(2 s) spends more samples where y is small.
        let s = TAU * i as f64 / 16_000.0;
        let t = s - 0.45 * (2.0 * s).sin();
        Point2::new(10.0 * t.cos(), 0.5 * t.sin())
    }).collect();
    let d = hausdorff(&curve, &out);
    assert!(d <= CURVE_TOLERANCE_MM, "slender ellipse Hausdorff {d}");
}

#[test]
fn insert_and_point_are_skipped_and_the_rest_reads() {
    let text = "0\nSECTION\n2\nENTITIES\n\
        0\nINSERT\n8\n0\n66\n1\n2\nBOLT\n10\n5\n20\n5\n\
        0\nATTRIB\n8\n0\n1\nM6\n2\nSIZE\n10\n5\n20\n5\n\
        0\nSEQEND\n8\n0\n\
        0\nPOINT\n8\n0\n10\n1\n20\n1\n\
        0\nINSERT\n8\n0\n2\nNUT\n10\n9\n20\n9\n\
        0\nLINE\n8\ncut\n10\n0\n20\n0\n11\n10\n21\n0\n0\nENDSEC\n0\nEOF\n";
    let doc = parse(text.as_bytes()).expect("the drawing reads");
    assert_eq!(doc.primitives.len(), 1);
    assert_eq!(doc.summary.skipped.get("INSERT"), Some(&2));
    assert_eq!(doc.summary.skipped.get("POINT"), Some(&1));
    assert_eq!(doc.curve_tolerance, None);
    let open = text.replace("0\nSEQEND\n8\n0\n", "");
    let refused = parse(open.as_bytes()).expect_err("attributes without SEQEND");
    assert!(refused.0.contains("INSERT"), "{refused}");
}
