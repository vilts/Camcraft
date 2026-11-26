//! Provenance: synthetic AC1015 object records from public DXF group definitions (#3450).
//! Gate map (M4.4): covers camcraft_geometry::dxf.

use camcraft_geometry::dxf::parse;

fn section(name: &str, content: &str) -> String {
    format!("0\nSECTION\n2\n{name}\n{content}0\nENDSEC\n")
}

fn reactors(owner: &str) -> String {
    format!("102\n{{ACAD_REACTORS\n330\n{owner}\n102\n}}\n330\n{owner}\n")
}

const HEADER: &str =
    "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n";
const CLASSES: &str = "0\nSECTION\n2\nCLASSES\n0\nCLASS\n1\nSAMPLEOBJ\n2\nSampleObject\n3\nSampleApp\n90\n0\n280\n0\n281\n0\n0\nENDSEC\n";
const ENTITIES: &str = "0\nSECTION\n2\nENTITIES\n\
    0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n12\n20\n0\n10\n12\n20\n7\n10\n0\n20\n7\n\
    0\nLINE\n8\ncut\n10\n2\n20\n2\n11\n9\n21\n5\n0\nENDSEC\n";

/// A root dictionary with a group dictionary, a layout dictionary and a plot-style
/// dictionary with a default, as an AC1015 exporter writes them. Handles are invented.
fn objects() -> String {
    let body = [
        "0\nDICTIONARY\n5\nA0\n330\n0\n100\nAcDbDictionary\n281\n1\n\
         3\nGROUPS\n350\nA1\n3\nLAYOUTS\n350\nA2\n3\nSTYLES\n350\nA3\n"
            .to_owned(),
        format!("0\nDICTIONARY\n5\nA1\n{}100\nAcDbDictionary\n281\n1\n", reactors("A0")),
        format!(
            "0\nDICTIONARY\n5\nA2\n{}100\nAcDbDictionary\n281\n1\n3\nSheet\n350\nB1\n3\nModel\n350\nB2\n",
            reactors("A0")
        ),
        format!(
            "0\nACDBDICTIONARYWDFLT\n5\nA3\n{}100\nAcDbDictionary\n281\n1\n3\nPlain\n350\nA4\n\
             100\nAcDbDictionaryWithDefault\n340\nA4\n",
            reactors("A0")
        ),
        format!("0\nACDBPLACEHOLDER\n5\nA4\n{}", reactors("A3")),
        format!(
            "0\nLAYOUT\n5\nB1\n{}100\nAcDbPlotSettings\n1\n\n2\nnone\n40\n0.0\n70\n688\n\
             100\nAcDbLayout\n1\nSheet\n70\n1\n71\n1\n10\n0.0\n20\n0.0\n330\nC1\n",
            reactors("A2")
        ),
        format!(
            "0\nLAYOUT\n5\nB2\n{}100\nAcDbPlotSettings\n70\n1712\n100\nAcDbLayout\n1\nModel\n330\nC0\n",
            reactors("A2")
        ),
    ]
    .concat();
    section("OBJECTS", &body)
}

#[test]
fn ac1015_classes_and_objects_import_the_same_entities_as_the_bare_file() {
    let bare = parse(format!("{HEADER}{ENTITIES}0\nEOF\n").as_bytes()).unwrap();
    // The closed four-vertex LWPOLYLINE gives four segments, the LINE one.
    assert_eq!(bare.primitives.len(), 5);
    let full = format!("{HEADER}{CLASSES}{ENTITIES}{}0\nEOF\n", objects());
    assert_eq!(parse(full.as_bytes()).unwrap(), bare);
}

fn refusal(objects_body: &str) -> String {
    let input = format!("{ENTITIES}{}0\nEOF\n", section("OBJECTS", objects_body));
    match parse(input.as_bytes()) {
        Ok(_) => panic!("accepted: {objects_body}"),
        Err(err) => err.to_string(),
    }
}

const ROOT: &str = "0\nDICTIONARY\n5\nA0\n330\n0\n100\nAcDbDictionary\n3\nOne\n350\nA1\n";

#[test]
fn unbalanced_group_102_blocks_are_named_errors() {
    let dict = |fields: &str| {
        format!("{ROOT}0\nDICTIONARY\n5\nA1\n{fields}330\nA0\n100\nAcDbDictionary\n")
    };
    let object =
        |fields: &str| format!("{ROOT}0\nXRECORD\n5\nA1\n{fields}330\nA0\n100\nAcDbXrecord\n");
    for (body, message) in [
        (
            dict("102\n{ACAD_REACTORS\n330\nA0\n"),
            "unclosed group 102 block",
        ),
        (dict("102\n}\n"), "group 102 closes no open block"),
        (
            dict("102\n{A\n102\n{B\n102\n}\n102\n}\n"),
            "nested group 102 block",
        ),
        (dict("102\nREACTORS\n"), "malformed group 102 block marker"),
        (dict("102\n{\n102\n}\n"), "malformed group 102 block marker"),
        (
            object("102\n{ACAD_REACTORS\n330\nA0\n"),
            "unclosed group 102 block",
        ),
        (object("102\n}\n"), "group 102 closes no open block"),
    ] {
        let err = refusal(&body);
        assert!(err.contains(message), "{err} / {body}");
    }
}

#[test]
fn a_file_that_ends_inside_objects_is_a_named_error() {
    for tail in ["", "0\nEOF\n", "0\nSECTION\n2\nTABLES\n0\nENDSEC\n0\nEOF\n"] {
        let input = format!(
            "{ENTITIES}0\nSECTION\n2\nOBJECTS\n{ROOT}0\nDICTIONARY\n5\nA1\n330\nA0\n{tail}"
        );
        let err = parse(input.as_bytes()).unwrap_err().to_string();
        assert!(err.contains("OBJECTS: missing ENDSEC"), "{err} / {tail}");
    }
}

#[test]
fn inert_objects_must_be_owned_members_of_the_object_tree() {
    let child = "0\nDICTIONARY\n5\nA1\n330\nA0\n100\nAcDbDictionary\n";
    for (body, message) in [
        (
            format!("{ROOT}{child}0\nXRECORD\n5\nB1\n100\nAcDbXrecord\n"),
            "missing object owner",
        ),
        (
            format!("{ROOT}{child}0\nXRECORD\n5\nB1\n330\n0\n"),
            "missing object owner",
        ),
        (
            format!("{ROOT}{child}0\nXRECORD\n330\nA1\n"),
            "missing or zero object handle",
        ),
        (
            format!("{ROOT}{child}0\nXRECORD\n5\n0\n330\nA1\n"),
            "missing or zero object handle",
        ),
        (
            format!("{ROOT}{child}0\nXRECORD\n5\nA1\n330\nA0\n"),
            "duplicate object handle",
        ),
        (
            format!("{ROOT}{child}0\nXRECORD\n5\nB1\n5\nB2\n330\nA1\n"),
            "duplicate object header field",
        ),
        (
            format!("{ROOT}{child}0\nXRECORD\n5\nB1\n330\nA1\n330\nA0\n"),
            "duplicate object header field",
        ),
        (
            format!("{ROOT}{child}0\nXRECORD\n5\nB1\n330\nEE\n"),
            "unresolved dictionary owner",
        ),
        (
            format!("{ROOT}0\nXRECORD\n5\nA1\n330\nA1\n"),
            "entry target names a different owner",
        ),
        (
            format!(
                "{ROOT}{child}0\nLINE\n5\nB1\n330\nA1\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0\n20\n0\n11\n1\n21\n1\n"
            ),
            "graphical entity in OBJECTS",
        ),
    ] {
        let err = refusal(&body);
        assert!(err.contains(message), "{err} / {body}");
    }
}

#[test]
fn object_data_after_the_subclass_marker_stays_opaque() {
    // A second 330 or a 102 after the first subclass marker is object data, not a header field.
    let body = format!(
        "{ROOT}0\nDICTIONARY\n5\nA1\n330\nA0\n100\nAcDbDictionary\n3\nTwo\n350\nB1\n\
         0\nLAYOUT\n5\nB1\n330\nA1\n100\nAcDbLayout\n330\nC0\n102\n{{OPAQUE\n"
    );
    let with = format!("{ENTITIES}{}0\nEOF\n", section("OBJECTS", &body));
    let without = format!("{ENTITIES}0\nEOF\n");
    assert_eq!(
        parse(with.as_bytes()).unwrap(),
        parse(without.as_bytes()).unwrap()
    );
}
