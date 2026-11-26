//! #912: the STEP syntax locator names the first DATA record that does not
//! parse, by `#id`, line and text. Its records are checked with solidcore's
//! Part 21 parser since #2789 (it was ruststep's entity parser); these pins
//! hold its wording and what it passes. The end-to-end refusals of the
//! model import are pinned in the app's `import` suite.
//! Provenance: our own test design; record framing per ISO 10303-21.
//!
//! Gate map (#2789): covers camcraft_geometry::step_diag.

#![cfg(not(target_arch = "wasm32"))]

use camcraft_geometry::step_diag::{locate, Failure};

fn file(data: &str) -> String {
    format!("ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('X'));\nENDSEC;\nDATA;\n{data}ENDSEC;\nEND-ISO-10303-21;\n")
}

#[test]
fn a_record_that_does_not_parse_is_named_with_its_line_and_text() {
    let f = locate(&file("#1 = A(1.0, 'x');\n#22 = B(1,,2);\n#3 = C();\n")).expect("found");
    assert_eq!(
        f,
        Failure::Entity { id: Some("#22".into()), line: 7, text: "#22 = B(1,,2)".into() }
    );
    assert_eq!(
        f.to_string(),
        "syntax error in DATA section: entity #22 at line 7 cannot be parsed: #22 = B(1,,2)"
    );
}

#[test]
fn records_the_parser_reads_pass() {
    let data = "#1 = A(1.0, -2, 1.E-3, 'it''s', .T., $, *, #2, (1, (2, ())), B(3.0), \"0F\");\n\
                #2 = ( C(1) D('x') E() );\n\
                /* a comment; with a semicolon */\n\
                #3 = F('\\X2\\00E9\\X0\\');\n";
    assert_eq!(locate(&file(data)), None);
}

#[test]
fn a_record_that_is_not_one_entity_is_named() {
    let f = locate(&file("#1 = A(1);\nB(2);\n")).expect("found");
    assert!(matches!(f, Failure::Entity { id: None, line: 7, .. }), "{f:?}");
    let g = locate(&file("#1 = A(1) #2 = B(2);\n")).expect("found");
    assert!(matches!(g, Failure::Entity { id: Some(ref i), .. } if i == "#1"), "{g:?}");
}

#[test]
fn no_data_section_and_open_strings_keep_their_wording() {
    let n = locate("ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;\n").expect("found");
    assert_eq!(n.to_string(), "syntax error: no DATA section found");
    let s = locate(&file("#1 = A('open);\n")).expect("found");
    assert!(s.to_string().contains("string starting at line 6 is never closed"), "{s}");
}

/// #3767: since solidcore fa67b3f5 the parser holds a text's parameter count
/// to its own length. A dense, legal record alone in the probe frame passes
/// that density; the locator must not name it. A real fault after it is
/// still named.
#[test]
fn a_dense_legal_record_is_not_named() {
    let deep = format!("#9 = DEEP_LIST({}1.0{});\n", "(".repeat(63), ")".repeat(63));
    let zeros = format!("#8 = INTS(({}0));\n", "0,".repeat(200));
    assert_eq!(locate(&file(&format!("#1 = A(1.0);\n{deep}{zeros}"))), None);
    let f = locate(&file(&format!("{deep}#22 = B(1,,2);\n"))).expect("found");
    assert!(matches!(f, Failure::Entity { id: Some(ref i), .. } if i == "#22"), "{f:?}");
}
