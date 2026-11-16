//! Provenance: hand-built examples from Autodesk's public ASCII DXF group conventions.
//! Gate map (M4.4): covers camcraft_geometry::dxf.

use camcraft_geometry::dxf::{Limits, Primitive, Unit, Units, parse, parse_with_limits};
use camcraft_geometry::geo2d::Point2;
use std::f64::consts::{FRAC_PI_2, PI};

fn drawing(header: &str, tables: &str, entities: &str) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n{header}0\nENDSEC\n0\nSECTION\n2\nTABLES\n{tables}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n"
    )
}

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

#[test]
fn dxf_rectangle_keeps_layers_and_explicit_units() {
    let text = drawing(
        "9\n$INSUNITS\n70\n4\n",
        "0\nTABLE\n2\nLAYER\n70\n1\n0\nLAYER\n2\ncut\n70\n0\n62\n7\n6\nCONTINUOUS\n0\nENDTAB\n",
        "0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n10\n20\n0\n10\n10\n20\n5\n10\n0\n20\n5\n",
    );
    let doc = parse(text.as_bytes()).unwrap();
    assert_eq!(doc.units, Units::Known(Unit::Millimeters));
    assert!(doc.layers.iter().any(|layer| layer.name == "cut"));
    assert_eq!(doc.primitives.len(), 4);
    let points = [
        Point2::new(0.0, 0.0),
        Point2::new(10.0, 0.0),
        Point2::new(10.0, 5.0),
        Point2::new(0.0, 5.0),
    ];
    for (i, p) in doc.primitives.iter().enumerate() {
        assert_eq!(p.layer, "cut");
        assert_eq!(
            p.primitive,
            Primitive::Line {
                start: points[i],
                end: points[(i + 1) % 4]
            }
        );
    }
    assert_eq!(doc.summary.vertices, 4);
}

#[test]
fn dxf_inches_are_metadata_without_silent_scaling() {
    let text = drawing(
        "9\n$INSUNITS\n70\n1\n",
        "",
        "0\nLINE\n10\n1\n20\n2\n11\n3\n21\n4\n",
    );
    let doc = parse(text.as_bytes()).unwrap();
    assert_eq!(doc.units, Units::Known(Unit::Inches));
    near(Unit::Inches.millimeters_per_unit(), 25.4);
    assert_eq!(doc.primitives[0].layer, "0");
    assert_eq!(
        doc.primitives[0].primitive,
        Primitive::Line {
            start: Point2::new(1.0, 2.0),
            end: Point2::new(3.0, 4.0)
        }
    );
}

#[test]
fn dxf_missing_unitless_unknown_and_standard_scales_are_explicit() {
    assert_eq!(
        parse(drawing("9\n$INSUNITS\n70\n-1\n", "", "").as_bytes())
            .unwrap()
            .units,
        Units::Unsupported(-1)
    );
    for header in ["", "9\n$INSUNITS\n70\n0\n"] {
        assert_eq!(
            parse(drawing(header, "", "").as_bytes()).unwrap().units,
            Units::MissingOrUnitless
        );
    }
    assert_eq!(
        parse(drawing("9\n$INSUNITS\n70\n999\n", "", "").as_bytes())
            .unwrap()
            .units,
        Units::Unsupported(999)
    );
    for (code, scale) in [
        (2, 304.8),
        (3, 1_609_344.0),
        (5, 10.0),
        (6, 1000.0),
        (7, 1_000_000.0),
        (8, 0.0000254),
        (9, 0.0254),
        (10, 914.4),
        (11, 1e-7),
        (12, 1e-6),
        (13, 0.001),
        (14, 100.0),
        (15, 10_000.0),
        (16, 100_000.0),
        (17, 1e12),
        (18, 149_597_870_700_000.0),
        (19, 9.4607304725808e18),
        (20, 3.085677581491367e19),
        (21, 304.8006096012192),
        (22, 25.4000508001016),
        (23, 914.4018288036576),
        (24, 1_609_347.2186944373),
    ] {
        let header = format!("9\n$INSUNITS\n70\n{code}\n");
        let Units::Known(unit) = parse(drawing(&header, "", "").as_bytes()).unwrap().units else {
            panic!("code {code}")
        };
        assert!((unit.millimeters_per_unit() / scale - 1.0).abs() < 1e-14);
    }
}

#[test]
fn dxf_unrelated_header_variables_and_tables_leave_units_and_layers_intact() {
    let text = drawing(
        "9\n$ACADVER\n1\nAC1027\n9\n$INSUNITS\n70\n4\n9\n$LUNITS\n70\n2\n",
        "0\nTABLE\n2\nSTYLE\n0\nSTYLE\n2\nStandard\n0\nENDTAB\n",
        "",
    );
    let doc = parse(text.as_bytes()).unwrap();
    assert_eq!(doc.units, Units::Known(Unit::Millimeters));
    assert!(doc.layers.is_empty());
}

#[test]
fn dxf_layer_name_length_is_bounded_before_segment_copies() {
    let name = "a".repeat(256);
    for (tables, entities) in [
        (
            String::new(),
            format!("0\nLWPOLYLINE\n8\n{name}\n90\n2\n10\n0\n20\n0\n10\n1\n20\n1\n"),
        ),
        (
            format!("0\nTABLE\n2\nLAYER\n0\nLAYER\n2\n{name}\n0\nENDTAB\n"),
            String::new(),
        ),
    ] {
        let error = parse(drawing("", &tables, &entities).as_bytes())
            .unwrap_err()
            .to_string();
        assert!(error.to_lowercase().contains("layer"), "{error}");
    }
    let name = "a".repeat(255);
    let entities = format!("0\nLINE\n8\n{name}\n10\n0\n20\n0\n11\n1\n21\n1\n");
    assert_eq!(
        parse(drawing("", "", &entities).as_bytes())
            .unwrap()
            .primitives[0]
            .layer,
        name
    );
}

#[test]
fn dxf_malformed_fields_do_not_fall_back_to_valid_geometry() {
    for (header, tables, entities, name) in [
        (
            "",
            "",
            "0\nLINE\n10\n0\n20\n0\n11\n1\n21\n1\n1072\nignored\n",
            "group",
        ),
        ("9\n$INSUNITS\n70\n4\n9\n$INSUNITS\n", "", "", "INSUNITS"),
        ("9\n$INSUNITS\n1\n4\n", "", "", "INSUNITS"),
        ("70\n4\n", "", "", "HEADER"),
        ("", "0\nTABLE\n2\nLAYER\n0\nLAYER\n0\nENDTAB\n", "", "LAYER"),
        ("", "", "0\nLINE\n10\nwrong\n20\n0\n11\n1\n21\n1\n", "LINE"),
        (
            "",
            "",
            "0\nLWPOLYLINE\n90\n2\n20\n0\n10\n0\n10\n1\n20\n1\n",
            "LWPOLYLINE",
        ),
        (
            "",
            "",
            "0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n42\n1e20\n10\n1e-8\n20\n0\n",
            "sweep",
        ),
    ] {
        let error = parse(drawing(header, tables, entities).as_bytes())
            .unwrap_err()
            .to_string();
        assert!(error.contains(name), "{error}");
    }
    let error = parse(b"0\nSECTION\n2\nBLOCKS\n0\nENDBLK\n0\nEOF\n")
        .unwrap_err()
        .to_string();
    assert!(error.contains("ENDSEC"), "{error}");
}

#[test]
fn dxf_declared_vertex_limit_is_checked_before_reading_vertices() {
    let text = drawing("", "", "0\nLWPOLYLINE\n90\n2147483647\n");
    let error = parse_with_limits(
        text.as_bytes(),
        Limits {
            max_vertices: 1,
            ..Limits::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("vertex count exceeds limit"), "{error}");
}

#[test]
fn dxf_circle_and_wrapping_arc_stay_exact() {
    let text = drawing(
        "",
        "",
        "0\nCIRCLE\n8\nholes\n10\n2\n20\n3\n40\n4\n0\nARC\n10\n5\n20\n6\n40\n2\n50\n270\n51\n90\n",
    );
    let doc = parse(text.as_bytes()).unwrap();
    assert_eq!(doc.primitives.len(), 2);
    assert_eq!(doc.primitives[0].layer, "holes");
    assert_eq!(
        doc.primitives[0].primitive,
        Primitive::Circle {
            center: Point2::new(2.0, 3.0),
            radius: 4.0
        }
    );
    let Primitive::Arc {
        center,
        radius,
        start_angle,
        sweep_angle,
    } = doc.primitives[1].primitive
    else {
        panic!("exact arc")
    };
    assert_eq!(center, Point2::new(5.0, 6.0));
    near(radius, 2.0);
    near(start_angle.rem_euclid(2.0 * PI), 3.0 * FRAC_PI_2);
    near(sweep_angle, PI);
}

#[test]
fn dxf_bulge_sign_and_major_arcs_keep_the_exact_sweep() {
    for bulge in [1.0f64, -1.0, 2.0, -2.0, 0.25, 1e-4, -1e-4] {
        let entity = format!("0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n42\n{bulge}\n10\n2\n20\n0\n");
        let doc = parse(drawing("", "", &entity).as_bytes()).unwrap();
        assert_eq!(doc.primitives.len(), 1);
        let Primitive::Arc {
            center,
            radius,
            start_angle,
            sweep_angle,
        } = doc.primitives[0].primitive
        else {
            panic!("exact bulge")
        };
        near(center.x, 1.0);
        near(center.y, (1.0 - bulge * bulge) / (2.0 * bulge));
        near(radius, (1.0 + bulge * bulge) / (2.0 * bulge.abs()));
        near(sweep_angle, 4.0 * bulge.atan());
        near(center.x + radius * start_angle.cos(), 0.0);
        near(center.y + radius * start_angle.sin(), 0.0);
        near(center.x + radius * (start_angle + sweep_angle).cos(), 2.0);
        near(center.y + radius * (start_angle + sweep_angle).sin(), 0.0);
    }
}

#[test]
fn dxf_bulge_arcs_that_lose_source_endpoints_are_refused() {
    // The smaller bulge collapses both endpoints. The larger one keeps them
    // distinct but displaces them from the source vertices in both directions.
    let mut accepted = Vec::new();
    for bulge in ["1e-17", "-1e-17", "1e-12", "-1e-12"] {
        for (kind, entity) in [
            (
                "LWPOLYLINE",
                format!("0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n42\n{bulge}\n10\n0.00001\n20\n0\n"),
            ),
            (
                "POLYLINE",
                format!(
                    "0\nPOLYLINE\n0\nVERTEX\n10\n0\n20\n0\n42\n{bulge}\n0\nVERTEX\n10\n0.00001\n20\n0\n0\nSEQEND\n"
                ),
            ),
        ] {
            match parse(drawing("", "", &entity).as_bytes()) {
                Err(error) => {
                    let error = error.to_string();
                    assert!(error.contains(kind), "{kind}, bulge {bulge}: {error}");
                    assert!(error.contains("endpoint"), "{kind}, bulge {bulge}: {error}");
                }
                Ok(_) => accepted.push(format!("{kind}, bulge {bulge}")),
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "arcs lost source endpoints: {accepted:?}"
    );
}

#[test]
fn dxf_legacy_polyline_uses_parent_layer_and_closing_bulge() {
    let text = drawing(
        "",
        "",
        "0\nPOLYLINE\n8\ncut\n66\n1\n70\n1\n10\n0\n20\n0\n30\n3\n0\nVERTEX\n10\n0\n20\n0\n30\n0\n0\nVERTEX\n10\n2\n20\n0\n30\n0\n42\n1\n0\nSEQEND\n",
    );
    let doc = parse(text.as_bytes()).unwrap();
    assert_eq!(doc.elevation, Some(3.0));
    assert_eq!(doc.primitives.len(), 2);
    assert!(doc.primitives.iter().all(|p| p.layer == "cut"));
    let Primitive::Arc {
        center,
        radius,
        sweep_angle,
        ..
    } = doc.primitives[1].primitive
    else {
        panic!("closing bulge")
    };
    assert_eq!(center, Point2::new(1.0, 0.0));
    near(radius, 1.0);
    near(sweep_angle, PI);
    assert_eq!(doc.summary.vertices, 2);
}

#[test]
fn dxf_common_elevation_is_retained_and_positive_extrusion_is_supported() {
    let text = drawing(
        "",
        "",
        "0\nLINE\n10\n0\n20\n0\n30\n7\n11\n1\n21\n2\n31\n7\n210\n0\n220\n0\n230\n1\n0\nLWPOLYLINE\n90\n2\n38\n7\n10\n0\n20\n0\n10\n1\n20\n1\n",
    );
    let doc = parse(text.as_bytes()).unwrap();
    assert_eq!(doc.elevation, Some(7.0));
    assert_eq!(doc.primitives.len(), 2);
}

#[test]
fn dxf_annotations_are_counted_by_kind() {
    let text = drawing(
        "",
        "",
        "0\nTEXT\n1\nlabel\n0\nMTEXT\n1\nnotes\n0\nTEXT\n1\nsecond\n0\nDIMENSION\n0\nHATCH\n",
    );
    let doc = parse(text.as_bytes()).unwrap();
    assert!(doc.primitives.is_empty());
    assert_eq!(doc.summary.skipped.get("TEXT"), Some(&2));
    for kind in ["MTEXT", "DIMENSION", "HATCH"] {
        assert_eq!(doc.summary.skipped.get(kind), Some(&1));
    }
    assert_eq!(doc.summary.entities, 5);
}

#[test]
fn dxf_crlf_and_group_comments_do_not_change_geometry() {
    let text = drawing(
        "",
        "",
        "999\ncomment\n0\nLINE\n10\n0\n20\n0\n11\n1\n21\n1\n",
    );
    assert_eq!(
        parse(text.as_bytes()).unwrap().primitives,
        parse(text.replace('\n', "\r\n").as_bytes())
            .unwrap()
            .primitives
    );
}

#[test]
fn dxf_deterministic_smoke_fuzz_never_panics() {
    let seed = drawing(
        "",
        "",
        "0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n42\n1\n10\n2\n20\n0\n",
    )
    .into_bytes();
    for cut in 0..seed.len() {
        let _ = parse(&seed[..cut]);
    }
    let mut state = 0x1860u64;
    for _ in 0..1000 {
        let mut bytes = seed.clone();
        for _ in 0..4 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let index = (state as usize) % bytes.len();
            bytes[index] = (state >> 32) as u8;
        }
        let _ = parse(&bytes);
    }
}

#[test]
fn dxf_zero_length_entities_do_not_create_primitives() {
    for entity in [
        "0\nLINE\n10\n0\n20\n0\n11\n0\n21\n0\n",
        "0\nARC\n10\n0\n20\n0\n40\n1\n50\n45\n51\n45\n",
        "0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n10\n0\n20\n0\n",
    ] {
        if let Ok(doc) = parse(drawing("", "", entity).as_bytes()) {
            assert!(doc.primitives.is_empty());
        }
    }
}
