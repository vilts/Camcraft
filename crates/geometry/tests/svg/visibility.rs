//! #1880: hidden SVG contours and unselected switch alternatives must not become cuts.
//! Provenance: hand-written fixtures from the W3C SVG 1.1 display and switch rules,
//! and CSS case-insensitive property names, keywords and importance syntax.
//! Gate map (M4.4): covers camcraft_geometry::svg and camcraft_geometry::sketch_data.

use camcraft_geometry::sketch_data::SketchData;
use camcraft_geometry::svg;

const HIDDEN_STYLES: &[&str] = &[
    "display:none !important",
    "DISPLAY: none",
    "fill: black; DiSpLaY : NoNe ! IMPORTANT ; stroke: none",
    "display:none",
];
const VISIBLE_RECT: &str = r#"<rect x="70" y="70" width="10" height="10"/>"#;
const VISIBILITY_DECLARATIONS: &[&str] = &[
    r#"visibility="hidden""#,
    r#"style="visibility:hidden""#,
    r#"style="VISIBILITY: hidden !important""#,
    r#"style="fill: black; ViSiBiLiTy : collapse ! IMPORTANT; stroke: none""#,
    r#"visibility="visible""#,
    r#"style="visibility:inherit""#,
    r#"visibility="hidden" display="none""#,
    r#"style="display:none; visibility:hidden""#,
];
const COMMENT_AND_ESCAPE_CASES: &[(&str, &str)] = &[
    ("display:/* hidden */none", "hidden"),
    ("/* hidden */display:none", "hidden"),
    (r"display:n\6f ne", "hidden"),
    (r"displ\61 y:none", "hidden"),
    (r"display:/* c */n\00006fne !/* c */imp\6f rtant", "hidden"),
    (r"font-family:'a;b';/* ; */di\splay:no\ne", "hidden"),
    ("/* hidden */visibility:hidden", "visibility"),
    (r"visib\69 lity:hidden", "visibility"),
    ("/* ; */visibility/* c */:/* c */visible", "visibility"),
];

fn page(body: &str) -> String {
    format!(r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100">{body}</svg>"#)
}

fn assert_only_visible_sibling(text: &str, skipped: &str) {
    let doc = svg::parse(text.as_bytes(), None).expect("the SVG reads");
    assert_eq!(doc.summary.skipped.get(skipped), Some(&1), "{text}");
    assert_eq!(doc.summary.skipped.len(), 1, "{text}");
    assert_eq!(doc.primitives.len(), 4, "only the visible rectangle emits lines: {text}");
    let data = SketchData::from_dxf(doc, None, None).expect("the visible rectangle chains");
    let chains = data.chains();
    assert_eq!((chains.closed.len(), chains.open.len(), chains.rejected.len()), (1, 0, 0), "{text}");
    let (lo, hi) = data.bounds().expect("the visible rectangle has bounds");
    assert_eq!((lo.x, lo.y, hi.x, hi.y), (70.0, 20.0, 80.0, 30.0), "{text}");
}

#[test]
fn svg_root_inline_display_none_with_importance_and_case_skips_all_geometry() {
    for style in HIDDEN_STYLES {
        let text = format!(
            r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100" style="{style}"><rect width="20" height="20"/>{VISIBLE_RECT}</svg>"#
        );
        let doc = svg::parse(text.as_bytes(), None).expect("the hidden root reads");
        assert!(doc.primitives.is_empty(), "hidden root emits no geometry: {style}");
        assert_eq!(doc.summary.skipped.get("hidden"), Some(&1), "{style}");
        assert_eq!(doc.summary.skipped.len(), 1, "{style}");
    }
}

#[test]
fn svg_group_inline_display_none_with_importance_and_case_skips_descendants() {
    for style in HIDDEN_STYLES {
        let text = page(&format!(
            r#"<g style="{style}"><rect width="20" height="20"/><g><rect x="30" width="20" height="20"/></g></g>{VISIBLE_RECT}"#
        ));
        assert_only_visible_sibling(&text, "hidden");
    }
}

#[test]
fn svg_shape_inline_display_none_with_importance_and_case_skips_its_contour() {
    for style in HIDDEN_STYLES {
        let text = page(&format!(r#"<rect width="20" height="20" style="{style}"/>{VISIBLE_RECT}"#));
        assert_only_visible_sibling(&text, "hidden");
    }
}

#[test]
fn svg_switch_reports_unsupported_without_importing_any_alternative() {
    let switch = r#"<switch><rect width="20" height="20"/><rect x="30" width="20" height="20"/></switch>"#;
    let text = page(&format!("{switch}{VISIBLE_RECT}"));
    assert_only_visible_sibling(&text, "switch");

    let doc = svg::parse(page(switch).as_bytes(), None).expect("the unsupported switch reads");
    assert!(doc.primitives.is_empty(), "no switch alternative emits geometry");
    assert_eq!(doc.summary.skipped.get("switch"), Some(&1));
    assert_eq!(doc.summary.skipped.len(), 1);
}

#[test]
fn svg_root_visibility_is_reported_without_importing_descendants_or_overrides() {
    for declaration in VISIBILITY_DECLARATIONS {
        let text = format!(
            r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100" {declaration}><rect width="20" height="20"/><g visibility="visible"><rect x="30" width="20" height="20"/></g></svg>"#
        );
        let doc = svg::parse(text.as_bytes(), None).expect("the unsupported root reads");
        assert!(doc.primitives.is_empty(), "unsupported root emits no geometry: {declaration}");
        assert_eq!(doc.summary.skipped.get("visibility"), Some(&1), "{declaration}");
        assert_eq!(doc.summary.skipped.len(), 1, "{declaration}");
    }
}

#[test]
fn svg_group_visibility_is_reported_without_importing_descendants_or_overrides() {
    for declaration in VISIBILITY_DECLARATIONS {
        let text = page(&format!(
            r#"<g {declaration}><rect width="20" height="20"/><g style="visibility:visible"><rect x="30" width="20" height="20"/></g></g>{VISIBLE_RECT}"#
        ));
        assert_only_visible_sibling(&text, "visibility");
    }
}

#[test]
fn svg_shape_visibility_is_reported_without_importing_its_contour() {
    for declaration in VISIBILITY_DECLARATIONS {
        let text = page(&format!(r#"<rect width="20" height="20" {declaration}/>{VISIBLE_RECT}"#));
        assert_only_visible_sibling(&text, "visibility");
    }
}

#[test]
fn svg_root_inline_css_comments_and_escapes_skip_hidden_and_visibility_subtrees() {
    for &(style, skipped) in COMMENT_AND_ESCAPE_CASES {
        let text = format!(
            r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100" style="{style}"><rect width="20" height="20"/>{VISIBLE_RECT}</svg>"#
        );
        let doc = svg::parse(text.as_bytes(), None).expect("the root with CSS comments/escapes reads");
        assert!(doc.primitives.is_empty(), "root emits no geometry: {style}");
        assert_eq!(doc.summary.skipped.get(skipped), Some(&1), "{style}");
        assert_eq!(doc.summary.skipped.len(), 1, "{style}");
    }
}

#[test]
fn svg_group_inline_css_comments_and_escapes_skip_hidden_and_visibility_subtrees() {
    for &(style, skipped) in COMMENT_AND_ESCAPE_CASES {
        let text = page(&format!(
            r#"<g style="{style}"><rect width="20" height="20"/><g style="visibility:visible"><rect x="30" width="20" height="20"/></g></g>{VISIBLE_RECT}"#
        ));
        assert_only_visible_sibling(&text, skipped);
    }
}

#[test]
fn svg_shape_inline_css_comments_and_escapes_skip_hidden_and_visibility_contours() {
    for &(style, skipped) in COMMENT_AND_ESCAPE_CASES {
        let text = page(&format!(r#"<rect width="20" height="20" style="{style}"/>{VISIBLE_RECT}"#));
        assert_only_visible_sibling(&text, skipped);
    }
}

#[test]
fn svg_benign_css_comments_strings_and_non_matching_escapes_keep_visible_geometry() {
    for style in [
        "/* display:none; visibility:hidden */ fill:black",
        "font-family:'display:none;visibility:hidden'; stroke:black",
        r"display:\20none",
        r"display:n\6f  ne",
        "di/* c */splay:none",
    ] {
        let text = page(&format!(r#"<rect x="70" y="70" width="10" height="10" style="{style}"/>"#));
        let doc = svg::parse(text.as_bytes(), None).expect("the SVG reads");
        assert!(doc.summary.skipped.is_empty(), "{style}: {:?}", doc.summary.skipped);
        assert_eq!(doc.primitives.len(), 4, "{style}");
        let data = SketchData::from_dxf(doc, None, None).expect("the visible rectangle chains");
        assert_eq!(data.chains().closed.len(), 1, "{style}");
    }
}
