//! Provenance: a hand-written AC1015 drawing with the section and object layout of a CAD
//! export (CLASSES, reactor brackets, LAYOUT and default dictionary objects), from public DXF
//! group definitions. Handles, names and sizes are invented (#3450).
//! Gate map (M4.4): covers camcraft_geometry::dxf, camcraft_geometry::sketch_data.

use camcraft_geometry::dxf::parse;
use camcraft_geometry::sketch_data::SketchData;

fn section(name: &str, content: &str) -> String {
    format!("0\nSECTION\n2\n{name}\n{content}0\nENDSEC\n")
}

fn reactors(owner: &str) -> String {
    format!("102\n{{ACAD_REACTORS\n330\n{owner}\n102\n}}\n330\n{owner}\n")
}

/// Two closed outlines: an outer 40 x 20 and an inner 10 x 5 rectangle, as LWPOLYLINE.
fn lwpolylines() -> String {
    [(0.0, 0.0, 40.0, 20.0), (5.0, 5.0, 15.0, 10.0)]
        .iter()
        .map(|&(x0, y0, x1, y1)| {
            format!(
                "0\nLWPOLYLINE\n5\n{}\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbPolyline\n\
                 90\n4\n70\n1\n10\n{x0}\n20\n{y0}\n10\n{x1}\n20\n{y0}\n10\n{x1}\n20\n{y1}\n10\n{x0}\n20\n{y1}\n",
                if x0 == 0.0 { "30" } else { "31" }
            )
        })
        .collect()
}

/// The same outlines as R12 LINE entities.
fn lines() -> String {
    let mut out = String::new();
    for &(x0, y0, x1, y1) in &[(0.0, 0.0, 40.0, 20.0), (5.0, 5.0, 15.0, 10.0)] {
        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        for (i, &(ax, ay)) in corners.iter().enumerate() {
            let (bx, by) = corners[(i + 1) % 4];
            out += &format!("0\nLINE\n8\n0\n10\n{ax}\n20\n{ay}\n11\n{bx}\n21\n{by}\n");
        }
    }
    out
}

const HEADER: &str =
    "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n";

fn classes() -> String {
    let class = |dxf: &str, cpp: &str| {
        format!("0\nCLASS\n1\n{dxf}\n2\n{cpp}\n3\nExample Classes\n90\n0\n280\n0\n281\n0\n")
    };
    section(
        "CLASSES",
        &[
            class("ACDBDICTIONARYWDFLT", "AcDbDictionaryWithDefault"),
            class("ACDBPLACEHOLDER", "AcDbPlaceHolder"),
            class("LAYOUT", "AcDbLayout"),
        ]
        .concat(),
    )
}

fn objects() -> String {
    let body = [
        "0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n\
         3\nACAD_GROUP\n350\nD\n3\nACAD_LAYOUT\n350\n1A\n3\nACAD_PLOTSTYLENAME\n350\nE\n"
            .to_owned(),
        format!("0\nDICTIONARY\n5\nD\n{}100\nAcDbDictionary\n281\n1\n", reactors("C")),
        format!(
            "0\nDICTIONARY\n5\n1A\n{}100\nAcDbDictionary\n281\n1\n3\nLayout1\n350\n1E\n3\nModel\n350\n22\n",
            reactors("C")
        ),
        format!(
            "0\nACDBDICTIONARYWDFLT\n5\nE\n{}100\nAcDbDictionary\n281\n1\n3\nNormal\n350\nF\n\
             100\nAcDbDictionaryWithDefault\n340\nF\n",
            reactors("C")
        ),
        format!("0\nACDBPLACEHOLDER\n5\nF\n{}", reactors("E")),
        format!(
            "0\nLAYOUT\n5\n22\n{}100\nAcDbPlotSettings\n1\n\n2\nnone_device\n4\n\n6\n\n40\n0.0\n70\n1712\n\
             100\nAcDbLayout\n1\nModel\n70\n1\n71\n0\n10\n0.0\n20\n0.0\n11\n420.0\n21\n297.0\n76\n0\n330\n1F\n",
            reactors("1A")
        ),
        format!(
            "0\nLAYOUT\n5\n1E\n{}100\nAcDbPlotSettings\n1\n\n2\nnone_device\n70\n688\n\
             100\nAcDbLayout\n1\nLayout1\n70\n1\n71\n1\n76\n0\n330\n1B\n",
            reactors("1A")
        ),
    ]
    .concat();
    section("OBJECTS", &body)
}

fn ac1015() -> String {
    format!(
        "{HEADER}{}{}{}0\nEOF\n",
        classes(),
        section("ENTITIES", &lwpolylines()),
        objects()
    )
}

fn r12() -> String {
    format!("{HEADER}{}0\nEOF\n", section("ENTITIES", &lines()))
}

fn closed_areas(text: &str) -> Vec<f64> {
    let doc = parse(text.as_bytes()).expect("the drawing parses");
    let sketch = SketchData::from_dxf(doc, None, None).expect("the drawing reads");
    assert!(sketch.chains().open.is_empty() && sketch.chains().rejected.is_empty());
    let mut areas: Vec<f64> = sketch
        .chains()
        .closed
        .iter()
        .map(|c| c.area.abs())
        .collect();
    areas.sort_by(f64::total_cmp);
    areas
}

#[test]
fn an_ac1015_export_gives_the_same_closed_chains_as_its_r12_twin() {
    let full = closed_areas(&ac1015());
    assert_eq!(full.len(), 2, "two closed outlines");
    let twin = closed_areas(&r12());
    assert_eq!(full.len(), twin.len());
    for (a, b) in full.iter().zip(&twin) {
        assert!((a - b).abs() < 1e-9, "{a} / {b}");
    }
    assert!(
        (full[0] - 50.0).abs() < 1e-9 && (full[1] - 800.0).abs() < 1e-9,
        "{full:?}"
    );
}

#[test]
fn a_truncated_ac1015_export_never_imports() {
    let text = ac1015();
    let bytes = text.as_bytes();
    // Every cut before the final EOF record is refused with an error, never a panic.
    let eof = text.rfind("0\nEOF").expect("an EOF record");
    for cut in 0..eof {
        assert!(parse(&bytes[..cut]).is_err(), "cut at {cut} imported");
    }
}

#[test]
fn a_mutated_ac1015_export_never_panics() {
    let seed = ac1015().into_bytes();
    let mut state = 0x3450u64;
    for _ in 0..2000 {
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
fn many_inert_objects_stop_at_the_entity_limit() {
    use camcraft_geometry::dxf::{Limits, parse_with_limits};
    let mut body = "0\nDICTIONARY\n5\nA\n330\n0\n100\nAcDbDictionary\n".to_owned();
    for handle in 0x100..0x200 {
        body += &format!("0\nXRECORD\n5\n{handle:X}\n330\nA\n100\nAcDbXrecord\n");
    }
    let text = format!(
        "{}{}0\nEOF\n",
        section("ENTITIES", &lines()),
        section("OBJECTS", &body)
    );
    let limits = Limits {
        max_entities: 64,
        ..Limits::default()
    };
    let err = parse_with_limits(text.as_bytes(), limits)
        .unwrap_err()
        .to_string();
    assert!(err.contains("object count exceeds limit"), "{err}");
    assert!(parse(text.as_bytes()).is_ok(), "within the default limit");
}
