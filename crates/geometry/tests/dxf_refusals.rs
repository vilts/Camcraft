//! Provenance: malformed hand-built ASCII DXF records and deterministic byte mutations.
//! Gate map (M4.4): covers camcraft_geometry::dxf, camcraft_geometry::sketch_data.

use camcraft_geometry::dxf::{Limits, parse, parse_with_limits};

#[path = "dxf/curve_bounds.rs"]
mod curve_bounds;
#[path = "dxf/curve_refusals.rs"]
mod curve_refusals;

fn drawing(entities: &str) -> String {
    format!("0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n")
}

fn refuses(text: &[u8], name: &str) {
    let error = parse(text)
        .expect_err("bad DXF must be refused")
        .to_string();
    assert!(
        error.to_lowercase().contains(&name.to_lowercase()),
        "{error:?} does not name {name}"
    );
}

#[test]
fn dxf_binary_and_malformed_streams_are_named() {
    refuses(b"AutoCAD Binary DXF\r\n\x1a\0", "binary");
    refuses(b"bad\nSECTION\n", "group");
    refuses(b"0\nSECTION\n2", "truncated");
    refuses(b"0\nSECTION\n2\nENTITIES\n", "ENDSEC");
    refuses(b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n", "EOF");
    refuses(b"0\nEOF\n0\nLINE\n", "EOF");
    refuses(b"0\nSECTION\n2\nHEADER\n", "ENDSEC");
    refuses(b"0\nSECTION\n2\nTABLES\n", "ENDSEC");
    refuses(b"0\nSECTION\n2\nTABLES\n0\nEOF\n", "ENDSEC");
    refuses(
        b"0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n0\nVERTEX\n10\n0\n20\n0\n0\nVERTEX\n10\n1\n20\n1\n",
        "SEQEND",
    );
}

#[test]
fn dxf_unsupported_entities_are_refused_by_name() {
    for kind in ["SPLINE", "ELLIPSE", "BLOCK", "3DFACE", "UNKNOWN"] {
        refuses(drawing(&format!("0\n{kind}\n")).as_bytes(), kind);
    }
    // #1865: a block reference is skipped and counted; the rest of the drawing reads.
    let insert = parse(drawing("0\nINSERT\n").as_bytes()).expect("INSERT is skipped");
    assert_eq!(insert.summary.skipped.get("INSERT"), Some(&1));
    refuses(
        b"0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nb\n0\nENDBLK\n0\nENDSEC\n0\nEOF\n",
        "BLOCK",
    );
}

#[test]
fn dxf_missing_fields_and_invalid_units_are_named() {
    refuses(drawing("0\nLINE\n10\n0\n20\n0\n11\n1\n").as_bytes(), "LINE");
    refuses(drawing("0\nCIRCLE\n10\n0\n20\n0\n").as_bytes(), "CIRCLE");
    refuses(
        b"0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\nwrong\n0\nENDSEC\n0\nEOF\n",
        "INSUNITS",
    );
    refuses(
        b"0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n0\nENDSEC\n0\nEOF\n",
        "INSUNITS",
    );
}

#[test]
fn dxf_nonfinite_and_out_of_range_geometry_is_refused() {
    for bad in [
        "NaN",
        "inf",
        "-inf",
        "1e309",
        "1000000000000",
        "-1000000000000",
    ] {
        refuses(
            drawing(&format!("0\nLINE\n10\n{bad}\n20\n0\n11\n1\n21\n1\n")).as_bytes(),
            "LINE",
        );
    }
    for bad in ["NaN", "inf", "-1", "0", "1e12"] {
        refuses(
            drawing(&format!("0\nCIRCLE\n10\n0\n20\n0\n40\n{bad}\n")).as_bytes(),
            "CIRCLE",
        );
    }
    refuses(
        drawing("0\nCIRCLE\n10\n999999999999\n20\n0\n40\n2\n").as_bytes(),
        "CIRCLE",
    );
    refuses(
        drawing("0\nARC\n10\n0\n20\n0\n40\n1\n50\nNaN\n51\n90\n").as_bytes(),
        "ARC",
    );
    refuses(
        drawing("0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n42\nNaN\n10\n1\n20\n1\n").as_bytes(),
        "LWPOLYLINE",
    );
    refuses(
        drawing("0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n42\n1e-300\n10\n1\n20\n1\n").as_bytes(),
        "LWPOLYLINE",
    );
}

#[test]
fn dxf_tilted_negative_and_zero_extrusions_are_refused() {
    for extrusion in [
        "210\n0.1\n220\n0\n230\n1\n",
        "210\n0\n220\n0.1\n230\n1\n",
        "230\n-1\n",
        "230\n0\n",
        "230\nNaN\n",
    ] {
        refuses(
            drawing(&format!("0\nCIRCLE\n10\n0\n20\n0\n40\n1\n{extrusion}")).as_bytes(),
            "extrusion",
        );
    }
}

#[test]
fn dxf_duplicate_units_and_geometry_fields_are_refused() {
    for extra in ["70\n1\n", "9\n$INSUNITS\n70\n1\n"] {
        refuses(
            format!("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n{extra}0\nENDSEC\n0\nEOF\n")
                .as_bytes(),
            "INSUNITS",
        );
    }
    refuses(
        drawing("0\nLINE\n10\n0\n10\n2\n20\n0\n11\n1\n21\n1\n").as_bytes(),
        "duplicate",
    );
}

#[test]
fn dxf_structure_and_group_codes_are_checked() {
    for code in ["-1", "1072", "65536"] {
        refuses(format!("{code}\nEOF\n").as_bytes(), "group");
    }
    refuses(b"0\nEOF\n1\nextra\n", "EOF");
    refuses(b"0\nSECTION\n0\nENDSEC\n0\nEOF\n", "SECTION");
    refuses(b"0\nSECTION\n3\nENTITIES\n0\nENDSEC\n0\nEOF\n", "SECTION");
    refuses(
        b"0\nSECTION\n2\nENTITIES\n3\nextra\n0\nENDSEC\n0\nEOF\n",
        "ENTITIES",
    );
    refuses(
        b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n",
        "duplicate",
    );
    refuses(
        b"0\nSECTION\n2\nUNSUPPORTED\n0\nENDSEC\n0\nEOF\n",
        "UNSUPPORTED",
    );
    refuses(b"1\ntext\n0\nEOF\n", "group");
    refuses(b"0\nSECTION\n2\nENTITIES\n0\nEOF\n", "ENDSEC");
    refuses(b"0\nEOF\n\xff", "UTF-8");
}

#[test]
fn dxf_layer_table_structure_and_names_are_checked() {
    for (body, name) in [
        ("0\nTABLE\n2\nLAYER\n", "ENDTAB"),
        ("0\nTABLE\n2\nLAYER\n0\nTABLE\n2\nLAYER\n", "nested"),
        ("0\nENDTAB\n", "ENDTAB"),
        (
            "0\nTABLE\n2\nLAYER\n0\nSTYLE\n2\ntext\n0\nENDTAB\n",
            "STYLE",
        ),
        ("0\nTABLE\n2\nLAYER\n0\nLAYER\n2\n\n0\nENDTAB\n", "LAYER"),
        (
            "0\nTABLE\n2\nLAYER\n0\nLAYER\n2\ncut\n0\nLAYER\n2\ncut\n0\nENDTAB\n",
            "duplicate",
        ),
    ] {
        refuses(
            format!("0\nSECTION\n2\nTABLES\n{body}0\nENDSEC\n0\nEOF\n").as_bytes(),
            name,
        );
    }
    let text = b"0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n0\nLAYER\n2\ncut\n0\nENDTAB\n0\nENDSEC\n0\nEOF\n";
    assert!(
        parse_with_limits(
            text,
            Limits {
                max_entities: 0,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("LAYER")
    );
}

#[test]
fn dxf_limits_count_skips_vertices_and_sequence_end() {
    for (entities, cap) in [
        ("0\nTEXT\n1\nnote\n", 0),
        (
            "0\nPOLYLINE\n0\nVERTEX\n10\n0\n20\n0\n0\nVERTEX\n10\n1\n20\n1\n0\nSEQEND\n",
            1,
        ),
        (
            "0\nPOLYLINE\n0\nVERTEX\n10\n0\n20\n0\n0\nVERTEX\n10\n1\n20\n1\n0\nSEQEND\n",
            3,
        ),
    ] {
        let error = parse_with_limits(
            drawing(entities).as_bytes(),
            Limits {
                max_entities: cap,
                ..Limits::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("entity"), "{error}");
    }
}

#[test]
fn dxf_application_groups_and_paper_space_are_refused() {
    for (extra, name) in [
        ("102\n{APP\n102\n}\n", "102"),
        ("67\n1\n", "paper space"),
        ("8\n\n", "layer"),
    ] {
        refuses(
            drawing(&format!("0\nLINE\n10\n0\n20\n0\n11\n1\n21\n1\n{extra}")).as_bytes(),
            name,
        );
    }
}

#[test]
fn dxf_polyline_vertex_details_are_checked() {
    for extra in ["20\n2\n", "42\n1\n42\n2\n", "30\n1\n"] {
        refuses(
            drawing(&format!(
                "0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n{extra}10\n1\n20\n1\n"
            ))
            .as_bytes(),
            "LWPOLYLINE",
        );
    }
    refuses(
        drawing("0\nLWPOLYLINE\n90\n2\n20\n0\n10\n0\n20\n0\n10\n1\n20\n1\n").as_bytes(),
        "LWPOLYLINE",
    );
    for extra in ["70\n1\n", "40\n1\n", "41\n1\n", "8\nother\n"] {
        refuses(
            drawing(&format!(
                "0\nPOLYLINE\n0\nVERTEX\n10\n0\n20\n0\n{extra}0\nVERTEX\n10\n1\n20\n1\n0\nSEQEND\n"
            ))
            .as_bytes(),
            "VERTEX",
        );
    }
    for extra in ["10\n1\n", "20\n1\n", "75\n1\n", "40\n1\n", "41\n1\n"] {
        refuses(
            drawing(&format!(
                "0\nPOLYLINE\n{extra}0\nVERTEX\n10\n0\n20\n0\n0\nVERTEX\n10\n1\n20\n1\n0\nSEQEND\n"
            ))
            .as_bytes(),
            "POLYLINE",
        );
    }
    refuses(
        drawing("0\nLWPOLYLINE\n90\n1\n10\n0\n20\n0\n").as_bytes(),
        "vertices",
    );
}

#[test]
fn dxf_varying_z_within_and_between_entities_is_refused() {
    refuses(
        drawing("0\nLINE\n10\n0\n20\n0\n30\n1\n11\n1\n21\n1\n31\n2\n").as_bytes(),
        "Z",
    );
    refuses(
        drawing("0\nCIRCLE\n10\n0\n20\n0\n30\n1\n40\n1\n0\nCIRCLE\n10\n3\n20\n0\n30\n2\n40\n1\n")
            .as_bytes(),
        "Z",
    );
    refuses(drawing("0\nPOLYLINE\n70\n0\n0\nVERTEX\n10\n0\n20\n0\n30\n1\n0\nVERTEX\n10\n2\n20\n0\n30\n2\n0\nSEQEND\n").as_bytes(), "Z");
}

#[test]
fn dxf_polylines_refuse_bad_counts_flags_and_missing_sequence_end() {
    for count in ["-1", "3", "no"] {
        refuses(
            drawing(&format!(
                "0\nLWPOLYLINE\n90\n{count}\n10\n0\n20\n0\n10\n1\n20\n1\n"
            ))
            .as_bytes(),
            "LWPOLYLINE",
        );
    }
    refuses(
        drawing("0\nLWPOLYLINE\n90\n2\n10\n0\n10\n1\n20\n1\n").as_bytes(),
        "LWPOLYLINE",
    );
    for flags in [2, 4, 8, 16, 32, 64] {
        refuses(drawing(&format!("0\nPOLYLINE\n70\n{flags}\n0\nVERTEX\n10\n0\n20\n0\n0\nVERTEX\n10\n1\n20\n1\n0\nSEQEND\n")).as_bytes(), "POLYLINE");
    }
    refuses(
        drawing("0\nPOLYLINE\n0\nVERTEX\n10\n0\n20\n0\n0\nVERTEX\n10\n1\n20\n1\n").as_bytes(),
        "SEQEND",
    );
    refuses(drawing("0\nVERTEX\n10\n0\n20\n0\n").as_bytes(), "VERTEX");
}

#[test]
fn dxf_width_and_thickness_are_not_silently_discarded() {
    refuses(
        drawing("0\nLINE\n10\n0\n20\n0\n11\n1\n21\n1\n39\n1\n").as_bytes(),
        "thickness",
    );
    for width in ["43\n1\n", "40\n1\n", "41\n1\n"] {
        refuses(
            drawing(&format!(
                "0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n{width}10\n1\n20\n1\n"
            ))
            .as_bytes(),
            "width",
        );
    }
}

#[test]
fn dxf_input_entity_and_vertex_caps_are_enforced() {
    let text = drawing("0\nLINE\n10\n0\n20\n0\n11\n1\n21\n1\n");
    let error = parse_with_limits(
        text.as_bytes(),
        Limits {
            max_bytes: text.len() - 1,
            ..Limits::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.to_lowercase().contains("size"), "{error}");
    assert!(
        parse_with_limits(
            text.as_bytes(),
            Limits {
                max_bytes: text.len(),
                max_entities: 1,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    let error = parse_with_limits(
        text.as_bytes(),
        Limits {
            max_entities: 0,
            ..Limits::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.to_lowercase().contains("entity"), "{error}");
    for entity in [
        "0\nLWPOLYLINE\n90\n2147483647\n",
        "0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n10\n1\n20\n1\n",
        "0\nPOLYLINE\n0\nVERTEX\n10\n0\n20\n0\n0\nVERTEX\n10\n1\n20\n1\n0\nSEQEND\n",
    ] {
        let error = parse_with_limits(
            drawing(entity).as_bytes(),
            Limits {
                max_vertices: 1,
                ..Limits::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.to_lowercase().contains("vertex"), "{error}");
    }
    let two = drawing(
        "0\nLWPOLYLINE\n90\n2\n10\n0\n20\n0\n10\n1\n20\n1\n0\nLWPOLYLINE\n90\n2\n10\n3\n20\n0\n10\n4\n20\n1\n",
    );
    assert!(
        parse_with_limits(
            two.as_bytes(),
            Limits {
                max_vertices: 3,
                ..Limits::default()
            }
        )
        .is_err()
    );
}
