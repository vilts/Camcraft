//! Provenance: hand-built ASCII DXF examples from Autodesk's public group conventions; the
//! expected sizes are computed by hand. Issue 1862.
//! Gate map (M4.4): covers camcraft_geometry::sketch_data, camcraft_geometry::dxf (units).

use camcraft_geometry::dxf::{Unit, Units, parse};
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::sketch_data::{SketchData, SketchError, SourceUnits, bounds};
use std::f64::consts::{FRAC_PI_2, PI};

mod common;

/// A 100 x 50 rectangle on layer `cut`, a circle of radius 5 on layer `holes`, and one TEXT.
fn drawing(insunits: Option<i32>) -> Vec<u8> {
    let header = insunits.map_or(String::new(), |code| format!("9\n$INSUNITS\n70\n{code}\n"));
    format!(
        "0\nSECTION\n2\nHEADER\n{header}0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n1\n\
         0\nLAYER\n2\nspare\n70\n0\n62\n7\n6\nCONTINUOUS\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
         0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n100\n20\n0\n10\n100\n20\n50\n10\n0\n20\n50\n\
         0\nCIRCLE\n8\nholes\n10\n20\n20\n25\n40\n5\n\
         0\nTEXT\n8\nnotes\n10\n0\n20\n0\n40\n3\n1\nhello\n\
         0\nENDSEC\n0\nEOF\n"
    )
    .into_bytes()
}

fn read(insunits: Option<i32>, unit: Option<Unit>) -> Result<SketchData, SketchError> {
    SketchData::from_dxf(parse(&drawing(insunits)).expect("the drawing parses"), unit, None)
}

#[test]
fn a_millimetre_drawing_keeps_its_sizes_layers_chains_and_skip_counts() {
    let sketch = read(Some(4), None).expect("a drawing with units reads");
    assert_eq!(sketch.units(), SourceUnits { stated: Units::Known(Unit::Millimeters), applied: Unit::Millimeters });
    assert!(!sketch.units().overridden());
    assert_eq!(sketch.primitives().len(), 5);
    assert_eq!(sketch.layers(), ["cut", "holes", "spare"], "table layers and used layers, sorted");
    let (lo, hi) = sketch.bounds().expect("bounds");
    assert_eq!((lo, hi), (Point2::new(0.0, 0.0), Point2::new(100.0, 50.0)));
    assert_eq!(sketch.chains().closed.len(), 2, "the rectangle and the circle");
    assert!(sketch.chains().open.is_empty() && sketch.chains().rejected.is_empty());
    common::near(sketch.chains().closed[0].area, 5000.0);
    assert_eq!(sketch.summary().skipped.get("TEXT"), Some(&1), "the text is counted, not dropped silently");
    common::near(sketch.join_tolerance(), 0.01);
}

#[test]
fn an_inch_drawing_is_scaled_to_millimetres() {
    let sketch = read(Some(1), None).expect("an inch drawing reads");
    assert_eq!(sketch.units().applied, Unit::Inches);
    let (_, hi) = sketch.bounds().expect("bounds");
    common::near(hi.x, 2540.0);
    common::near(hi.y, 1270.0);
    common::near(sketch.chains().closed[0].area, 5000.0 * 25.4 * 25.4);
}

#[test]
fn a_drawing_without_units_is_refused_until_the_caller_states_them() {
    for code in [None, Some(0)] {
        let refused = read(code, None).expect_err("no unit, no answer: refused");
        assert_eq!(refused, SketchError::MissingUnits { width: 100.0, height: 50.0 });
        assert!(refused.needs_units());
        assert!(refused.to_string().contains("states no units"), "{refused}");
        let sketch = read(code, Some(Unit::Inches)).expect("the stated unit clears it");
        assert_eq!(sketch.units().stated, Units::MissingOrUnitless);
        assert!(sketch.units().overridden());
        common::near(sketch.bounds().expect("bounds").1.x, 2540.0);
    }
}

#[test]
fn an_unsupported_unit_code_is_named_and_a_stated_unit_wins_over_the_file() {
    let refused = read(Some(999), None).expect_err("code 999 is no unit");
    assert_eq!(refused, SketchError::UnsupportedUnits { code: 999 });
    assert!(refused.needs_units());
    assert_eq!(read(Some(999), Some(Unit::Millimeters)).expect("stated").units().stated, Units::Unsupported(999));
    let over = read(Some(4), Some(Unit::Inches)).expect("the caller's unit wins");
    assert!(over.units().overridden());
    common::near(over.bounds().expect("bounds").1.x, 2540.0);
}

#[test]
fn a_drawing_with_nothing_to_place_and_a_bad_tolerance_are_refused_by_name() {
    let empty = b"0\nSECTION\n2\nENTITIES\n0\nTEXT\n8\nnotes\n10\n0\n20\n0\n40\n3\n1\nhello\n0\nENDSEC\n0\nEOF\n";
    let refused = SketchData::from_dxf(parse(empty).expect("parses"), Some(Unit::Millimeters), None).expect_err("empty");
    assert_eq!(refused, SketchError::Empty { skipped: 1 });
    let doc = parse(&drawing(Some(4))).expect("parses");
    let refused = SketchData::from_dxf(doc, None, Some(50.0)).expect_err("a 50 mm join tolerance");
    assert!(refused.to_string().contains("join tolerance"), "{refused}");
    assert!(!refused.needs_units());
}

#[test]
fn stored_parts_read_back_as_the_same_sketch() {
    let sketch = read(Some(1), None).expect("reads");
    let again = SketchData::from_parts(
        sketch.primitives().to_vec(),
        sketch.layers().to_vec(),
        sketch.units(),
        Some(sketch.join_tolerance()),
        sketch.summary().clone(),
    )
    .expect("stored parts read");
    assert_eq!(again, sketch);
}

#[test]
fn unit_codes_read_back() {
    for code in 1..=24 {
        let Units::Known(unit) = Units::from_code(code) else { panic!("code {code} is a unit") };
        assert_eq!(unit.code(), code);
        assert_eq!(Units::Known(unit).code(), code);
    }
    assert_eq!(Units::MissingOrUnitless.code(), 0);
    assert_eq!(Units::Unsupported(77).code(), 77);
}

#[test]
fn arc_bounds_count_the_axis_extremes_the_sweep_passes() {
    // A quarter arc from 45 degrees to 135 degrees passes the top of its circle only.
    let quarter = [common::arc("a", 0.0, 0.0, 10.0, PI / 4.0, FRAC_PI_2)];
    let (lo, hi) = bounds(&quarter).expect("bounds");
    common::near(hi.y, 10.0);
    common::near(lo.y, 10.0 * (PI / 4.0).sin());
    common::near(hi.x, 10.0 * (PI / 4.0).cos());
    // The same arc walked clockwise from 135 degrees has the same extents.
    let back = [common::arc("a", 0.0, 0.0, 10.0, 3.0 * PI / 4.0, -FRAC_PI_2)];
    let (lo2, hi2) = bounds(&back).expect("bounds");
    common::near(lo2.x, lo.x);
    common::near(hi2.y, hi.y);
    assert!(bounds(&[]).is_none());
}

#[test]
fn invalid_summary_counts_are_refused_by_both_sketch_constructors() {
    use camcraft_geometry::dxf::{ImportSummary, Limits};
    use std::collections::BTreeMap;

    let limits = Limits::default();
    let summaries = [
        ImportSummary { entities: limits.max_entities + 1, ..Default::default() },
        ImportSummary { vertices: limits.max_vertices + 1, ..Default::default() },
        ImportSummary { skipped: BTreeMap::from([("TEXT".into(), limits.max_entities + 1)]), ..Default::default() },
        ImportSummary { skipped: BTreeMap::from([("TEXT".into(), limits.max_entities), ("DIMENSION".into(), 1)]), ..Default::default() },
        ImportSummary { skipped: BTreeMap::from([("TEXT".into(), usize::MAX), ("DIMENSION".into(), 1)]), ..Default::default() },
    ];
    for summary in summaries {
        for empty in [false, true] {
            let mut doc = parse(&drawing(Some(4))).expect("drawing parses");
            doc.summary = summary.clone();
            if empty {
                doc.primitives.clear();
            }
            let why = SketchData::from_parts(
                doc.primitives.clone(),
                doc.layers.iter().map(|l| l.name.clone()).collect(),
                SourceUnits { stated: doc.units, applied: Unit::Millimeters },
                None,
                doc.summary.clone(),
            ).expect_err("stored summary counts must be checked");
            assert!(why.to_string().contains("invalid DXF import summary"), "{why}");
            let why = SketchData::from_dxf(doc, None, None).expect_err("parsed summary counts must be checked");
            assert!(why.to_string().contains("invalid DXF import summary"), "{why}");
        }
    }
}
