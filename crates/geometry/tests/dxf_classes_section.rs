//! Provenance: synthetic AC1015 section layout from public DXF group definitions; class values are invented.
//! Gate map (M4.4): covers camcraft_geometry::dxf.

use camcraft_geometry::dxf::parse;

fn section(name: &str, content: &str) -> String {
    format!("0\nSECTION\n2\n{name}\n{content}0\nENDSEC\n")
}

const LINE: &str = "0\nLINE\n8\nProfile\n10\n1\n20\n3\n11\n5\n21\n7\n";
const CLASS: &str =
    "0\nCLASS\n1\nEXAMPLE_OBJECT\n2\nAcDbExample\n3\nExample|Test\n90\n0\n91\n0\n280\n0\n281\n0\n";

fn file(classes: &str) -> String {
    let header = section("HEADER", "9\n$ACADVER\n1\nAC1015\n");
    let tables = section(
        "TABLES",
        "0\nTABLE\n2\nLAYER\n0\nLAYER\n2\nProfile\n70\n0\n62\n7\n6\nCONTINUOUS\n0\nENDTAB\n",
    );
    let entities = section("ENTITIES", LINE);
    format!("{header}{classes}{tables}{entities}0\nEOF\n")
}

#[test]
fn classes_section_is_skipped_and_entities_import() {
    let with = parse(file(&section("CLASSES", &CLASS.repeat(2))).as_bytes()).unwrap();
    assert!(!with.primitives.is_empty());
    assert_eq!(with, parse(file("").as_bytes()).unwrap());
}

#[test]
fn empty_classes_section_is_skipped() {
    let with = parse(file(&section("CLASSES", "")).as_bytes()).unwrap();
    assert_eq!(with, parse(file("").as_bytes()).unwrap());
}

#[test]
fn classes_section_without_endsec_is_a_named_error() {
    let text = format!("0\nSECTION\n2\nCLASSES\n{CLASS}");
    let error = parse(text.as_bytes()).unwrap_err().0;
    assert!(
        error.contains("CLASSES") && error.contains("ENDSEC"),
        "{error}"
    );
}

#[test]
fn thumbnailimage_section_is_skipped() {
    let body = "90\n4\n310\n89504E47\n310\n0D0A1A0A\n";
    let with = parse(file(&section("THUMBNAILIMAGE", body)).as_bytes()).unwrap();
    assert!(!with.primitives.is_empty());
    assert_eq!(with, parse(file("").as_bytes()).unwrap());
}

#[test]
fn acdsdata_section_is_skipped() {
    let body =
        "70\n2\n71\n6\n0\nACDSSCHEMA\n90\n0\n1\nAcDb_Thumbnail_Schema\n0\nACDSRECORD\n90\n1\n";
    let with = parse(file(&section("ACDSDATA", body)).as_bytes()).unwrap();
    assert_eq!(with, parse(file("").as_bytes()).unwrap());
}

#[test]
fn inert_section_without_endsec_is_a_named_error() {
    for name in ["ACDSDATA", "THUMBNAILIMAGE"] {
        let text = format!("0\nSECTION\n2\n{name}\n90\n4\n0\nEOF\n");
        let error = parse(text.as_bytes()).unwrap_err().0;
        assert!(error.contains(name) && error.contains("ENDSEC"), "{error}");
        let text = format!("0\nSECTION\n2\n{name}\n90\n4\n");
        assert!(parse(text.as_bytes()).is_err());
    }
}

#[test]
fn other_unknown_sections_stay_refused() {
    let error = parse(file(&section("UNSUPPORTED", "")).as_bytes())
        .unwrap_err()
        .0;
    assert!(error.contains("unsupported SECTION"), "{error}");
}

#[test]
fn an_entity_in_a_section_without_geometry_is_refused() {
    // A drawable record in a skipped section would be dropped silently: refuse it instead.
    for name in ["CLASSES", "ACDSDATA", "THUMBNAILIMAGE"] {
        for record in [
            LINE,
            "0\nlwpolyline\n90\n2\n10\n0\n20\n0\n10\n1\n20\n0\n",
            "0\nINSERT\n2\nB\n10\n0\n20\n0\n",
        ] {
            let error = parse(file(&section(name, record)).as_bytes())
                .unwrap_err()
                .0;
            assert!(
                error.contains(name) && error.contains("entity in a section without geometry"),
                "{error}"
            );
        }
    }
}
