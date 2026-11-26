//! #1880: SVG drawings read as 2D sketch geometry - shapes, paths, curves, group transforms,
//! units and the viewBox - and chained by the sketch chaining of #1861.
//!
//! Provenance: hand-written SVG fixtures from the W3C SVG 1.1 element and path grammar;
//! the expected sizes are worked by hand. Gate map (M4.4): covers camcraft_geometry::svg and
//! camcraft_geometry::sketch_data (from_dxf over an SVG document).
//!
//! This is the directory suite of the SVG reader: each file in `tests/svg/` is a module of
//! this one test binary (AGENTS.md "New tests join a directory suite").

#[path = "svg/references.rs"]
mod references;
#[path = "svg/refusals.rs"]
mod refusals;
#[path = "svg/style_safety.rs"]
mod style_safety;
#[path = "svg/tree_rules.rs"]
mod tree_rules;
#[path = "svg/url_tokens.rs"]
mod url_tokens;
#[path = "svg/visibility.rs"]
mod visibility;

use camcraft_geometry::dxf::{Primitive, Unit, Units};
use camcraft_geometry::sketch_chains::LoopRole;
use camcraft_geometry::sketch_data::SketchData;
use camcraft_geometry::svg;

const TOL: f64 = 1e-6;

fn sketch(text: &str, unit: Option<Unit>) -> SketchData {
    let doc = svg::parse(text.as_bytes(), unit).expect("the SVG reads");
    SketchData::from_dxf(doc, unit, None).expect("the SVG chains")
}

fn size(data: &SketchData) -> (f64, f64, f64, f64) {
    let (lo, hi) = data.bounds().expect("bounds");
    (lo.x, lo.y, hi.x - lo.x, hi.y - lo.y)
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn svg_rectangle_with_a_hole_in_mm_nests_and_flips_y() {
    let text = r#"<?xml version="1.0"?>
<!-- a plate with a hole -->
<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm" viewBox="0 0 100 50">
  <rect x="10" y="5" width="80" height="30"/>
  <circle cx="30" cy="15" r="5"/>
</svg>"#;
    let data = sketch(text, None);
    assert_eq!(data.units().stated, Units::Known(Unit::Millimeters));
    let chains = data.chains();
    assert_eq!((chains.closed.len(), chains.open.len(), chains.rejected.len()), (2, 0, 0));
    let roles: Vec<LoopRole> = chains.closed.iter().map(|c| c.nesting.clone().expect("nesting").role).collect();
    assert!(roles.contains(&LoopRole::Outer) && roles.contains(&LoopRole::Hole), "{roles:?}");
    // Y points down in SVG: the rectangle's top edge (y = 5) lies 45 mm up from the page bottom.
    let (x, y, w, h) = size(&data);
    assert!(near(x, 10.0, TOL) && near(y, 15.0, TOL) && near(w, 80.0, TOL) && near(h, 30.0, TOL), "{x} {y} {w} {h}");
    let circle = data.primitives().iter().find_map(|p| match p.primitive {
        Primitive::Circle { center, radius } => Some((center, radius)),
        _ => None,
    });
    let (center, radius) = circle.expect("a circle stays a circle");
    assert!(near(center.x, 30.0, TOL) && near(center.y, 35.0, TOL) && near(radius, 5.0, TOL));
}

#[test]
fn svg_inch_and_point_documents_scale_to_mm() {
    let inch = r#"<svg width="2in" height="1in" viewBox="0 0 200 100"><rect width="200" height="100"/></svg>"#;
    let (_, _, w, h) = size(&sketch(inch, None));
    assert!(near(w, 50.8, TOL) && near(h, 25.4, TOL), "{w} {h}");
    // No viewBox: one user unit is one CSS px, 96 to the inch.
    let px = r#"<svg width="96px" height="96px"><rect width="96" height="48"/></svg>"#;
    let (_, _, w, h) = size(&sketch(px, None));
    assert!(near(w, 25.4, TOL) && near(h, 12.7, TOL), "{w} {h}");
    let pt = r#"<svg width="72pt" height="36pt" viewBox="0 0 72 36"><line x1="0" y1="0" x2="72" y2="0"/></svg>"#;
    let (_, _, w, _) = size(&sketch(pt, None));
    assert!(near(w, 25.4, TOL), "{w}");
    let cm = r#"<svg width="10cm" height="5cm" viewBox="0 0 10 5"><polygon points="0,0 10,0 10,5"/></svg>"#;
    let (_, _, w, h) = size(&sketch(cm, None));
    assert!(near(w, 100.0, TOL) && near(h, 50.0, TOL), "{w} {h}");
}

#[test]
fn svg_without_a_physical_size_is_never_guessed() {
    let text = r#"<svg viewBox="0 0 40 20"><rect width="40" height="20"/></svg>"#;
    let doc = svg::parse(text.as_bytes(), None).expect("reads");
    assert_eq!(doc.units, Units::MissingOrUnitless);
    let refusal = SketchData::from_dxf(doc, None, None).expect_err("no size, no unit: refused");
    assert!(refusal.needs_units(), "{refusal}");
    // The caller's unit is one drawing unit.
    let (_, _, w, h) = size(&sketch(text, Some(Unit::Inches)));
    assert!(near(w, 1016.0, TOL) && near(h, 508.0, TOL), "{w} {h}");
    // A unitless width is not a stated size either.
    let bare = r#"<svg width="40" height="20"><rect width="40" height="20"/></svg>"#;
    assert_eq!(svg::parse(bare.as_bytes(), None).expect("reads").units, Units::MissingOrUnitless);
    // A file that states its size refuses a caller's unit by name.
    let sized = r#"<svg width="40mm" height="20mm"><rect width="40" height="20"/></svg>"#;
    let why = svg::parse(sized.as_bytes(), Some(Unit::Inches)).expect_err("two sizes").to_string();
    assert!(why.contains("states its size"), "{why}");
}

#[test]
fn svg_path_commands_absolute_and_relative_give_the_same_closed_loop() {
    let page = |d: &str| format!(r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100"><path d="{d}"/></svg>"#);
    for d in [
        "M10 10 L60 10 L60 40 L10 40 Z",
        "m10,10 l50,0 0,30 -50,0 z",
        "M10 10H60V40H10Z",
        "m10 10h50v30h-50z",
    ] {
        let data = sketch(&page(d), None);
        assert_eq!(data.chains().closed.len(), 1, "{d}");
        let (x, y, w, h) = size(&data);
        assert!(near(x, 10.0, TOL) && near(y, 60.0, TOL) && near(w, 50.0, TOL) && near(h, 30.0, TOL), "{d}: {x} {y} {w} {h}");
    }
}

#[test]
fn svg_beziers_flatten_within_the_chord_tolerance_and_arcs_stay_arcs() {
    // A circle of radius 20 drawn as two elliptical arcs stays two analytic arcs.
    let arcs = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100">
      <path d="M30 50 A20 20 0 0 1 70 50 A20 20 0 0 1 30 50 Z"/></svg>"#;
    let data = sketch(arcs, None);
    assert_eq!(data.chains().closed.len(), 1);
    assert!(data.primitives().iter().all(|p| matches!(p.primitive, Primitive::Arc { .. })), "{:?}", data.primitives());
    let (_, _, w, h) = size(&data);
    assert!(near(w, 40.0, TOL) && near(h, 40.0, TOL), "{w} {h}");

    // Cubic and quadratic Beziers (with the S and T shorthands) flatten to lines whose
    // vertices lie on the curve, and the outline closes.
    let curves = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100">
      <path d="M10 50 C10 20 40 20 40 50 S70 80 70 50 Q85 20 90 50 T90 90 L10 90 Z"/></svg>"#;
    let data = sketch(curves, None);
    let chains = data.chains();
    assert_eq!((chains.closed.len(), chains.rejected.len()), (1, 0), "{:?}", chains.rejected);
    assert!(data.primitives().len() > 20, "the curves are flattened: {}", data.primitives().len());
    // The first cubic peaks at y = 27.5 in SVG space (t = 0.5): 72.5 after the flip.
    let top = data.primitives().iter().filter_map(|p| match p.primitive {
        Primitive::Line { start, .. } if start.x <= 40.0 => Some(start.y),
        _ => None,
    });
    let peak = top.fold(f64::MIN, f64::max);
    assert!(near(peak, 72.5, svg::CHORD_TOLERANCE_MM), "{peak}");
}

#[test]
fn svg_group_transforms_compose_and_an_ellipse_under_shear_is_flattened() {
    let text = r#"<svg width="200mm" height="200mm" viewBox="0 0 200 200">
      <g transform="translate(100,100)">
        <g transform="rotate(90) scale(2)">
          <rect x="0" y="0" width="10" height="5"/>
        </g>
        <circle cx="0" cy="0" r="4" transform="matrix(2 0 0 2 -50 0)"/>
        <ellipse cx="0" cy="0" rx="10" ry="5" transform="translate(-60 -60) skewX(30)"/>
      </g></svg>"#;
    let data = sketch(text, None);
    let chains = data.chains();
    assert_eq!((chains.closed.len(), chains.rejected.len()), (3, 0), "{:?}", chains.rejected);
    // rotate(90) scale(2): the 10 x 5 rectangle becomes 10 wide and 20 tall, left of x = 100.
    let lines: Vec<_> = data.primitives().iter().filter_map(|p| match p.primitive {
        Primitive::Line { start, end } => Some((start, end)),
        _ => None,
    }).filter(|(s, _)| s.x > 85.0).collect();
    let xs = lines.iter().flat_map(|(s, e)| [s.x, e.x]);
    let ys = lines.iter().flat_map(|(s, e)| [s.y, e.y]);
    let (x0, x1) = xs.fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
    let (y0, y1) = ys.fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
    assert!(near(x0, 90.0, TOL) && near(x1, 100.0, TOL) && near(y0, 80.0, TOL) && near(y1, 100.0, TOL), "{x0} {x1} {y0} {y1}");
    // A uniform scale keeps the circle a circle: radius 8 at (50, 100).
    let circle = data.primitives().iter().find_map(|p| match p.primitive {
        Primitive::Circle { center, radius } => Some((center, radius)),
        _ => None,
    });
    let (center, radius) = circle.expect("circle");
    assert!(near(center.x, 50.0, TOL) && near(center.y, 100.0, TOL) && near(radius, 8.0, TOL));
}

#[test]
fn svg_rounded_rect_polyline_and_hidden_or_unsupported_content() {
    let text = r##"<svg width="100mm" height="100mm" viewBox="0 0 100 100" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape">
      <defs><rect id="tab" width="5" height="5"/></defs>
      <g id="layer1" inkscape:label="Cut">
        <rect x="10" y="10" width="40" height="20" rx="5"/>
        <polyline points="60,10 70,20 80,10" fill="none" stroke="black"/>
        <use href="#tab" x="60" y="60"/>
      </g>
      <text x="0" y="0">label</text>
      <image href="photo.png" width="10" height="10"/>
      <use xlink:href="other.svg#part"/>
      <rect x="0" y="0" width="5" height="5" style="display:none"/>
    </svg>"##;
    let data = sketch(text, None);
    let chains = data.chains();
    assert_eq!((chains.closed.len(), chains.open.len(), chains.rejected.len()), (2, 1, 0), "{:?}", chains.rejected);
    // A top-level group is a layer, named by its Inkscape label.
    assert_eq!(data.layers(), ["Cut".to_owned()]);
    let arcs = data.primitives().iter().filter(|p| matches!(p.primitive, Primitive::Arc { .. })).count();
    assert_eq!(arcs, 4, "rx = ry corners are circular arcs");
    let skipped = &data.summary().skipped;
    for kind in ["text", "image", "use", "hidden"] {
        assert_eq!(skipped.get(kind), Some(&1), "{kind}: {skipped:?}");
    }
}

#[test]
fn svg_root_display_none_skips_all_descendants_and_reports_hidden_content() {
    for attr in [r#"display="none""#, r#"style="display: none""#] {
        let text = format!(
            r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100" {attr}><rect width="20" height="20"/></svg>"#
        );
        let doc = svg::parse(text.as_bytes(), None).expect("hidden root reads");
        assert!(doc.primitives.is_empty(), "{attr}: hidden root must emit no geometry");
        assert_eq!(doc.summary.skipped.get("hidden"), Some(&1), "{attr}: {:?}", doc.summary.skipped);
    }
}

#[test]
fn svg_root_clipping_and_masking_are_reported_as_unsupported() {
    for attr in ["clip-path", "mask"] {
        let text = format!(
            r##"<svg width="100mm" height="100mm" viewBox="0 0 100 100" {attr}="url(#c)"><defs><clipPath id="c"><rect width="5" height="5"/></clipPath></defs><rect width="20" height="20"/></svg>"##
        );
        let doc = svg::parse(text.as_bytes(), None).expect("unsupported root attribute reads");
        assert_eq!(doc.primitives.len(), 4);
        assert_eq!(doc.summary.skipped.get(attr), Some(&1), "{attr}: {:?}", doc.summary.skipped);
    }
}

fn assert_inline_effects_are_reported(element: &str) {
    for property in ["clip-path", "mask"] {
        let escaped = if property == "clip-path" { r"clip-\70 ath" } else { r"m\61 sk" };
        for style in [
            format!("{property}:url(#c)"),
            format!("/* effect */{property}:/* ref */url(#c)"),
            format!("{escaped}:url(#c)"),
            format!(
                "fill: black; {} : url(#c) !important; stroke: none",
                property.to_ascii_uppercase()
            ),
        ] {
            let effect = format!(r#"style="{style}""#);
            let rect = r#"<rect width="20" height="20"/>"#;
            let (root_effect, content) = match element {
                "svg" => (effect, rect.to_owned()),
                "g" => (String::new(), format!("<g {effect}>{rect}</g>")),
                "rect" => (
                    String::new(),
                    format!(r#"<rect width="20" height="20" {effect}/>"#),
                ),
                _ => unreachable!(),
            };
            let text = format!(
                r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100" {root_effect}>{content}</svg>"#
            );
            let doc = svg::parse(text.as_bytes(), None).expect("unsupported inline effect reads");
            assert_eq!(doc.primitives.len(), 4, "{element}: {style}");
            assert_eq!(
                doc.summary.skipped.get(property),
                Some(&1),
                "{element}: {style}: {:?}",
                doc.summary.skipped
            );
            assert_eq!(
                doc.summary.skipped.len(),
                1,
                "{element}: {style}: {:?}",
                doc.summary.skipped
            );
        }
    }
}

#[test]
fn svg_root_inline_clipping_and_masking_are_reported_as_unsupported() {
    assert_inline_effects_are_reported("svg");
}

#[test]
fn svg_group_inline_clipping_and_masking_are_reported_as_unsupported() {
    assert_inline_effects_are_reported("g");
}

#[test]
fn svg_shape_inline_clipping_and_masking_are_reported_as_unsupported() {
    assert_inline_effects_are_reported("rect");
}

#[test]
fn svg_stylesheets_inside_defs_and_tree_are_counted_as_skipped() {
    let in_defs = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100">
      <defs><style>.h{stroke:none}</style></defs>
      <rect class="h" width="20" height="20"/>
    </svg>"#;
    // A sheet of paint rules is counted. A rule that can hide or move a shape is refused:
    // `svg/tree_rules.rs` pins that.
    let doc = svg::parse(in_defs.as_bytes(), None).expect("stylesheet in defs reads");
    assert_eq!(doc.summary.skipped.get("style"), Some(&1), "{:?}", doc.summary.skipped);
    assert_eq!(doc.summary.skipped.len(), 1, "{:?}", doc.summary.skipped);

    let nested_and_used = r##"<svg width="100mm" height="100mm" viewBox="0 0 100 100">
      <style>.a{fill:none}</style>
      <defs>
        <g><style>.c{opacity:0}</style></g>
        <g id="part"><style>.b{stroke:black}</style><rect width="10" height="10"/></g>
      </defs>
      <use href="#part"/>
      <use href="#part" x="20"/>
    </svg>"##;
    let doc = svg::parse(nested_and_used.as_bytes(), None).expect("nested stylesheets read");
    assert_eq!(doc.primitives.len(), 8);
    assert_eq!(doc.summary.skipped.get("style"), Some(&3), "{:?}", doc.summary.skipped);
    assert_eq!(doc.summary.skipped.len(), 1, "{:?}", doc.summary.skipped);
}

#[test]
fn svg_bunched_curve_vertices_merge_so_no_piece_is_shorter_than_the_join_tolerance() {
    // A cubic with both control points on its start runs as x = 10 t^3: evenly spaced t gives
    // pieces far shorter than the 0.01 mm join tolerance near the start. They must merge.
    let text = r#"<svg width="20mm" height="20mm" viewBox="0 0 20 20"><path d="M0 0 C0 0 0 0 10 0 L10 10 L0 10 Z"/></svg>"#;
    let data = sketch(text, None);
    let chains = data.chains();
    assert_eq!((chains.closed.len(), chains.rejected.len()), (1, 0), "{:?}", chains.rejected);
}

#[test]
fn svg_flags_and_numbers_in_compact_path_syntax() {
    // Compact arc flags ("0 01") and numbers that run together ("10-5", ".5.5").
    let text = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100">
      <path d="M10,50a20,20 0 0110,0l10-5l.5.5L30.5,70L10,70z"/></svg>"#;
    let data = sketch(text, None);
    assert_eq!(data.chains().closed.len(), 1, "{:?}", data.chains().rejected);
}

#[test]
fn svg_cusp_beziers_with_a_retracted_handle_import_within_the_chord_bound() {
    // A control point equal to its end point (a cusp node in Inkscape and Illustrator files)
    // makes the end piece shorter than the join tolerance. The short piece merges, and the
    // merged outline stays within the chord bound of the true curve.
    let cusps = [
        [10.0, 0.0, 10.0, 10.0, 10.0, 10.0],
        [0.0, 0.0, 10.0, 0.0, 10.0, 10.0],
        [100.0, 0.0, 100.0, 100.0, 100.0, 100.0],
    ];
    for c in cusps {
        let d = format!("M0 0 C{} {} {} {} {} {}", c[0], c[1], c[2], c[3], c[4], c[5]);
        let text = format!(r#"<svg width="120mm" height="120mm" viewBox="-10 -10 120 120"><path d="{d}"/></svg>"#);
        let doc = svg::parse(text.as_bytes(), None).unwrap_or_else(|e| panic!("{d}: {e}"));
        let lines: Vec<_> = doc
            .primitives
            .iter()
            .filter_map(|p| match p.primitive {
                Primitive::Line { start, end } => Some((start, end)),
                _ => None,
            })
            .collect();
        assert!(!lines.is_empty(), "{d}");
        let gap = |x: f64, y: f64| {
            lines
                .iter()
                .map(|&(a, b)| {
                    let (dx, dy) = (b.x - a.x, b.y - a.y);
                    let t = (((x - a.x) * dx + (y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                    (x - a.x - t * dx).hypot(y - a.y - t * dy)
                })
                .fold(f64::MAX, f64::min)
        };
        let worst = (0..=4000)
            .map(|i| {
                let t = i as f64 / 4000.0;
                let u = 1.0 - t;
                let (w1, w2, w3) = (3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                let x = w1 * c[0] + w2 * c[2] + w3 * c[4];
                let y = w1 * c[1] + w2 * c[3] + w3 * c[5];
                // The viewBox starts at (-10, -10) and is 120 high: x moves by 10, y flips to 110 - y.
                gap(x + 10.0, 110.0 - y)
            })
            .fold(0.0, f64::max);
        assert!(worst <= svg::CHORD_TOLERANCE_MM, "{d}: {worst} mm from the curve");
    }
}
