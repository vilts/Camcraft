//! #1880: an inline `style` can hold only properties that cannot change geometry, and a
//! `use` that draws an element again cannot make the reader hang (Prime Directive).
//!
//! Provenance: hand-built SVG text; the SVG 2 geometry properties and the vendor-prefixed
//! transform properties that browsers apply from a `style` attribute.
//! Gate map (M4.4): covers camcraft_geometry::svg.

use camcraft_geometry::dxf::{Limits, Primitive};
use camcraft_geometry::svg;
use std::time::{Duration, Instant};

const ROOT: &str = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100""#;

fn page(body: &str) -> String {
    format!("{ROOT}>{body}</svg>")
}

fn refuses(text: &str, name: &str) {
    let error = svg::parse(text.as_bytes(), None).expect_err("the style must be refused").to_string();
    assert!(error.to_lowercase().contains(&name.to_lowercase()), "{error:?} does not name {name:?}");
}

/// The x extent of the imported lines.
fn width(text: &str) -> f64 {
    let doc = svg::parse(text.as_bytes(), None).expect("the SVG reads");
    let (lo, hi) = doc.primitives.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| match &p.primitive {
        Primitive::Line { start, end } => (lo.min(start.x).min(end.x), hi.max(start.x).max(end.x)),
        _ => (lo, hi),
    });
    hi - lo
}

/// `style` on a shape, a group, a `use`, a referenced shape and the root.
fn everywhere(style: &str) -> [String; 5] {
    [
        page(&format!(r#"<rect width="20" height="20" style="{style}"/>"#)),
        page(&format!(r#"<g style="{style}"><rect width="20" height="20"/></g>"#)),
        page(&format!(r##"<defs><rect id="r" width="20" height="20"/></defs><use href="#r" style="{style}"/>"##)),
        page(&format!(r##"<defs><rect id="r" width="20" height="20" style="{style}"/></defs><use href="#r"/>"##)),
        format!(r#"{ROOT} style="{style}"><rect width="20" height="20"/></svg>"#),
    ]
}

#[test]
fn svg_vendor_prefixed_style_properties_are_refused_by_name() {
    // A browser draws each of these at 40 x 40 or moved. The reader does not apply them,
    // so it must refuse them, never import the shape at 20 x 20.
    for (style, name) in [
        ("-webkit-transform:scale(2)", "inline CSS -webkit-transform"),
        ("-WEBKIT-TRANSFORM: scale(2) !important", "inline CSS -webkit-transform"),
        (r"-webkit-tr\61 nsform:scale(2)", "inline CSS -webkit-transform"),
        ("fill:black; -webkit-transform-origin:10px 10px", "inline CSS -webkit-transform-origin"),
        ("-moz-transform:scale(2)", "inline CSS -moz-transform"),
        ("-ms-transform:scale(2)", "inline CSS -ms-transform"),
        ("zoom:2", "inline CSS zoom"),
        ("some-property-of-a-later-standard:1", "inline CSS some-property-of-a-later-standard"),
    ] {
        for text in everywhere(style) {
            refuses(&text, name);
        }
    }
}

#[test]
fn svg_geometry_in_an_inline_style_is_refused_by_name() {
    // In SVG 2 these are CSS properties, and a browser takes the value of the style.
    refuses(&page(r#"<rect width="20" height="20" style="width:40px"/>"#), "inline CSS width");
    refuses(&page(r#"<circle r="5" style="r:20px"/>"#), "inline CSS r ");
    refuses(
        &page(r#"<path d="M0 0 L10 0 L10 10 Z" style="d:path('M0 0 L50 0 L50 50 Z')"/>"#),
        "inline CSS d ",
    );
    for property in ["x", "y", "width", "height", "cx", "cy", "r", "rx", "ry", "d", "inline-size"] {
        for text in everywhere(&format!("fill:none; {property}:40px; stroke:black")) {
            refuses(&text, &format!("inline CSS {property} is not supported"));
        }
    }
    // A value that can move geometry is refused for a property that is safe with other values.
    refuses(
        &page(r#"<rect width="20" height="20" style="vector-effect:non-scaling-size"/>"#),
        "inline CSS vector-effect: non-scaling-size",
    );
}

#[test]
fn svg_paint_and_text_style_properties_import_at_the_attribute_size() {
    // The style of a usual editor file: paint and text properties only.
    let style = "opacity:1;fill:#ff0000;fill-opacity:1;fill-rule:evenodd;stroke:#000000;stroke-width:0.26458332;\
                 stroke-linecap:butt;stroke-linejoin:miter;stroke-miterlimit:4;stroke-dasharray:none;\
                 stroke-dashoffset:0;stroke-opacity:1;paint-order:normal;stop-color:#000000;color:#000000;\
                 marker:none;mix-blend-mode:normal;isolation:auto;overflow:visible;display:inline;\
                 font-size:10px;font-family:'sans-serif';-inkscape-font-specification:'sans-serif';\
                 vector-effect:none;VECTOR-EFFECT: Non-Scaling-Stroke";
    for text in everywhere(style) {
        assert!((width(&text) - 20.0).abs() < 1e-9, "{text}");
    }
    // An element that is skipped whole and counted is not checked: nothing of it is imported.
    let text = page(r#"<text style="inline-size:40px;shape-inside:url(#r)">A</text><rect width="20" height="20"/>"#);
    let doc = svg::parse(text.as_bytes(), None).expect("the text is skipped");
    assert_eq!(doc.primitives.len(), 4);
    assert_eq!(doc.summary.skipped.get("text"), Some(&1), "{:?}", doc.summary.skipped);
}

#[test]
fn svg_a_reused_element_reads_its_style_and_transform_one_time() {
    // Each visit of a `use` target once parsed its style and its transform again: 1 000 visits
    // of 2 MiB are 2e9 character steps. Read one time, the file must import in under five seconds.
    let blank = " ".repeat(1 << 20);
    let uses = r##"<use href="#t"/>"##.repeat(1_000);
    let text = page(&format!(
        r#"<defs><g id="t" style="{blank}fill:none" transform="{blank}scale(2)"><rect width="20" height="20"/></g></defs>{uses}"#
    ));
    let start = Instant::now();
    let doc = svg::parse(text.as_bytes(), None).expect("the reused group reads");
    let spent = start.elapsed();
    assert!(spent < Duration::from_secs(5), "{spent:?}");
    // The facts that are kept are the facts of the element: each copy is 40 wide.
    assert_eq!(doc.primitives.len(), 4_000);
    assert!((width(&text) - 40.0).abs() < 1e-9);
}

#[test]
fn svg_many_use_references_find_their_target_in_linear_time() {
    // Each `use` once searched all nodes for its `id`: 20 000 references in a file of
    // 40 000 elements are 8e8 lookups. With a map of the ids it must read in under five seconds.
    let others: String = (0..20_000).map(|i| format!(r#"<desc id="d{i}"/>"#)).collect();
    let uses = r##"<use href="#missing"/>"##.repeat(20_000);
    let twin = r#"<rect id="r" width="20" height="20"/><rect id="r" width="90" height="90"/>"#;
    let text = page(&format!(r##"<defs>{others}{twin}</defs>{uses}<use href="#r"/>"##));
    let start = Instant::now();
    let doc = svg::parse(text.as_bytes(), None).expect("the references read");
    let spent = start.elapsed();
    assert!(spent < Duration::from_secs(5), "{spent:?}");
    assert_eq!(doc.summary.skipped.get("use"), Some(&20_000), "{:?}", doc.summary.skipped);
    // The first element of a repeated id is the target, as before.
    assert_eq!(doc.primitives.len(), 4);
    assert!((width(&text) - 20.0).abs() < 1e-9);
}

#[test]
fn svg_reused_shape_data_is_charged_against_the_work_budget() {
    // The path data of a `use` target is read at each visit. 1 MiB of blank path data and
    // 2 000 references are 2e9 character steps: the work budget must refuse them by name.
    let blank = " ".repeat(1 << 20);
    let uses = r##"<use href="#p"/>"##.repeat(2_000);
    let text = page(&format!(r#"<defs><path id="p" d="M0 0{blank}"/></defs>{uses}"#));
    let limits = Limits { max_bytes: 4 << 20, ..Limits::default() };
    let start = Instant::now();
    let error = svg::parse_with_limits(text.as_bytes(), None, limits).expect_err("the work budget").to_string();
    let spent = start.elapsed();
    assert!(error.contains("import budget") && error.contains("reference"), "{error}");
    assert!(spent < Duration::from_secs(5), "{spent:?}");
    // One pass over a file is never charged more than its size: the same data read one time imports.
    let once = page(&format!(r#"<path d="M0 0{blank}L10 0"/>"#));
    assert_eq!(svg::parse_with_limits(once.as_bytes(), None, limits).expect("one pass").primitives.len(), 1);
}
