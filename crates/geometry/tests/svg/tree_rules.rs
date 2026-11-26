//! #1880: what can hide, move or resize a drawn shape from outside its own attributes is
//! refused by name or counted: style sheets, animation elements, elements below a shape,
//! DOCTYPE entities, an unknown `preserveAspectRatio` and markers (Prime Directive).
//!
//! Provenance: hand-built SVG text; the SVG 1.1 styling, animation and coordinate-systems
//! chapters, CSS Syntax 3 and the XML 1.0 entity rules.
//! Gate map (M4.4): covers camcraft_geometry::svg.

use camcraft_geometry::dxf::{Limits, Primitive};
use camcraft_geometry::svg;
use std::time::{Duration, Instant};

const ROOT: &str = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100""#;

fn page(body: &str) -> String {
    format!("{ROOT}>{body}</svg>")
}

fn refuses(text: &str, name: &str) {
    let error = match svg::parse(text.as_bytes(), None) {
        Err(error) => error.to_string(),
        Ok(doc) => panic!("{text} must be refused; it imported {} primitives", doc.primitives.len()),
    };
    assert!(error.to_lowercase().contains(&name.to_lowercase()), "{error:?} does not name {name:?}");
}

/// The x extent of the imported lines: lowest and highest x.
fn x_range(text: &str) -> (f64, f64) {
    let doc = svg::parse(text.as_bytes(), None).unwrap_or_else(|e| panic!("{text}: {e}"));
    doc.primitives.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| match &p.primitive {
        Primitive::Line { start, end } => (lo.min(start.x).min(end.x), hi.max(start.x).max(end.x)),
        _ => (lo, hi),
    })
}

fn skipped(text: &str) -> std::collections::BTreeMap<String, usize> {
    let doc = svg::parse(text.as_bytes(), None).unwrap_or_else(|e| panic!("{text}: {e}"));
    doc.summary.skipped.iter().map(|(kind, count)| (kind.clone(), *count)).collect()
}

const RECT: &str = r#"<rect class="s" width="20" height="20"/>"#;

#[test]
fn svg_style_sheet_rules_that_hide_or_resize_a_shape_are_refused_by_name() {
    // The file of the review: a browser hides the first rectangle and draws the second at 40 x 40.
    let review = page(
        r#"<style>.h{display:none} .s{transform:scale(2)}</style>
           <rect class="h" width="20" height="20"/><rect class="s" x="50" width="20" height="20"/>"#,
    );
    refuses(&review, "style sheet display");
    for (sheet, name) in [
        (".s{display:none}", "style sheet display"),
        (".s{fill:red;DISPLAY : inline}", "style sheet display"),
        (".s{visibility:hidden}", "style sheet visibility"),
        (".s{transform:scale(2)}", "style sheet transform"),
        ("rect{fill:none;-webkit-transform:scale(2)}", "style sheet -webkit-transform"),
        ("rect{width:40px}", "style sheet width"),
        ("path{d:path('M0 0h9')}", "style sheet d "),
        (".s{vector-effect:non-scaling-size}", "style sheet vector-effect"),
        (r"/* c */ .s { tr\61 nsform : scale(2) }", "style sheet transform"),
        // An open block at the end of the file is a rule in a browser.
        (".a{fill:red} .s{transform:scale(2)", "style sheet transform"),
        // CSS Nesting: a rule inside a rule.
        (".a{fill:red; rect{transform:scale(2)}}", "a rule inside a rule"),
        ("@media screen{.s{transform:scale(2)}}", "rule @media"),
        ("@supports (display:grid){.s{display:none}}", "rule @supports"),
        ("@import url(other.css);", "rule @import"),
        ("@IMPORT 'other.css';", "rule @import"),
        // A `}` in an unquoted url ends no block in a browser, and none here: the next
        // declaration is read and refused (`url_tokens.rs` has the url cases).
        (".a{fill:url(a}b);transform:scale(2)}", "style sheet transform"),
        (".a{fill:red}} .s{transform:scale(2)}", "'}' outside a rule"),
        (".a{fill:red} transform:scale(2)", "text after its last rule"),
        (".a{fill:red} x; .s{fill:red}", "';' outside a rule"),
        ("a[x='{']{transform:scale(2)}", "style sheet transform"),
    ] {
        for text in [
            page(&format!("<style>{sheet}</style>{RECT}")),
            page(&format!("<style><![CDATA[{sheet}]]></style>{RECT}")),
            page(&format!("<defs><style type=\"text/css\">{sheet}</style></defs>{RECT}")),
            page(&format!("{RECT}<g display=\"none\"><g><style>{sheet}</style></g></g>")),
        ] {
            refuses(&text, name);
        }
    }
    // A sheet in character references is the same sheet.
    refuses(&page(&format!("<style>.s{{transform&#58;scale(2)}}</style>{RECT}")), "style sheet transform");
}

#[test]
fn svg_external_style_sheets_are_refused_by_name() {
    let rect = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100"><rect width="20" height="20"/></svg>"#;
    refuses(&format!("<?xml version=\"1.0\"?><?xml-stylesheet href=\"a.css\" type=\"text/css\"?>{rect}"), "xml-stylesheet");
    refuses(&format!("{rect}<?xml-stylesheet href=\"a.css\"?>"), "xml-stylesheet");
    let link = r#"<link xmlns="http://www.w3.org/1999/xhtml" rel="StyleSheet" href="a.css"/>"#;
    refuses(&page(&format!("{link}{RECT}")), "<link>");
    refuses(&page(&format!("<defs>{link}</defs>{RECT}")), "<link>");
    // Another processing instruction and the XML declaration read as before.
    let plain = format!("<?xml version=\"1.0\"?><?other data?>{rect}");
    assert_eq!(x_range(&plain), (0.0, 20.0));
}

#[test]
fn svg_paint_only_style_sheets_import_at_the_attribute_size() {
    for sheet in [
        ".cls-1{fill:none;stroke:#000;stroke-miterlimit:10}",
        ".cls-1,.cls-2{fill:none}\n.cls-2{stroke:#231f20;stroke-width:0.25px;}",
        "/* made by an editor */ rect { fill: url(#a); font-family: 'A;}{B', serif !important }",
        "<!-- .s{opacity:.5} -->",
        "@charset \"utf-8\"; @namespace svg url(http://www.w3.org/2000/svg); .s{fill:red}",
        "@font-face{font-family:F;src:url(data:font/woff2;base64,AAAA)} .s{font-family:F}",
        "",
        "  \n ",
    ] {
        for text in [
            page(&format!("<style>{sheet}</style>{RECT}")),
            page(&format!("<style><![CDATA[{sheet}]]></style>{RECT}")),
        ] {
            assert_eq!(x_range(&text), (0.0, 20.0), "{sheet}");
            let expected = [("style".to_owned(), 1)].into_iter().collect();
            assert_eq!(skipped(&text), expected, "{sheet}");
        }
    }
    // Clipping, masking and markers of a sheet are counted, as the inline ones are.
    let text = page(&format!(
        "<style>.a{{clip-path:url(#c)}} .b{{mask:url(#m);marker-end:url(#k)}} .c{{marker:none}}</style>{RECT}"
    ));
    let expected = [("clip-path", 1), ("marker", 1), ("mask", 1), ("style", 1)];
    assert_eq!(skipped(&text), expected.into_iter().map(|(k, n)| (k.to_owned(), n)).collect());
}

#[test]
fn svg_animation_elements_are_refused_by_name_at_each_place() {
    // The file of the review: a browser shows 40 x 20.
    refuses(&page(r#"<rect width="20" height="20"><set attributeName="width" to="40"/></rect>"#), "<set> animation");
    for (element, name) in [
        (r#"<set attributeName="width" to="40"/>"#, "<set> animation"),
        (r#"<animate attributeName="width" from="20" to="40" dur="1s" fill="freeze"/>"#, "<animate> animation"),
        (r#"<animateTransform attributeName="transform" type="scale" to="2" fill="freeze"/>"#, "<animateTransform>"),
        (r#"<animateMotion path="M0 0L50 0" fill="freeze"/>"#, "<animateMotion> animation"),
        (r#"<animateColor attributeName="fill" to="red"/>"#, "<animateColor> animation"),
        (r#"<discard begin="0s"/>"#, "<discard> animation"),
        (r##"<svg:set xmlns:svg="http://www.w3.org/2000/svg" href="#r" attributeName="width" to="40"/>"##, "<set>"),
    ] {
        let rect = |inner: &str| format!(r#"<rect id="r" width="20" height="20">{inner}</rect>"#);
        for text in [
            page(&rect(element)),
            page(&format!("<g>{element}{}</g>", rect(""))),
            page(&format!("{element}{}", rect(""))),
            page(&format!(r##"<defs>{}</defs><use href="#r">{element}</use>"##, rect(""))),
            page(&format!(r##"<defs>{}</defs><use href="#r"/>"##, rect(element))),
            // An animation names its target with `href`: its place does not limit it.
            page(&format!("<defs>{element}</defs>{}", rect(""))),
            page(&format!("<text>A{element}</text>{}", rect(""))),
            page(&format!("<g display=\"none\">{element}</g>{}", rect(""))),
            page(&format!("<metadata><x>{element}</x></metadata>{}", rect(""))),
        ] {
            refuses(&text, name);
        }
    }
}

#[test]
fn svg_elements_below_a_shape_or_a_use_are_counted_by_name() {
    // Descriptive children draw nothing and change nothing: no entry.
    let described = page(
        r##"<rect id="r" width="20" height="20"><title>T</title><desc>D</desc><metadata><x/></metadata></rect>
            <use href="#r" x="30"><title>T</title></use>"##,
    );
    assert_eq!(x_range(&described), (0.0, 50.0));
    assert!(skipped(&described).is_empty(), "{:?}", skipped(&described));
    // Each other child, known or not, has an entry that names it and its parent.
    let text = page(
        r##"<rect id="r" width="20" height="20"><future-element/><rect width="90" height="90"/></rect>
            <path d="M0 0h5"><text>A</text></path>
            <use href="#r" x="30"><future-element/></use>"##,
    );
    assert_eq!(x_range(&text), (0.0, 50.0), "a child of a shape is not drawn");
    // The rect is drawn two times (itself and the `use`), so its children are counted two times.
    let expected = [("future-element in rect", 2), ("rect in rect", 2), ("text in path", 1), ("future-element in use", 1)];
    assert_eq!(skipped(&text), expected.into_iter().map(|(k, n)| (k.to_owned(), n)).collect());
    // An element that this reader does not know is counted by its name at each walked place.
    let unknown = page(&format!("<future-element/><g><future-element><rect width=\"90\" height=\"90\"/></future-element></g>{RECT}"));
    assert_eq!(x_range(&unknown), (0.0, 20.0));
    assert_eq!(skipped(&unknown), [("future-element".to_owned(), 2)].into_iter().collect());
}

#[test]
fn svg_children_of_a_reused_shape_are_charged_against_the_work_budget() {
    // 50 000 children and 40 000 references are 2e9 steps: the work budget must refuse them.
    let children = "<title/>".repeat(50_000);
    let uses = r##"<use href="#r"/>"##.repeat(40_000);
    let text = page(&format!(r#"<defs><rect id="r" width="2" height="2">{children}</rect></defs>{uses}"#));
    let limits = Limits { max_bytes: 2 << 20, ..Limits::default() };
    let start = Instant::now();
    let error = svg::parse_with_limits(text.as_bytes(), None, limits).expect_err("the work budget").to_string();
    let spent = start.elapsed();
    assert!(error.contains("import budget") && error.contains("reference"), "{error}");
    assert!(spent < Duration::from_secs(5), "{spent:?}");
}

#[test]
fn svg_doctype_entity_references_are_refused_by_name() {
    let doctype = r#"<!DOCTYPE svg [ <!ENTITY r "<rect width='90' height='90'/>"> <!ENTITY d "none">
        <!ENTITY s "transform:scale(2)"> <!ENTITY ns "http://www.w3.org/2000/svg"> ]>"#;
    let file = |root: &str, body: &str| format!("{doctype}{ROOT}{root}>{body}</svg>");
    // In content a browser expands the entity into elements.
    refuses(&file("", &format!("&r;{RECT}")), "entity reference &r;");
    refuses(&file("", &format!("<g>{RECT}&r;</g>")), "entity reference &r;");
    refuses(&file("", &format!("<style>&s;</style>{RECT}")), "entity reference &s;");
    // In an attribute it expands into a value: this shape is hidden, or two times the size.
    refuses(&file("", r#"<rect width="20" height="20" display="&d;"/>"#), "entity reference &d; in attribute display");
    refuses(&file("", r#"<rect width="20" height="20" style="&s;"/>"#), "entity reference &s; in attribute style");
    refuses(&file(r#" style="fill:red;&s;""#, RECT), "entity reference &s;");
    // A namespace declaration is not read, and old editor files write it with an entity.
    let text = file(r#" xmlns="&ns;" xmlns:x="&ns;""#, RECT);
    assert_eq!(x_range(&text), (0.0, 20.0));
    // The predefined entities, character references and a plain `&` read as before.
    let text = file("", r#"<title>a &amp; b &lt; &#65; &#x41; & c</title><rect width="20" height="20" id="a&amp;b &#65; & c &;"/>"#);
    assert_eq!(x_range(&text), (0.0, 20.0));
    // In a CDATA section `&r;` is text, not a reference.
    let text = file("", &format!("<desc><![CDATA[&r;]]></desc>{RECT}"));
    assert_eq!(x_range(&text), (0.0, 20.0));
}

#[test]
fn svg_preserve_aspect_ratio_places_the_view_box_and_refuses_an_unknown_value() {
    // A 100 x 100 viewBox in a 200 x 100 mm page: 100 mm of free width.
    let file = |par: &str| {
        format!(r#"<svg width="200mm" height="100mm" viewBox="0 0 100 100" {par}><rect width="100" height="100"/></svg>"#)
    };
    let at = |par: &str| x_range(&file(&format!(r#"preserveAspectRatio="{par}""#)));
    assert_eq!(x_range(&file("")), (50.0, 150.0), "the default is xMidYMid meet");
    assert_eq!(at("xMidYMid"), (50.0, 150.0));
    assert_eq!(at("xMinYMax meet"), (0.0, 100.0));
    assert_eq!(at("  xMaxYMin  "), (100.0, 200.0));
    assert_eq!(at("defer xMaxYMid meet"), (100.0, 200.0));
    assert_eq!(at("none"), (0.0, 200.0));
    assert_eq!(at("xMinYMin slice"), (0.0, 200.0));
    // An unknown value once fell back to xMin/yMin without a note: 50 mm from the true place.
    for bad in ["xMidYMid bogus", "xmidymid", "xMid", "XMidYMid", "xMidYMiddle", "meet", "", "xMidYMid meet slice", "xMidyMid"] {
        refuses(&file(&format!(r#"preserveAspectRatio="{bad}""#)), "preserveAspectRatio");
    }
}

#[test]
fn svg_marker_shapes_are_counted_as_skipped() {
    let marker = r#"<defs><marker id="k"><path d="M0 0L9 9"/></marker></defs>"#;
    for line in [
        r#"<path d="M0 0h20" marker-end="url(#k)"/>"#,
        r#"<path d="M0 0h20" style="marker-start:url(#k)"/>"#,
        r#"<path d="M0 0h20" style="MARKER: url(#k) !important"/>"#,
        r#"<g marker-mid="url(#k)"><path d="M0 0h20"/></g>"#,
    ] {
        let text = page(&format!("{marker}{line}"));
        assert_eq!(x_range(&text), (0.0, 20.0), "{line}");
        assert_eq!(skipped(&text), [("marker".to_owned(), 1)].into_iter().collect(), "{line}");
    }
    let none = page(&format!(r#"{marker}<path d="M0 0h20" marker-end="none" style="marker-start: None"/>"#));
    assert!(skipped(&none).is_empty(), "{:?}", skipped(&none));
}
