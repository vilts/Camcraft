//! #1880 review of `f96ea8fa0`: a `url(` token in a style ends where a browser ends it. A bad
//! url token (a quote, a `(` or a gap in an unquoted address) once hid the next declaration
//! from the check; a comment start in a good one did the same. The sheet and the inline style
//! use one scanner, so they refuse the same text.
//!
//! Provenance: our own tests; the cases are from CSS Syntax Level 3 (4.3.6 url tokens, 4.3.14
//! the remnants of a bad url) and the review of PR #3061.

use camcraft_geometry::dxf::Primitive;
use camcraft_geometry::svg;

const ROOT: &str = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100""#;
const RECT: &str = r#"<rect class="s" width="20" height="20"/>"#;
/// A second shape that no style of these tests names.
const FAR: &str = r#"<rect x="70" y="70" width="10" height="10"/>"#;

fn page(body: &str) -> String {
    format!("{ROOT}>{body}</svg>")
}

fn refuses(text: &str, name: &str) {
    let error = match svg::parse(text.as_bytes(), None) {
        Err(error) => error.to_string(),
        Ok(doc) => panic!("{text} must be refused; it imported {} primitives", doc.primitives.len()),
    };
    assert!(error.to_lowercase().contains(&name.to_lowercase()), "{text}: {error:?} does not name {name:?}");
}

/// The x extent of the imported lines: lowest and highest x.
fn x_range(text: &str) -> (f64, f64) {
    let doc = svg::parse(text.as_bytes(), None).unwrap_or_else(|e| panic!("{text}: {e}"));
    doc.primitives.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| match &p.primitive {
        Primitive::Line { start, end } => (lo.min(start.x).min(end.x), hi.max(start.x).max(end.x)),
        _ => (lo, hi),
    })
}

/// The style on a shape, on a group and on the root.
fn inline(style: &str) -> [String; 3] {
    [
        page(&format!(r#"<rect width="20" height="20" style="{style}"/>{FAR}"#)),
        page(&format!(r#"<g style="{style}"><rect width="20" height="20"/></g>"#)),
        format!(r#"{ROOT} style="{style}"><rect width="20" height="20"/></svg>"#),
    ]
}

#[test]
fn svg_style_sheet_bad_url_tokens_are_refused_by_name() {
    // The files of the review. A browser drops the `fill` declaration and applies the next
    // one: the rectangle is hidden, or drawn at 40 x 40.
    for sheet in [
        ".s{fill:url(a(b);transform:scale(2)}",
        ".s{fill:url(a(b);display:none}",
        ".s{fill:url(a'b);transform:scale(2)} .t{fill:'red'}",
        ".s{fill:url(a\"b);transform:scale(2)} .t{fill:\"red\"}",
        // `url` is not case-sensitive, and an escape can write it.
        ".s{fill:URL(a(b);display:none}",
        ".s{fill:\\75rl(a'b);transform:scale(2)} .t{fill:'red'}",
        ".s{fill:u\\52 l(a'b);transform:scale(2)} .t{fill:'red'}",
        // White space is permitted only before the `)`.
        ".s{fill:url(a b);transform:scale(2)}",
        ".s{fill:url( a'b );transform:scale(2)} .t{fill:'red'}",
    ] {
        for text in [
            page(&format!("<style>{sheet}</style>{RECT}")),
            page(&format!("<style><![CDATA[{sheet}]]></style>{RECT}")),
        ] {
            refuses(&text, "style sheet has a bad url( token");
        }
    }
}

#[test]
fn svg_font_face_block_with_a_bad_url_token_is_refused_by_name() {
    // The block of `@font-face` is not checked, and the quote once moved its end past the
    // rule that resizes the rectangle.
    for sheet in [
        "@font-face{src:url(a'b)} .s{transform:scale(2)} .t{fill:'x'}",
        "@font-face{src:url(a(b)} .s{display:none}",
        "@font-face{src:url(a'b)} .s{fill:red} .t{fill:'x'}",
    ] {
        refuses(&page(&format!("<style>{sheet}</style>{RECT}")), "style sheet has a bad url( token");
    }
}

#[test]
fn svg_inline_bad_url_tokens_are_refused_by_name_on_a_shape_a_group_and_the_root() {
    for style in [
        "fill:url(a(b);transform:scale(2)",
        "fill:url(a'b);transform:scale(2)",
        "fill:URL(a(b);transform:scale(2)",
        "fill:\\75rl(a'b);transform:scale(2)",
        "fill:url(a b);transform:scale(2)",
        // The bad token is the last declaration, or it has no property.
        "stroke:red;fill:url(a'b)",
        "url(a'b);stroke:red",
    ] {
        for text in inline(style) {
            refuses(&text, "inline CSS has a bad url( token");
        }
    }
}

#[test]
fn svg_inline_display_none_after_a_bad_url_token_hides_the_shape() {
    // A browser drops the `fill` declaration and applies `display:none`. The reader once
    // read the two as one declaration and imported the rectangle at 20 x 20.
    for style in [
        "fill:url(a(b);display:none",
        "fill:url(a'b);display:none",
        "fill:URL(a(b);display:none",
        "fill:\\75rl(a'b);display:none",
        "fill:url(a b);display:none",
        "url(a'b);display:none",
    ] {
        let [shape, group, root] = inline(style);
        // On the shape: only the far rectangle is imported.
        assert_eq!(x_range(&shape), (70.0, 80.0), "{style}");
        for text in [group, root] {
            let doc = svg::parse(text.as_bytes(), None).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert!(doc.primitives.is_empty(), "{style}: {} primitives", doc.primitives.len());
            assert_eq!(doc.summary.skipped.get("hidden"), Some(&1), "{style}: {:?}", doc.summary.skipped);
        }
    }
}

#[test]
fn svg_a_comment_start_in_a_url_token_hides_no_declaration() {
    // In an unquoted address `/*` is text. A browser reads the declaration between the two
    // addresses; the old scans read it as a comment.
    let hides = "fill:url(/*);display:none;stroke:url(*/)";
    let resizes = "fill:url(/*);transform:scale(2);stroke:url(*/)";
    refuses(&page(&format!("<style>.s{{{hides}}}</style>{RECT}")), "style sheet display");
    refuses(&page(&format!("<style>.s{{{resizes}}}</style>{RECT}")), "style sheet transform");
    for text in inline(resizes) {
        refuses(&text, "inline CSS transform");
    }
    // The hidden shape is skipped: only the far rectangle is imported.
    let text = page(&format!(r#"<rect width="20" height="20" style="{hides}"/>{FAR}"#));
    assert_eq!(x_range(&text), (70.0, 80.0));
}

#[test]
fn svg_good_url_tokens_import_at_the_attribute_size() {
    for style in [
        "fill:url(#a)",
        "fill: url( #a ) ;stroke:red",
        "fill:url(data:image/png;base64,AAAA);stroke:red",
        // A `;`, a brace or a comment start in an address is a part of the address.
        "fill:url(a;b);stroke:red",
        "fill:url(a/*b);stroke:url(c*/d)",
        // An escaped `)` or quote does not end the address or make it bad.
        "fill:url(a\\)b);stroke:red",
        "fill:url(a\\'b);stroke:red",
        // A quoted address is a string: a `(` or a `;` in it is text.
        "fill:url('a(b;c');stroke:red",
        "fill:url( 'a)b' );stroke:red",
        // Not the function `url`: the usual rules for a `(`.
        "fill:xurl(a);stroke:red",
        "fill:url (#a);stroke:red",
    ] {
        for text in inline(style) {
            let width = x_range(&text);
            assert_eq!(width.0, 0.0, "{style}");
            assert!(width.1 == 20.0 || width.1 == 80.0, "{style}: {width:?}");
        }
        let text = page(&format!("<style>.s{{{style}}}</style>{RECT}"));
        assert_eq!(x_range(&text), (0.0, 20.0), "{style}");
    }
    // The usual font rule of an editor file, and an address with a brace in a sheet.
    for sheet in [
        "@font-face{font-family:F;src:url(data:font/woff2;base64,AAAA)} .s{font-family:F}",
        "@font-face{src:url(a}b)} .s{fill:url(c{d)}",
    ] {
        assert_eq!(x_range(&page(&format!("<style>{sheet}</style>{RECT}"))), (0.0, 20.0), "{sheet}");
    }
}
