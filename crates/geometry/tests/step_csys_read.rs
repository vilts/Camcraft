//! #2789: the tool coordinate-system reader on solidcore's Part 21 parser
//! keeps the file-level refusals of the ruststep reader it replaced, and
//! names the readings that changed: an entity id defined twice is refused,
//! several DATA sections are read as one, a missing DATA section is refused
//! by its old text, and no corrupt input panics or hangs. The fixture
//! results themselves are pinned end to end in the app's `tool_csys` suite.
//! Provenance: our own test design; Part 21 structure per ISO 10303-21.
//!
//! Gate map (#2789): covers camcraft_geometry::step_csys.

#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use camcraft_geometry::step_csys::{read_tool_csys, CsysRole};

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name);
    std::fs::read_to_string(path).expect("fixture reads").replace("\r\n", "\n")
}

/// `tool_csys_iso13399.step` with `from` replaced by `to` (which must be
/// in it).
fn edited(from: &str, to: &str) -> String {
    let text = fixture("tool_csys_iso13399.step");
    assert!(text.contains(from), "the fixture has {from:?}");
    text.replacen(from, to, 1)
}

#[test]
fn the_fixture_reads_through_the_part_21_parser() {
    let r = read_tool_csys(&fixture("tool_csys_iso13399.step")).expect("reads");
    let cip = r.find(CsysRole::InProcess).expect("one CIP");
    assert_eq!(cip.entity, 50);
    assert!((cip.origin[2] + 75.0).abs() < 1e-12, "{:?}", cip.origin);
    assert_eq!(r.find(CsysRole::Primary).map(|c| c.entity), Ok(30));
}

/// The fixture's millimetre unit line.
const MM: &str = "#12 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );";
/// An inch: 25.4 of the millimetre (ids 15-17 are free in the fixture).
const INCH: &str = "#12 = ( CONVERSION_BASED_UNIT('INCH', #15) LENGTH_UNIT() NAMED_UNIT(#16) );\n\
                    #15 = LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4), #17);\n\
                    #16 = DIMENSIONAL_EXPONENTS(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);\n\
                    #17 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );";
/// The metre.
const METRE: &str = "#12 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT($,.METRE.) );";

/// `text` with the ISO 10303-21 short names that the parser maps to long
/// names (`A2PL3D`, `CRTPNT`, `DRCTN`, `LNGUNT`, ...). The unit context
/// gets `context`: the parser maps no short form of
/// `GLOBAL_UNIT_ASSIGNED_CONTEXT`, so any name it leaves as written stands
/// for one (we do not state the ISO short form here). `SI_UNIT` keeps its
/// long name for the same reason.
fn short_names(text: &str, context: &str) -> String {
    [
        ("AXIS2_PLACEMENT_3D(", "A2PL3D("),
        ("CARTESIAN_POINT(", "CRTPNT("),
        ("DIRECTION(", "DRCTN("),
        ("GEOMETRIC_REPRESENTATION_CONTEXT(", "GMRPCN("),
        ("LENGTH_UNIT(", "LNGUNT("),
        ("NAMED_UNIT(", "NMDUNT("),
        ("CONVERSION_BASED_UNIT(", "CNBSUN("),
        ("LENGTH_MEASURE_WITH_UNIT(", "LMWU("),
        ("DIMENSIONAL_EXPONENTS(", "DMNEXP("),
        ("GLOBAL_UNIT_ASSIGNED_CONTEXT(", context),
    ]
    .iter()
    .fold(text.to_string(), |t, (long, short)| t.replace(long, short))
}

/// The CIP's z in millimetres (the fixture writes -75 file units).
fn cip_z(text: &str) -> Result<f64, String> {
    let r = read_tool_csys(text)?;
    r.find(CsysRole::InProcess).map(|c| c.origin[2])
}

/// #2789 review: a short-name tool file whose unit context the parser does
/// not read, in inches or metres, is refused by name. The old reader found
/// no placement in it; the new one finds the CIP and must not place it at
/// 1 mm per unit (25.4 or 1000 times too small).
#[test]
fn a_short_name_tool_file_in_inches_or_metres_is_refused_by_name() {
    for unit in [INCH, METRE] {
        let text = short_names(&edited(MM, unit), "UNMAPPED_UNIT_CONTEXT(");
        let got = cip_z(&text);
        assert!(
            got.as_ref()
                .is_err_and(|e| e.contains("length unit #12") && e.contains("GLOBAL_UNIT_ASSIGNED_CONTEXT")),
            "{unit}: {got:?}"
        );
    }
}

/// The same short-name files with a unit context that the parser reads
/// place the CIP at the file's scale: the refusal above is the context's.
#[test]
fn a_short_name_tool_file_with_a_long_unit_context_reads_at_its_scale() {
    for (unit, k) in [(INCH, 25.4), (METRE, 1000.0), (MM, 1.0)] {
        let text = short_names(&edited(MM, unit), "GLOBAL_UNIT_ASSIGNED_CONTEXT(");
        let z = cip_z(&text).expect("reads");
        assert!((z + 75.0 * k).abs() < 1e-9, "{unit}: {z}");
    }
}

#[test]
fn a_long_name_tool_file_in_inches_reads_at_twenty_five_point_four() {
    let z = cip_z(&edited(MM, INCH)).expect("reads");
    assert!((z + 75.0 * 25.4).abs() < 1e-9, "{z}");
}

/// A file with no unit record at all keeps 1 mm per unit, as the ruststep
/// reader did (its `mm_per_unit` gave `None`, read as 1.0).
#[test]
fn a_tool_file_with_no_unit_record_keeps_millimetres() {
    let text = fixture("tool_csys_iso13399.step");
    let context = text
        .lines()
        .find(|l| l.starts_with("#10 ="))
        .expect("the fixture has its context")
        .to_string();
    let mut text = text.replacen(&context, "#10 = ( GEOMETRIC_REPRESENTATION_CONTEXT(3) REPRESENTATION_CONTEXT('', '') );", 1);
    for id in ["#12 =", "#13 =", "#14 ="] {
        let line = text.lines().find(|l| l.starts_with(id)).expect("unit line").to_string();
        text = text.replacen(&format!("{line}\n"), "", 1);
    }
    assert!(!text.contains("_UNIT"), "no unit record is left");
    assert_eq!(cip_z(&text), Ok(-75.0));
}

#[test]
fn an_entity_id_defined_twice_is_refused_by_name() {
    // A second #51 that moves the CIP: the old reader took the last
    // definition, the parser keeps the first. Neither is safe to use.
    let text = edited(
        "#51 = CARTESIAN_POINT('', (0.0, 0.0, -75.0));",
        "#51 = CARTESIAN_POINT('', (0.0, 0.0, -75.0));\n#51 = CARTESIAN_POINT('', (0.0, 0.0, -90.0));",
    );
    let e = read_tool_csys(&text).expect_err("a duplicate id is refused");
    assert!(e.contains("entity #51 is defined more than once"), "{e}");
}

#[test]
fn a_file_with_no_data_section_is_refused_by_its_old_text() {
    let text = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('X'));\nENDSEC;\nEND-ISO-10303-21;\n";
    assert_eq!(read_tool_csys(text).err().as_deref(), Some("file has no DATA section"));
    // An empty DATA section is a file that names no coordinate systems.
    let empty = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n";
    let r = read_tool_csys(empty).expect("an empty DATA section reads");
    assert_eq!(r.none_found().as_deref(), Some("the file names no coordinate systems"));
}

#[test]
fn several_data_sections_are_read_as_one() {
    // The CIP and its points move to a second DATA section.
    let text = fixture("tool_csys_iso13399.step");
    let cip = "#50 = AXIS2_PLACEMENT_3D('CIP', #51, #52, #53);\n";
    let text = text.replacen(cip, "", 1).replacen(
        "ENDSEC;\nEND-ISO-10303-21;",
        &format!("ENDSEC;\nDATA;\n{cip}ENDSEC;\nEND-ISO-10303-21;"),
        1,
    );
    let r = read_tool_csys(&text).expect("reads");
    assert_eq!(r.find(CsysRole::InProcess).map(|c| c.entity), Ok(50));
}

#[test]
fn a_syntax_error_names_the_entity_and_its_line() {
    let text = edited("#41 = CARTESIAN_POINT('', (0.0, 0.0, -40.0));", "#41 = CARTESIAN_POINT('', (0.0,, -40.0));");
    let e = read_tool_csys(&text).expect_err("refused");
    assert!(e.contains("entity #41") && e.contains("line 20"), "{e}");
}

/// The parser's own depth cap (64) does not read an empty innermost `()`
/// one level past it, so it takes this record. That is why the app's
/// nesting scan (`step_depth`, which refuses it) stays in front of every
/// read: the app's `step_nesting` suite pins the refusal.
#[test]
fn the_parser_alone_takes_an_empty_list_one_past_its_depth_cap() {
    let deep = format!("#999999 = DEEP_LIST({}{});\n", "(".repeat(64), ")".repeat(64));
    let text = edited("ENDSEC;\nEND-ISO-10303-21;", &format!("{deep}ENDSEC;\nEND-ISO-10303-21;"));
    assert!(read_tool_csys(&text).is_ok(), "the parser has no law for this record");
}

#[test]
fn corrupt_input_is_a_clean_error_never_a_panic() {
    let good = fixture("tool_csys_iso13399.step");
    let mut cases: Vec<String> = vec![
        String::new(),
        "ISO-10303-21; junk".into(),
        "ISO-10303-21;\nDATA;\n#1 = A('open);\nENDSEC;\nEND-ISO-10303-21;\n".into(),
        "ISO-10303-21;\nDATA;\n#1 = A(1);\n/* never closed\nENDSEC;\nEND-ISO-10303-21;\n".into(),
        "ISO-10303-21;\nDATA;\n#1 = A(\u{0});\nENDSEC;\nEND-ISO-10303-21;\n".into(),
        format!("ISO-10303-21;\nDATA;\n#1 = A({});\nENDSEC;\nEND-ISO-10303-21;\n", "(".repeat(5000)),
    ];
    // Every prefix of the fixture at a coarse step: truncated files.
    let step = (good.len() / 40).max(1);
    cases.extend((0..good.len()).step_by(step).filter_map(|n| good.get(..n)).map(str::to_string));
    for text in &cases {
        let _ = read_tool_csys(text);
    }
    // A self-referencing placement refuses, never hangs (a unit that loops:
    // the `step_units` suite).
    let looped = edited("#50 = AXIS2_PLACEMENT_3D('CIP', #51, #52, #53);", "#50 = AXIS2_PLACEMENT_3D('CIP', #50, #50, #50);");
    let r = read_tool_csys(&looped).expect("reads");
    assert!(r.find(CsysRole::InProcess).is_err_and(|e| e.contains("#50")), "{:?}", r.refused);
}
