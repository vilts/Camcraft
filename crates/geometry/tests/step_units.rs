//! #306: a STEP file's length unit in millimetres, read from every
//! `GLOBAL_UNIT_ASSIGNED_CONTEXT`. Moved from the app's in-file tests with
//! the reader (#2789: it reads through solidcore's Part 21 parser now).
//! Provenance: our own test design; the unit entities per ISO 10303-41.
//!
//! Gate map (#2789): covers camcraft_geometry::step_units.

#![cfg(not(target_arch = "wasm32"))]

use camcraft_geometry::step_units::length_unit_mm;

/// A Part 21 header as exporters write it.
const HEADER: &str = "HEADER;\nFILE_DESCRIPTION( ( '' ), '2;1' );\n\
                      FILE_NAME( 'units.step', '2026-11-08T00:00:00', ( '' ), ( '' ), '', '', '' );\n\
                      FILE_SCHEMA( ( 'AUTOMOTIVE_DESIGN' ) );\nENDSEC;";

/// The units of a file whose DATA section is `data`.
fn units(data: &str) -> Result<Option<f64>, String> {
    length_unit_mm(&format!("ISO-10303-21;\n{HEADER}\nDATA;\n{data}\nENDSEC;\nEND-ISO-10303-21;\n"))
}

/// The angle and solid-angle units every exporter writes beside the
/// length unit, so the reader is shown to skip them.
const ANGLES: &str = "#73 = ( NAMED_UNIT( * ) PLANE_ANGLE_UNIT( ) SI_UNIT( $, .RADIAN. ) );\n\
                      #74 = ( NAMED_UNIT( * ) SI_UNIT( $, .STERADIAN. ) SOLID_ANGLE_UNIT( ) );";

fn context(length: u64) -> String {
    format!(
        "#10 = ( GEOMETRIC_REPRESENTATION_CONTEXT( 3 ) GLOBAL_UNIT_ASSIGNED_CONTEXT( ( #{length}, #73, #74 ) ) REPRESENTATION_CONTEXT( '', '' ) );\n{ANGLES}"
    )
}

#[test]
fn an_si_length_unit_reads_its_prefix() {
    let si = |prefix: &str| {
        units(&format!(
            "{}\n#72 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( {prefix}, .METRE. ) );",
            context(72)
        ))
    };
    assert_eq!(si(".MILLI."), Ok(Some(1.0)));
    assert_eq!(si(".CENTI."), Ok(Some(10.0)));
    assert_eq!(si("$"), Ok(Some(1000.0)), "no prefix is the metre");
}

#[test]
fn an_inch_unit_is_twenty_five_point_four_millimetres() {
    let data = format!(
        "{}\n#72 = ( CONVERSION_BASED_UNIT( 'INCH', #76 ) LENGTH_UNIT( ) NAMED_UNIT( #75 ) );\n\
         #75 = DIMENSIONAL_EXPONENTS( 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0 );\n\
         #76 = LENGTH_MEASURE_WITH_UNIT( LENGTH_MEASURE( 25.4 ), #77 );\n\
         #77 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( .MILLI., .METRE. ) );",
        context(72)
    );
    let k = units(&data).expect("inch reads").expect("a length unit");
    assert!((k - 25.4).abs() < 1e-12, "{k}");
}

#[test]
fn a_file_with_no_unit_context_keeps_millimetres() {
    assert_eq!(units(ANGLES), Ok(None));
}

#[test]
fn contexts_that_disagree_or_a_unit_that_cannot_be_read_refuse_by_name() {
    let two = format!(
        "{}\n#11 = ( GLOBAL_UNIT_ASSIGNED_CONTEXT( ( #82 ) ) REPRESENTATION_CONTEXT( '', '' ) );\n\
         #72 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( .MILLI., .METRE. ) );\n\
         #82 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( $, .METRE. ) );",
        context(72)
    );
    assert!(
        units(&two).is_err_and(|e| e.contains("two length units")),
        "{:?}",
        units(&two)
    );
    let odd = format!(
        "{}\n#72 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( .EXA., .METRE. ) );",
        context(72)
    );
    assert!(
        units(&odd).is_err_and(|e| e.contains("EXA")),
        "{:?}",
        units(&odd)
    );
}

/// #2789 review: a length unit that no context states is refused by name,
/// never read as 1 mm per unit. Angle units alone (a radian, a degree
/// converted from it) are no length unit, and keep millimetres.
#[test]
fn a_length_unit_that_no_context_states_is_refused_by_name() {
    // The context lists only the angle units; the inch stands beside it.
    let data = format!(
        "#10 = ( GEOMETRIC_REPRESENTATION_CONTEXT( 3 ) GLOBAL_UNIT_ASSIGNED_CONTEXT( ( #73, #74 ) ) REPRESENTATION_CONTEXT( '', '' ) );\n{ANGLES}\n\
         #72 = ( CONVERSION_BASED_UNIT( 'INCH', #76 ) LENGTH_UNIT( ) NAMED_UNIT( * ) );\n\
         #76 = LENGTH_MEASURE_WITH_UNIT( LENGTH_MEASURE( 25.4 ), #77 );\n\
         #77 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( .MILLI., .METRE. ) );"
    );
    let got = units(&data);
    assert!(
        got.as_ref().is_err_and(|e| e.contains("length unit #72") && e.contains("GLOBAL_UNIT_ASSIGNED_CONTEXT")),
        "{got:?}"
    );
    // A metre SI unit without its LENGTH_UNIT part is a length unit too.
    let bare = units("#72 = ( NAMED_UNIT( * ) SI_UNIT( $, .METRE. ) );");
    assert!(bare.as_ref().is_err_and(|e| e.contains("length unit #72")), "{bare:?}");
    // A degree is a conversion-based unit of an angle, not of a length.
    let degree = format!(
        "{ANGLES}\n#75 = ( CONVERSION_BASED_UNIT( 'DEGREE', #76 ) NAMED_UNIT( * ) PLANE_ANGLE_UNIT( ) );\n\
         #76 = PLANE_ANGLE_MEASURE_WITH_UNIT( PLANE_ANGLE_MEASURE( 0.0174532925 ), #73 );"
    );
    assert_eq!(units(&degree), Ok(None));
}

/// #2789: a unit that converts to itself is refused at the chain cap, never
/// followed forever.
#[test]
fn a_unit_that_converts_to_itself_is_refused_at_the_chain_cap() {
    let data = format!(
        "{}\n#72 = ( CONVERSION_BASED_UNIT( 'LOOP', #76 ) LENGTH_UNIT( ) NAMED_UNIT( * ) );\n\
         #76 = LENGTH_MEASURE_WITH_UNIT( LENGTH_MEASURE( 2.0 ), #72 );",
        context(72)
    );
    let got = units(&data);
    assert!(got.as_ref().is_err_and(|e| e.contains("chains through more than")), "{got:?}");
}
