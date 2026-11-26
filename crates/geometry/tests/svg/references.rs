//! #1880 review of `f96ea8fa0`: a reference (`&name;`, `&#10;`, `&#xA;`) is a character that
//! the reader decodes, or the file is refused by name. An entity whose name has a character
//! outside the ASCII name set, and a character reference with many leading zeros, once went
//! through as text where a browser puts a value.
//!
//! Provenance: our own tests; the cases are from the W3C XML 1.0 Recommendation (2.3 names,
//! 4.1 references) and the review of PR #3061.

use camcraft_geometry::dxf::Primitive;
use camcraft_geometry::svg;
use std::time::{Duration, Instant};

const ROOT: &str = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100""#;
const RECT: &str = r#"<rect class="s" width="20" height="20"/>"#;
/// A second shape that no reference of these tests changes.
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

#[test]
fn svg_entity_names_outside_ascii_are_refused_at_each_place() {
    // An XML name can have these characters. A browser expands the entity: the rectangle is
    // hidden, or drawn at 40 x 40.
    for name in ["\u{20ac}", "a\u{b7}b", "a\u{301}", "\u{2030}x", "a\u{203f}b", "\u{3a9}"] {
        let doctype = format!(
            r#"<!DOCTYPE svg [ <!ENTITY {name} "none"> <!ENTITY s{name} "transform:scale(2)"> ]>"#
        );
        let file = |body: &str| format!("{doctype}{ROOT}>{body}</svg>");
        let named = format!("entity reference &{name};");
        refuses(&file(&format!(r#"<rect width="20" height="20" display="&{name};"/>"#)), &named);
        refuses(&file(&format!(r#"<rect width="20" height="20" style="display:&{name};"/>"#)), &named);
        refuses(&file(&format!(r#"<g style="&s{name};">{RECT}</g>"#)), "entity reference &s");
        refuses(&file(&format!("<style>.s{{display:&{name};}}</style>{RECT}")), &named);
        refuses(&file(&format!("<style>.s{{&s{name};}}</style>{RECT}")), "entity reference &s");
        refuses(&file(&format!("&{name};{RECT}")), &format!("{named} in the content"));
        refuses(&file(&format!("<g>{RECT}&{name};</g>")), &named);
    }
}

#[test]
fn svg_entity_names_with_an_ogham_space_are_refused_at_each_place() {
    // U+1680 is a name character in XML, not XML whitespace (2.3, `S` and `NameStartChar`).
    // A browser expands `&a\u{1680}b;`: the rectangle is hidden, or drawn at 40 x 40.
    let doctype = "<!DOCTYPE svg [ <!ENTITY a\u{1680}b \"none\"> <!ENTITY s\u{1680}b \"transform:scale(2)\"> \
                   <!ENTITY r\u{1680}b '<rect x=\"50\" width=\"20\" height=\"20\"/>'> ]>";
    let file = |body: &str| format!("{doctype}{ROOT}>{body}</svg>");
    let named = "entity reference &a\u{1680}b;";
    refuses(&file(&format!("<rect width=\"20\" height=\"20\" display=\"&a\u{1680}b;\"/>{FAR}")), named);
    refuses(&file(&format!("<rect width=\"20\" height=\"20\" style=\"&s\u{1680}b;\"/>{FAR}")), "entity reference &s\u{1680}b;");
    refuses(&file(&format!("<style>.s{{&s\u{1680}b;}}</style>{RECT}")), "entity reference &s\u{1680}b;");
    refuses(&file(&format!("&r\u{1680}b;{FAR}")), "entity reference &r\u{1680}b; in the content");
}

#[test]
fn svg_a_run_that_is_not_an_xml_name_is_refused_when_it_ends_at_a_semicolon() {
    // Such a file is not well-formed: a browser does not draw it.
    for run in ["a=b", "a/b", "a(b)", "1a", "a,b", "#", "#x", "#xG", "#1a", "#+65", "#-65", "#X41"] {
        let text = page(&format!(r#"<rect width="20" height="20" display="&{run};"/>"#));
        refuses(&text, &format!("reference &{run};"));
        refuses(&page(&format!("<title>&{run};</title>{RECT}")), &format!("reference &{run};"));
    }
}

#[test]
fn svg_character_references_with_leading_zeros_are_decoded_in_an_attribute() {
    // `display="none"`, written with a reference that is longer than the old 12-byte window.
    for display in ["&#x0000000006E;one", "&#00000000110;one", "n&#x00000000000000000000000000006F;ne"] {
        let text = page(&format!(r#"<rect width="20" height="20" display="{display}"/>{FAR}"#));
        assert_eq!(x_range(&text), (70.0, 80.0), "{display}");
    }
    let text = page(&format!(r#"<rect width="20" height="20" style="display:n&#x00000000006F;ne"/>{FAR}"#));
    assert_eq!(x_range(&text), (70.0, 80.0));
    for style in ["transform&#x00000000003A;scale(2)", "transform&#00000000058;scale(2)"] {
        refuses(&page(&format!(r#"<rect width="20" height="20" style="{style}"/>"#)), "inline CSS transform");
        refuses(&page(&format!(r#"<g style="{style}">{RECT}</g>"#)), "inline CSS transform");
        refuses(&format!(r#"{ROOT} style="{style}">{RECT}</svg>"#), "inline CSS transform");
    }
}

#[test]
fn svg_character_references_with_leading_zeros_are_decoded_in_a_style_sheet() {
    for colon in ["&#58;", "&#x3A;", "&#x00000000003A;", "&#00000000000000058;"] {
        refuses(&page(&format!("<style>.s{{transform{colon}scale(2)}}</style>{RECT}")), "style sheet transform");
        refuses(&page(&format!("<style>.s{{display{colon}none}}</style>{RECT}")), "style sheet display");
    }
    // A paint rule that is written with a long reference still imports.
    let text = page(&format!("<style>.s{{fill&#x00000000003A;red}}</style>{RECT}"));
    assert_eq!(x_range(&text), (0.0, 20.0));
}

#[test]
fn svg_character_references_that_are_not_a_character_are_refused_by_name() {
    for code in ["#0", "#x0", "#8", "#x1F", "#xD800", "#xDFFF", "#xFFFE", "#x110000", "#99999999999999999999", "#xFFFFFFFFF"] {
        let named = format!("character reference &{code};");
        refuses(&page(&format!(r#"<rect width="20" height="20" id="&{code};"/>"#)), &named);
        refuses(&page(&format!(r#"<rect width="20" height="20" style="fill:&{code};"/>"#)), &named);
        refuses(&page(&format!("<style>.s{{fill:&{code};}}</style>{RECT}")), &named);
        refuses(&page(&format!("<title>&{code};</title>{RECT}")), &format!("{named} in the content"));
    }
}

#[test]
fn svg_predefined_and_plain_references_read_as_before() {
    let text = page(
        r#"<title>a &amp; b &lt; &gt; &quot; &apos; &#65; &#x41; &#x000000000041; &#x10FFFF; &#9; & c &; &a b; &a&amp;</title>
           <rect width="20" height="20" id="a&amp;b &#65; &#0000000000065; & c &; &a b &a<"/>"#,
    );
    assert_eq!(x_range(&text), (0.0, 20.0));
    // A namespace declaration is not read, and a CDATA section has no references.
    let doctype = r#"<!DOCTYPE svg [ <!ENTITY ns "http://www.w3.org/2000/svg"> ]>"#;
    let text = format!(r#"{doctype}{ROOT} xmlns="&ns;" xmlns:x="&ns;"><desc><![CDATA[&r; &#0;]]></desc>{RECT}</svg>"#);
    assert_eq!(x_range(&text), (0.0, 20.0));
    // In a CDATA style sheet a reference is CSS text, and `&#58;` is not a colon there.
    let text = page(&format!("<style><![CDATA[.s{{transform&#58;scale(2)}}]]></style>{RECT}"));
    assert_eq!(x_range(&text), (0.0, 20.0));
}

#[test]
fn svg_many_reference_starts_read_in_linear_time() {
    // Each scan stops at the end of its run, and the next search starts there. 1 MiB of
    // each must read in well under five seconds.
    for filler in ["&a".repeat(1 << 19), format!("&{}", "a".repeat(1 << 20)), "&#0000".repeat(1 << 17)] {
        for text in [
            format!(r#"{ROOT} a="{filler}">{RECT}</svg>"#),
            page(&format!("<title>{filler}</title>{RECT}")),
        ] {
            let start = Instant::now();
            assert_eq!(x_range(&text), (0.0, 20.0));
            let spent = start.elapsed();
            assert!(spent < Duration::from_secs(5), "{spent:?}");
        }
    }
}
