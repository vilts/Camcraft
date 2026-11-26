//! Provenance: synthetic space-block and dictionary examples from public DXF group definitions.
//! Gate map (M4.4): covers camcraft_geometry::dxf.

use camcraft_geometry::dxf::{Limits, parse, parse_with_limits};

fn section(name: &str, content: &str) -> String {
    format!("0\nSECTION\n2\n{name}\n{content}0\nENDSEC\n")
}

const LINE: &str = "0\nLINE\n8\nProfile\n10\n1\n20\n3\n11\n5\n21\n7\n";

fn layout(name: &str) -> String {
    format!("0\nBLOCK\n2\n{name}\n0\nENDBLK\n")
}

fn dict(handle: &str, fields: &str) -> String {
    format!("0\nDICTIONARY\n5\n{handle}\n100\nAcDbDictionary\n{fields}")
}

#[test]
fn empty_space_definitions_and_dictionary_forest_preserve_the_document() {
    let model = "0\nBLOCK\n5\n92\n330\n91\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockBegin\n2\n*Model_Space\n70\n0\n10\n0\n20\n0\n30\n0\n3\n*MODEL_SPACE\n1\n\n0\nENDBLK\n5\n93\n330\n91\n100\nAcDbBlockEnd\n";
    let blocks = section(
        "BLOCKS",
        &format!(
            "{model}{}{}",
            layout("*paper_space"),
            layout("*Paper_Space12")
        ),
    );
    let tables = section(
        "TABLES",
        "0\nTABLE\n2\nBLOCK_RECORD\n0\nBLOCK_RECORD\n5\n91\n2\n*Model_Space\n0\nENDTAB\n",
    );
    let dictionaries = [
        dict("fB", "330\n0\n281\n1\n3\nNotes\n350\n00aB\n"),
        dict("ab", "330\nFB\n280\n1\n3\nEmpty\n360\nB3\n"),
        dict("B3", ""),
        dict("17F", "330\nFB\n"),
    ]
    .concat();
    let entities = section("ENTITIES", LINE);
    let expected = parse(format!("{entities}0\nEOF\n").as_bytes()).unwrap();
    for objects in [section("OBJECTS", ""), section("OBJECTS", &dictionaries)] {
        let input = format!("{blocks}{entities}{objects}{tables}0\nEOF\n");
        assert_eq!(parse(input.as_bytes()).unwrap(), expected);
    }
}

#[test]
fn layout_wrappers_refuse_geometry_external_content_and_broken_structure() {
    let model = "0\nBLOCK\n2\n*Model_Space\n";
    for body in [
        layout("*Paper_SpaceCAD"),
        layout("Fixture"),
        format!("{model}1\nremote.dwg\n0\nENDBLK\n"),
        format!("{model}70\n4\n0\nENDBLK\n"),
        format!("{model}3\nOther\n0\nENDBLK\n"),
        format!("{model}330\nFF\n0\nENDBLK\n"),
        format!("{model}0\nENDBLK\n102\n{{APP\n"),
        format!("{model}{LINE}0\nENDBLK\n"),
        format!("{model}0\nINSERT\n2\npart\n0\nENDBLK\n"),
        format!("{model}{model}0\nENDBLK\n"),
        format!("{model}0\nENDBLK\n{}", layout("*MODEL_SPACE")),
        model.into(),
    ] {
        let input = format!(
            "{}{}0\nEOF\n",
            section("BLOCKS", &body),
            section("ENTITIES", LINE)
        );
        assert!(parse(input.as_bytes()).is_err(), "{body}");
    }
}

#[test]
fn dictionary_entries_must_form_a_closed_acyclic_ownership_graph() {
    let root = dict("AE", "3\nChild\n350\nB1\n");
    for body in [
        root.clone(),
        format!("{root}{}", dict("B1", "330\nEE\n")),
        format!("{root}{}", dict("B1", "3\nBack\n350\naE\n")),
        format!("{root}{}", dict("B1", "330\n0\n")),
        dict("AE", "330\nF0\n"),
        dict("AE", "3\nMissingTarget\n"),
        dict("AE", "350\nB1\n"),
        dict("AE", "3\nChild\n350\nB1\n3\nchild\n350\nB2\n"),
        dict("AE", "3\nChild\n350\nB1\n3\nAlias\n350\nB1\n"),
        dict("AE", "340\nE1\n"),
        dict("AE", "100\nOtherSubclass\n"),
        dict("ZZ", ""),
        format!("{}{}", dict("F2", ""), dict("0f2", "")),
        format!("{root}0\nXRECORD\n5\nB1\n"),
        LINE.into(),
    ] {
        let input = format!("{}0\nEOF\n", section("OBJECTS", &body));
        assert!(parse(input.as_bytes()).is_err(), "{body}");
    }
}

#[test]
fn metadata_collections_obey_the_entity_limit() {
    let limits = Limits {
        max_entities: 1,
        ..Limits::default()
    };
    for (name, body) in [
        (
            "BLOCKS",
            format!("{}{}", layout("*Model_Space"), layout("*Paper_Space")),
        ),
        ("OBJECTS", format!("{}{}", dict("A", ""), dict("B", ""))),
        ("OBJECTS", dict("A", "3\nOne\n350\nB\n3\nTwo\n350\nC\n")),
    ] {
        let input = format!("{}0\nEOF\n", section(name, &body));
        let err = parse_with_limits(input.as_bytes(), limits).unwrap_err();
        assert!(err.to_string().contains("limit"), "{err}");
    }
}
