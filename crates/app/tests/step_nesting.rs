//! #1075: a STEP record nested thousands of parentheses deep is refused by
//! name before ruststep's recursive parser can overflow the stack - on the
//! model import path and on the tool-body coordinate-system read. The deep
//! inputs are built here from box.step, not committed as fixtures.
//!
//! No test here hands ruststep a record deeper than the cap: every deep
//! input is checked by the scan (`step_depth::check`) first, and only a
//! refused one goes on to the importer, which then refuses it before the
//! parser runs. Inputs that do reach ruststep stop at the cap.
//!
//! Gate map: covers app::geometry::import::step_depth, step and step_csys.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use camcraft_app::geometry::import::step_csys::read_tool_csys;
use camcraft_app::geometry::import::step_depth::{check, max_depth, MAX_NESTING};
use camcraft_app::geometry::import::{self, Format};

fn models() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models")
}

/// Where extra text goes into box.step.
#[derive(Clone, Copy, Default)]
struct Extra<'a> {
    /// Appended to the HEADER section (before its ENDSEC).
    header: &'a str,
    /// Put between the HEADER and the DATA section (ANCHOR, REFERENCE).
    pre_data: &'a str,
    /// Replaces `DATA;` when set (e.g. `DATA(...);`).
    data_kw: Option<&'a str>,
    /// Put in the DATA section just before the deep record.
    data: &'a str,
}

/// box.step with `extra` and one record `#999999` nested `depth` deep just
/// before the DATA section's ENDSEC; and the 1-based line it starts on.
fn box_with(extra: Extra, depth: usize, crlf: bool) -> (Vec<u8>, usize) {
    let text = std::fs::read_to_string(models().join("box.step")).expect("box.step reads");
    let text = text.replace("\r\n", "\n");
    let head_end = text.find("ENDSEC;").expect("box.step has a HEADER ENDSEC");
    let data_at = text.find("DATA;").expect("box.step has a DATA section");
    let data_end = text.rfind("ENDSEC;").expect("box.step has a DATA ENDSEC");
    let inner = depth.saturating_sub(1);
    let record = format!(
        "#999999 = DEEP_LIST({}1.0{});\n",
        "(".repeat(inner),
        ")".repeat(inner)
    );
    let out = format!(
        "{}{}{}{}{}{}{}{record}{}",
        &text[..head_end],
        extra.header,
        &text[head_end..data_at],
        extra.pre_data,
        extra.data_kw.unwrap_or("DATA;"),
        &text[data_at + "DATA;".len()..data_end],
        extra.data,
        &text[data_end..],
    );
    let line = out[..out.find("#999999 =").unwrap_or(0)]
        .matches('\n')
        .count()
        + 1;
    let out = if crlf { out.replace('\n', "\r\n") } else { out };
    (out.into_bytes(), line)
}

fn box_with_deep_record(depth: usize, crlf: bool) -> (Vec<u8>, usize) {
    box_with(Extra::default(), depth, crlf)
}

/// The scan's verdict on `data`, which must be a refusal; then (only then,
/// so the parser never sees the deep record) the importer's own message.
fn refused_end_to_end(data: &[u8]) -> String {
    let scan = check(data).err().map(|e| e.to_string());
    let Some(scan) = scan else {
        panic!("the depth scan let a record deeper than {MAX_NESTING} through");
    };
    let msg = match import::parse_model(data, Format::Step) {
        Ok(_) => String::from("<imported>"),
        Err(e) => e.to_string(),
    };
    assert!(msg.contains(&scan), "importer: {msg}; scan: {scan}");
    msg
}

#[test]
fn a_record_nested_5000_deep_is_refused_by_name_quickly() {
    let (data, line) = box_with_deep_record(5000, false);
    let start = Instant::now();
    let msg = refused_end_to_end(&data);
    let took = start.elapsed();
    assert!(msg.contains(&format!("deeper than {MAX_NESTING}")), "{msg}");
    assert!(msg.contains("entity #999999"), "{msg}");
    assert!(msg.contains(&format!("line {line}")), "{msg}");
    assert!(took < Duration::from_secs(1), "refusal took {took:?}");
}

#[test]
fn crlf_and_lf_files_name_the_same_line() {
    let (lf, line) = box_with_deep_record(5000, false);
    let (crlf, _) = box_with_deep_record(5000, true);
    let a = refused_end_to_end(&lf);
    assert!(a.contains(&format!("line {line}")), "{a}");
    assert_eq!(a, refused_end_to_end(&crlf));
}

#[test]
fn a_record_at_the_cap_still_imports() {
    let (data, _) = box_with_deep_record(MAX_NESTING, false);
    assert_eq!(max_depth(&data), MAX_NESTING);
    let mesh = import::parse_model(&data, Format::Step).expect("a record at the cap imports");
    let plain = import::load_model(&models().join("box.step")).expect("box.step imports");
    assert_eq!(
        mesh.facets.len(),
        plain.facets.len(),
        "the dead record must not change the mesh"
    );
    let (over, _) = box_with_deep_record(MAX_NESTING + 1, false);
    assert!(
        refused_end_to_end(&over).contains("deeper than"),
        "one past the cap is refused"
    );
}

#[test]
fn the_tool_body_csys_read_is_guarded_too() {
    let (data, line) = box_with_deep_record(5000, false);
    assert!(
        check(&data).is_err(),
        "the scan must refuse before read_tool_csys parses"
    );
    let msg = read_tool_csys(&data)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(
        msg.contains("entity #999999") && msg.contains(&format!("line {line}")),
        "{msg}"
    );
}

/// The PR #1079 review's reproductions: a `<...>` resource holding a `/*`
/// or a `'` must not hide the deep record after it (at depth 65 - a scan
/// regression here cannot overflow the parser, which survives ~400).
#[test]
fn a_reference_resource_cannot_hide_a_deep_record() {
    for uri in ["<x/*y>", "<x'y>"] {
        let pre = format!("REFERENCE;\n#900000 = {uri};\nENDSEC;\n");
        let extra = Extra {
            pre_data: &pre,
            ..Extra::default()
        };
        let (data, line) = box_with(extra, MAX_NESTING + 1, false);
        let msg = refused_end_to_end(&data);
        assert!(
            msg.contains("entity #999999") && msg.contains(&format!("line {line}")),
            "{uri}: {msg}"
        );
    }
}

/// Every construct of the module table in `step_depth.rs`, with a stray
/// `'`, `/*`, `<` or `"` inside where it can hold one, followed by a
/// record one past the cap: the scan must refuse it, naming that record.
#[test]
fn no_lexical_construct_hides_a_deep_record() {
    let none = Extra::default();
    let cases: &[(&str, Extra)] = &[
        (
            "string holding /* < \"",
            Extra {
                data: "#5 = A('x/*y<z\"w');\n",
                ..none
            },
        ),
        (
            "string with '' then /*",
            Extra {
                data: "#5 = A('it''s /* no comment');\n",
                ..none
            },
        ),
        (
            "empty string then '",
            Extra {
                data: "#5 = A('', '''', 'a');\n",
                ..none
            },
        ),
        (
            "string ending in a backslash",
            Extra {
                data: "#5 = A('a\\');\n",
                ..none
            },
        ),
        (
            "comment holding ' < \"",
            Extra {
                data: "/* x'y<z\"w */\n",
                ..none
            },
        ),
        (
            "comment /*/ ... */",
            Extra {
                data: "/*/ ' */\n",
                ..none
            },
        ),
        (
            "comment /**/",
            Extra {
                data: "/**/ #5 = A(1);\n",
                ..none
            },
        ),
        (
            "comment closed by */*",
            Extra {
                data: "#5 = A(*/* c */);\n",
                ..none
            },
        ),
        (
            "resource holding \"",
            Extra {
                pre_data: "REFERENCE;\n#900001 = <a\"b(>;\nENDSEC;\n",
                ..none
            },
        ),
        (
            "anchor name and tag",
            Extra {
                pre_data: "ANCHOR;\n<a'b/*> = (1, (2, 'c')) {t:<u'v>};\nENDSEC;\n",
                ..none
            },
        ),
        (
            "binary literal holding ' /* <",
            Extra {
                data: "#5 = A(\"0'1/*<\");\n",
                ..none
            },
        ),
        (
            "enumeration, user keyword, names",
            Extra {
                data: "#5 = !USER(.T., .F2., *, $, @3, #4, 1.E-3);\n",
                ..none
            },
        ),
        (
            "complex entity",
            Extra {
                data: "#5 = ( A('(') B((1)) );\n",
                ..none
            },
        ),
        (
            "HEADER record holding ' and /*",
            Extra {
                header: "FILE_X('a''/*', ('<'));\n",
                ..none
            },
        ),
        (
            "DATA section parameters",
            Extra {
                data_kw: Some("DATA(('x/*<'), 'y''');"),
                ..none
            },
        ),
    ];
    for (name, extra) in cases {
        let (data, line) = box_with(*extra, MAX_NESTING + 1, false);
        let msg = check(&data)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            msg.contains("entity #999999") && msg.contains(&format!("line {line}")),
            "{name}: the scan did not refuse the deep record ({msg:?})"
        );
    }
}

/// Parentheses inside a string, a comment or a `<...>` resource are text:
/// a valid file full of them still imports (through the real parser).
#[test]
fn parentheses_inside_text_are_not_counted() {
    let many = "(".repeat(200);
    let pre = format!("REFERENCE;\n#900000 = <x{many}/*'y>;\nENDSEC;\n");
    let data =
        format!("/* {many} ' */\n#999998 = DESCRIPTIVE_REPRESENTATION_ITEM('n', '{many}''');\n");
    let extra = Extra {
        pre_data: &pre,
        data: &data,
        ..Extra::default()
    };
    let (bytes, _) = box_with(extra, MAX_NESTING, false);
    assert_eq!(max_depth(&bytes), MAX_NESTING);
    let mesh = import::parse_model(&bytes, Format::Step).expect("text parentheses are not nesting");
    let plain = import::load_model(&models().join("box.step")).expect("box.step imports");
    assert_eq!(mesh.facets.len(), plain.facets.len());
}

/// Every committed STEP fixture nests far below the cap (they reach 3).
#[test]
fn every_step_fixture_is_under_the_cap() {
    let mut seen = 0;
    for entry in std::fs::read_dir(models()).expect("testdata/models lists") {
        let path = entry.expect("dir entry").path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        if !matches!(ext.as_deref(), Some("step" | "stp")) {
            continue;
        }
        let depth = max_depth(&std::fs::read(&path).expect("fixture reads"));
        println!("{}: max depth {depth}", path.display());
        assert!(
            depth <= 8,
            "{} nests {depth} deep - re-check the cap's margin",
            path.display()
        );
        assert!(depth < MAX_NESTING);
        seen += 1;
    }
    assert!(seen >= 5, "found only {seen} STEP fixtures");
}
