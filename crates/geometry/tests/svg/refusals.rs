//! #1880: malformed SVG input is refused by name and never panics or hangs (Prime Directive).
//!
//! Provenance: hand-built malformed SVG text and deterministic byte mutations.
//! Gate map (M4.4): covers camcraft_geometry::svg.

use camcraft_geometry::dxf::{Limits, Primitive};
use camcraft_geometry::svg;

fn page(body: &str) -> String {
    format!(r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100">{body}</svg>"#)
}

fn refuses(text: &str, name: &str) {
    let error = svg::parse(text.as_bytes(), None).expect_err("bad SVG must be refused").to_string();
    assert!(error.to_lowercase().contains(&name.to_lowercase()), "{error:?} does not name {name:?}");
}

#[test]
fn svg_malformed_documents_are_named() {
    refuses("", "no <svg>");
    refuses("hello", "no <svg>");
    refuses("<html></html>", "no <svg>");
    refuses("<svg", "unterminated");
    refuses("<svg><g></svg>", "does not match");
    refuses("<svg><rect/>", "not closed");
    refuses("<svg><!-- open", "unterminated comment");
    refuses("<svg width=100mm></svg>", "quoted");
    let gzip: &[u8] = b"\x1f\x8b\x08\0";
    assert!(svg::parse(gzip, None).expect_err("svgz").to_string().contains("compressed"));
    let bad_utf8: &[u8] = b"<svg>\xff</svg>";
    assert!(svg::parse(bad_utf8, None).expect_err("not UTF-8").to_string().contains("UTF-8"));
}

#[test]
fn svg_malformed_paths_and_shapes_are_named() {
    refuses(&page(r#"<path d="L10 10"/>"#), "malformed path");
    refuses(&page(r#"<path d="M10 10 L20"/>"#), "malformed path");
    refuses(&page(r#"<path d="M10 10 X20 20"/>"#), "malformed path");
    refuses(&page(r#"<path d="M0 0 A0 5 0 0 1 10 10"/>"#), "degenerate arc from (0, 0) to (10, 10): a radius is zero");
    refuses(&page(r#"<path d="M0 0 A5 5 0 2 1 10 10"/>"#), "malformed path");
    refuses(&page(r#"<rect width="-5" height="5"/>"#), "negative");
    refuses(&page(r#"<circle r="-1"/>"#), "negative");
    refuses(&page(r#"<polyline points="0,0 10"/>"#), "points");
    refuses(&page(r#"<rect width="5" height="5" transform="rotate(1 2)"/>"#), "transform");
    refuses(&page(r#"<rect width="5" height="5" transform="scale(0)"/>"#), "transform");
    refuses(&page(r#"<rect width="50%" height="5"/>"#), "percent");
    refuses(&page(r#"<line x2="1e400"/>"#), "finite");
}

#[test]
fn svg_zero_or_negative_document_sizes_are_refused() {
    refuses(r#"<svg width="0mm" height="10mm"><rect width="1" height="1"/></svg>"#, "zero or negative");
    refuses(r#"<svg width="10mm" height="-1mm"><rect width="1" height="1"/></svg>"#, "zero or negative");
    refuses(r#"<svg viewBox="0 0 0 10"><rect width="1" height="1"/></svg>"#, "viewBox");
    refuses(r#"<svg viewBox="0 0 10"><rect width="1" height="1"/></svg>"#, "viewBox");
    refuses(r#"<svg width="10mm"><rect width="1" height="1"/></svg>"#, "no height");
}

#[test]
fn svg_import_budget_is_enforced() {
    let small = Limits { max_bytes: 64, max_entities: 4, max_vertices: 16 };
    let big = page(&"<line x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"/>".repeat(4));
    let error = svg::parse_with_limits(big.as_bytes(), None, small).expect_err("bytes").to_string();
    assert!(error.contains("import budget"), "{error}");
    let roomy = Limits { max_bytes: 1 << 20, ..small };
    let many = page(&"<line x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"/>".repeat(8));
    let error = svg::parse_with_limits(many.as_bytes(), None, roomy).expect_err("elements").to_string();
    assert!(error.contains("import budget"), "{error}");
    let curve = page(r#"<circle r="40" transform="scale(1 0.5)"/>"#);
    let error = svg::parse_with_limits(curve.as_bytes(), None, roomy).expect_err("vertices").to_string();
    assert!(error.contains("import budget"), "{error}");
    let deep = format!("<svg width=\"1mm\" height=\"1mm\">{}{}</svg>", "<g>".repeat(5000), "</g>".repeat(5000));
    let error = svg::parse(deep.as_bytes(), None).expect_err("depth").to_string();
    assert!(error.contains("nested"), "{error}");
    // A `use` that refers to itself is cut off, not followed for ever.
    let looped = page(r##"<g id="a"><use href="#a"/><line x2="1"/></g>"##);
    let error = svg::parse(looped.as_bytes(), None).expect_err("loop").to_string();
    assert!(error.contains("use"), "{error}");
}

#[test]
fn svg_short_curve_vertices_cannot_silently_break_the_chord_bound() {
    // Removing the inner vertices makes the midpoint 0.01 mm from the emitted chord.
    // This exceeds the 0.005 mm bound, although the endpoints can form an open chain.
    let text = r#"<svg width="1mm" height="1mm" viewBox="0 0 1 1"><path d="M0 0 Q0 0.02 0.02 0"/></svg>"#;
    refuses(text, "curve");
    refuses(text, "chord");
}

#[test]
fn svg_beziers_above_the_segment_cap_refuse_the_import_budget() {
    for path in [
        "M0 0 Q0 100000000 100000000 0",
        "M0 0 C0 100000000 100000000 100000000 100000000 0",
    ] {
        let text = format!(
            r#"<svg width="100000000mm" height="100000000mm" viewBox="0 0 100000000 100000000"><path d="{path}"/></svg>"#
        );
        refuses(&text, "import budget");
    }
}

#[test]
fn svg_ellipses_above_the_segment_cap_refuse_the_import_budget() {
    let text = r#"<svg width="100000000mm" height="100000000mm" viewBox="0 0 100000000 100000000"><ellipse cx="50000000" cy="50000000" rx="40000000" ry="20000000"/></svg>"#;
    refuses(text, "import budget");
}

#[test]
fn svg_symbol_viewport_cannot_silently_ignore_the_use_size() {
    // The symbol is 10 units wide, but its referenced viewport must be 50 mm wide.
    let text = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100mm" height="100mm" viewBox="0 0 100 100">
      <defs><symbol id="s" viewBox="0 0 10 10"><rect width="10" height="10"/></symbol></defs>
      <use xlink:href="#s" width="50" height="50"/>
    </svg>"##;
    refuses(text, "symbol");
    refuses(text, "viewport");
}

#[test]
fn svg_acyclic_shared_references_refuse_excess_traversal_work() {
    let mut body = String::from(r#"<defs><g id="g0"/>"#);
    for level in 1..=6 {
        body.push_str(&format!(r#"<g id="g{level}">"#));
        for _ in 0..10 {
            body.push_str(&format!(r##"<use href="#g{}"/>"##, level - 1));
        }
        body.push_str("</g>");
    }
    body.push_str(r##"</defs><use href="#g6"/><rect width="1" height="1"/>"##);
    let text = page(&body);
    // Fewer than 100 XML elements expand to over a million empty group visits.
    assert!(text.matches('<').count() < 100);
    let limits = Limits { max_bytes: 1 << 20, max_entities: 100, max_vertices: 10 };
    let error = svg::parse_with_limits(text.as_bytes(), None, limits).expect_err("traversal budget").to_string();
    assert!(error.contains("import budget") && error.contains("traversal"), "{error}");
}

#[test]
fn svg_combined_group_and_reference_depth_is_refused_without_stack_overflow() {
    // Each XML subtree and reference chain is legal by itself. Their combined
    // active traversal must be refused before it exhausts a normal test stack.
    let mut body = String::from("<defs>");
    for level in 0..16 {
        body.push_str(&format!(r#"<g id="g{level}">"#));
        body.push_str(&"<g>".repeat(230));
        if level < 15 {
            body.push_str(&format!(r##"<use href="#g{}"/>"##, level + 1));
        } else {
            body.push_str(r#"<rect width="10" height="10"/>"#);
        }
        body.push_str(&"</g>".repeat(231));
    }
    body.push_str(r##"</defs><use href="#g0"/>"##);
    refuses(&page(&body), "depth");
}

#[test]
fn svg_near_circular_large_ellipse_cannot_exceed_the_absolute_chord_bound() {
    // The old relative test emitted a circle 0.0095 mm from each axis.
    // The 0.005 mm chord bound requires flattening, which exceeds this budget.
    let text = r#"<svg width="100000000mm" height="100000000mm" viewBox="0 0 100000000 100000000"><ellipse rx="40000000" ry="40000000.019"/></svg>"#;
    refuses(text, "import budget");
}

#[test]
fn svg_root_transform_attribute_is_refused_by_name() {
    for attr in [r#"transform="scale(2)""#, r#"transform="translate(10, 20)""#] {
        let text = format!(
            r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100" {attr}><rect width="20" height="20"/></svg>"#
        );
        refuses(&text, "root <svg>");
        refuses(&text, "transform");
    }
}

#[test]
fn svg_inline_css_transform_is_refused_by_name() {
    for style in [
        "transform: scale(2)",
        "/* move */transform:scale(2)",
        r"tr\61 nsform:scale(2)",
        "fill: black; TRANSFORM: translate(10px, 20px) !important",
    ] {
        let on_root = format!(
            r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100" style="{style}"><rect width="20" height="20"/></svg>"#
        );
        let on_group = page(&format!(r#"<g style="{style}"><rect width="20" height="20"/></g>"#));
        let on_use = page(&format!(
            r##"<defs><rect id="r" width="20" height="20"/></defs><use href="#r" style="{style}"/>"##
        ));
        let on_shape = page(&format!(r#"<rect width="20" height="20" style="{style}"/>"#));
        for text in [on_root, on_group, on_use, on_shape] {
            refuses(&text, "inline CSS transform");
        }
    }
    // Comments or strings that only mention `transform` must not trigger a false refusal.
    let benign = page(
        r#"<rect width="20" height="20" style="/* transform: scale(2) */ font-family: 'transform: scale(2)'; fill: black"/>"#,
    );
    let doc = svg::parse(benign.as_bytes(), None).expect("benign comment/string reads");
    assert_eq!(doc.primitives.len(), 4);
}

#[test]
fn svg_transform_origin_attribute_is_refused_by_name() {
    let origin = r#"transform-origin="10 10""#;
    let on_root = format!(
        r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100" {origin}><rect width="20" height="20" transform="scale(2)"/></svg>"#
    );
    let on_group = page(&format!(
        r#"<g transform="scale(2)" {origin}><rect x="10" y="10" width="20" height="20"/></g>"#
    ));
    let on_use = page(&format!(
        r##"<defs><rect id="r" x="10" y="10" width="20" height="20"/></defs><use href="#r" transform="scale(2)" {origin}/>"##
    ));
    let on_shape = page(&format!(
        r#"<rect x="10" y="10" width="20" height="20" transform="scale(2)" {origin}/>"#
    ));
    let on_referenced_shape = page(&format!(
        r##"<defs><rect id="r" x="10" y="10" width="20" height="20" transform="scale(2)" {origin}/></defs><use href="#r"/>"##
    ));
    for text in [on_root, on_group, on_use, on_shape, on_referenced_shape] {
        refuses(&text, "transform-origin presentation attribute");
    }
}

#[test]
fn svg_inline_css_transform_origin_is_refused_by_name() {
    for style in [
        "transform-origin:10px 10px",
        "/* origin */transform-origin:10px 10px",
        r"transform-ori\67 in:10px 10px",
        "fill:black; TRANSFORM-ORIGIN:10px 10px !important",
    ] {
        let on_root = format!(
            r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100" style="{style}"><rect width="20" height="20" transform="scale(2)"/></svg>"#
        );
        let on_group = page(&format!(
            r#"<g transform="scale(2)" style="{style}"><rect x="10" y="10" width="20" height="20"/></g>"#
        ));
        let on_use = page(&format!(
            r##"<defs><rect id="r" x="10" y="10" width="20" height="20"/></defs><use href="#r" transform="scale(2)" style="{style}"/>"##
        ));
        let on_shape = page(&format!(
            r#"<rect x="10" y="10" width="20" height="20" transform="scale(2)" style="{style}"/>"#
        ));
        let on_referenced_shape = page(&format!(
            r##"<defs><rect id="r" x="10" y="10" width="20" height="20" transform="scale(2)" style="{style}"/></defs><use href="#r"/>"##
        ));
        for text in [on_root, on_group, on_use, on_shape, on_referenced_shape] {
            refuses(&text, "inline CSS transform-origin");
        }
    }
    let benign = page(
        r#"<rect width="20" height="20" transform="scale(2)" style="/* transform-origin:10px 10px */ font-family:'transform-origin:10px 10px'; fill:black"/>"#,
    );
    let doc = svg::parse(benign.as_bytes(), None).expect("benign comment/string reads");
    assert_eq!(doc.primitives.len(), 4);
}

#[test]
fn svg_more_transform_sources_are_refused_by_name() {
    // CSS Transforms 1 and 2 and CSS Motion Path move geometry through these properties.
    // The reader does not apply them, so it must refuse them, never cut in the wrong place.
    let attr = page(r#"<rect x="10" y="10" width="20" height="20" transform="scale(2)" transform-box="fill-box"/>"#);
    refuses(&attr, "transform-box presentation attribute");
    for (style, name) in [
        ("transform-box:fill-box", "inline CSS transform-box"),
        ("scale:2", "inline CSS scale"),
        ("translate:10px 10px", "inline CSS translate"),
        ("ROTATE:45deg", "inline CSS rotate"),
        ("offset-path:path('M0 0 L10 10')", "inline CSS offset-path"),
        ("offset:path('M0 0 L10 10') 50%", "inline CSS offset"),
    ] {
        let on_shape = page(&format!(r#"<rect width="20" height="20" style="{style}"/>"#));
        let on_group = page(&format!(r#"<g style="{style}"><rect width="20" height="20"/></g>"#));
        let on_root = format!(
            r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100" style="{style}"><rect width="20" height="20"/></svg>"#
        );
        for text in [on_shape, on_group, on_root] {
            refuses(&text, name);
        }
    }
    // The `rotate` attribute of <text> is a glyph rotation, not a transform: the text is noted.
    let text = page(r#"<text rotate="30">A</text><rect width="20" height="20"/>"#);
    let doc = svg::parse(text.as_bytes(), None).expect("text rotate reads");
    assert_eq!(doc.primitives.len(), 4);
    assert_eq!(doc.summary.skipped.get("text"), Some(&1), "{:?}", doc.summary.skipped);
}

#[test]
fn svg_many_ampersands_in_an_attribute_read_in_linear_time() {
    // Each `&` once scanned the whole rest of the attribute for a `;`: quadratic time,
    // hours of CPU for a 16 MiB file. 1 MiB must read in well under five seconds.
    let text = format!(
        r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10" a="{}"><rect width="1" height="1"/></svg>"#,
        "&".repeat(1 << 20)
    );
    let start = std::time::Instant::now();
    let doc = svg::parse(text.as_bytes(), None).expect("the ampersands read as text");
    assert_eq!(doc.primitives.len(), 4);
    let spent = start.elapsed();
    assert!(spent < std::time::Duration::from_secs(5), "{spent:?}");
}

#[test]
fn svg_many_attributes_on_a_tag_read_in_linear_time() {
    // Each attribute once scanned all earlier attributes of its tag for a duplicate name:
    // quadratic time, about 35 minutes for a 16 MiB tag. 100 000 must read in under five seconds.
    let many: String = (0..100_000).map(|i| format!(r#" a{i}="1""#)).collect();
    let text = page(&format!(r#"<rect width="2" height="1"{many} width="9"/>"#));
    let start = std::time::Instant::now();
    let doc = svg::parse(text.as_bytes(), None).expect("the attributes read");
    let spent = start.elapsed();
    assert!(spent < std::time::Duration::from_secs(5), "{spent:?}");
    // The first value of a repeated attribute stays, as before: the rect is 2 wide, not 9.
    assert_eq!(doc.primitives.len(), 4);
    let (lo, hi) = doc.primitives.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| match &p.primitive {
        Primitive::Line { start, end } => (lo.min(start.x).min(end.x), hi.max(start.x).max(end.x)),
        _ => (lo, hi),
    });
    assert!((hi - lo - 2.0).abs() < 1e-9, "width {}", hi - lo);
}

#[test]
fn svg_deterministic_smoke_fuzz_never_panics() {
    let seed = page(
        r#"<g transform="translate(5 5) rotate(30)"><path d="M0 0 C10 0 10 10 0 10 S-10 20 0 20 Q5 25 10 20 T20 20 A5 3 20 1 0 30 30 Z"/>
        <rect x="1" y="1" width="8" height="4" rx="2" ry="1"/><ellipse rx="3" ry="2"/><polygon points="0,0 4,0 4,4"/></g>"#,
    )
    .into_bytes();
    for cut in 0..seed.len() {
        let _ = svg::parse(&seed[..cut], None);
    }
    let mut state = 0x1880u64;
    for _ in 0..1000 {
        let mut bytes = seed.clone();
        for _ in 0..4 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let index = (state as usize) % bytes.len();
            bytes[index] = (state >> 32) as u8;
        }
        let _ = svg::parse(&bytes, None);
        let _ = svg::parse(&bytes, Some(camcraft_geometry::dxf::Unit::Inches));
    }
}
