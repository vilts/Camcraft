//! #931 slice 1: work offsets ride the stream as a 1-based index and the
//! post writes its own word. A script that declares `workOffsetIndex`
//! gets the number and states its maximum; one that does not gets the
//! legacy word for offsets 1-6 and is refused by setup name above 6.
//! Every shipped script is run on recorded streams for offsets 1, 6, 7,
//! its declared maximum and one past it.
//!
//! Gate map (#931): covers camcraft_post::post::work_offset, the engine's
//! WorkOffset translation, and assets/posts (every script's
//! workOffsetName / workOffsetMax).

use camcraft_post::cl::MoveKind;
use camcraft_post::post::cnc::{self, MoveRec, Record};
use camcraft_post::post::cnc_torch::PlasmaRec;
use camcraft_post::post::work_offset::{self, WorkOffsets};
use camcraft_post::post::{dump, engine, Dump};
use serde_json::{json, Value};

macro_rules! post {
    ($f:literal) => {
        ($f, include_str!(concat!("../../../assets/posts/", $f)))
    };
}

/// Every shipped script.
const SCRIPTS: [(&str, &str); 13] = [
    post!("carvera.cps"),
    post!("doosan-dnm.cps"),
    post!("fanuc-lathe.cps"),
    post!("fanuc.cps"),
    post!("haas.cps"),
    post!("heidenhain.cps"),
    post!("mazak.cps"),
    post!("sinumerik.cps"),
    post!("tormach-lathe.cps"),
    post!("tormach-plasma.cps"),
    post!("tormach.cps"),
    post!("grblhal.cps"),
    post!("tinyg.cps"),
];

/// What each dialect writes for offset `n`, and its declared maximum.
fn expected(file: &str, n: u32) -> String {
    match file {
        "heidenhain.cps" => format!("Q339={n}"),
        "sinumerik.cps" if n > 4 => format!("G{}", 500 + n),
        "tormach.cps" | "tormach-lathe.cps" if n > 6 => format!("G59.{}", n - 6),
        _ => format!("G{}", 53 + n),
    }
}

fn max_of(file: &str) -> u32 {
    match file {
        "tormach.cps" | "tormach-lathe.cps" => 9,
        _ => 6,
    }
}

fn param(name: &str, value: Value) -> Record {
    Record::Param { name: name.into(), value }
}

fn rapid(x: f64, z: f64) -> Record {
    Record::Move(MoveRec { kind: MoveKind::Rapid, x, y: 0.0, z, feed: None, center: None, ccw: false, plane: Default::default(), abc: None })
}

/// One section of setup `Fixture` at work offset `wcs`, as stream text.
fn stream(wcs: Value) -> String {
    section(wcs, None)
}

/// The same with a plasma header (the plasma post cuts torch work only).
fn section(wcs: Value, plasma: Option<Record>) -> String {
    let mut rs = vec![
        param("OperationName", json!("Face1")),
        param("SetupName", json!("Fixture")),
        param("SpindleSpeed", json!(2000.0)),
        param("ClearancePlane", json!(10.0)), // #1614: a section states its clearance
        param("WorkOffset", wcs),
        Record::Tool { number: 1, name: "T1".into() },
    ];
    rs.extend(plasma);
    rs.extend([Record::Section { begin: true }, rapid(0.0, 5.0), rapid(1.0, 5.0), Record::Section { begin: false }]);
    cnc::serialize(&rs)
}

/// Post one section through a shipped script - the plasma post on its
/// own 1300PL machine with a plasma header, every other on no machine.
fn run(file: &str, cps: &str, wcs: Value) -> engine::PostOutput {
    let out = if file == "tormach-plasma.cps" {
        let mch: Value = serde_json::from_str(include_str!("../../../assets/machines/tormach-1300pl.mch")).expect("mch");
        let header = Record::Plasma(PlasmaRec {
            kerf: 1.5,
            control_comp: false,
            pierce_height: None,
            cut_height: None,
            pierce_delay: None,
            auto_feed: true,
            reversed: false,
        });
        engine::run(cps, &mch, &section(wcs.clone(), Some(header)))
    } else {
        engine::run(cps, &Value::Null, &stream(wcs.clone()))
    };
    out.unwrap_or_else(|e| panic!("{file} offset {wcs}: {e}"))
}

fn offset_warnings(w: &[String]) -> Vec<&String> {
    w.iter().filter(|w| w.contains("work offset")).collect()
}

fn has_word(program: &str, word: &str) -> bool {
    // a Heidenhain block ends in `*`
    program.lines().any(|l| l.split_whitespace().any(|w| w.trim_end_matches('*') == word))
}

#[test]
fn every_shipped_script_declares_the_index_flag_and_its_names() {
    for (file, cps) in SCRIPTS {
        let wo = work_offset::work_offsets(cps).unwrap_or_else(|e| panic!("{file}: {e}"));
        let max = max_of(file);
        assert!(wo.indexed, "{file} declares workOffsetIndex");
        assert_eq!(wo.max, max, "{file}'s workOffsetMax");
        let names: Vec<String> = (1..=max).map(|n| {
            // the display name is the word the program writes; heidenhain's
            // is the preset row it selects
            if file == "heidenhain.cps" { format!("preset {n}") } else { expected(file, n) }
        }).collect();
        assert_eq!(wo.names, names, "{file}'s workOffsetName");
    }
}

#[test]
fn every_shipped_script_posts_offsets_up_to_its_max_and_refuses_one_past() {
    for (file, cps) in SCRIPTS {
        let max = max_of(file);
        let mut ns = vec![1, 6, 7, max, max + 1];
        ns.dedup();
        for n in ns {
            let out = run(file, cps, json!(n));
            let refusals = offset_warnings(&out.warnings);
            if n <= max {
                assert!(refusals.is_empty(), "{file} offset {n}: {refusals:?}");
                let word = expected(file, n);
                assert!(
                    has_word(&out.program, &word),
                    "{file} offset {n} writes {word}:\n{}",
                    out.program
                );
            } else {
                assert!(
                    refusals.iter().any(|w| w.contains("setup Fixture") && w.contains(&format!("work offset {n}")) && w.contains(&format!("1-{max}"))),
                    "{file} refuses offset {n} by setup name: {:?}",
                    out.warnings
                );
            }
        }
    }
}

#[test]
fn a_legacy_word_in_the_stream_reads_as_its_index() {
    // Streams written before #931 carried the word; it still posts the
    // same program as the number it names.
    for (file, cps) in SCRIPTS {
        let by_word = run(file, cps, json!("G59"));
        let by_index = run(file, cps, json!(6));
        assert_eq!(by_word.program, by_index.program, "{file}");
    }
}

#[test]
fn a_value_that_names_no_offset_is_refused_by_setup_name() {
    let (_, cps) = SCRIPTS[3]; // fanuc
    for bad in [json!("G53"), json!(0), json!(-1), json!(2.5), json!("G54.1 P3")] {
        let out = engine::run(cps, &Value::Null, &stream(bad.clone())).expect("engine runs");
        assert!(
            out.warnings.iter().any(|w| w.starts_with("setup Fixture: work offset") && w.contains("not a work offset number")),
            "{bad}: {:?}",
            out.warnings
        );
    }
}

// A user post that predates #931: no flag, reads the word verbatim.
const UNFLAGGED: &str = r#"
var wcs = "?";
function onParameter(name, value) { if (name === "WorkOffset") wcs = String(value); }
function onSection() { writeBlock("G90", wcs); writeBlock("PROP", String(properties.WorkOffset)); }
"#;

#[test]
fn an_unflagged_script_gets_the_word_for_one_to_six_and_is_refused_above() {
    for n in 1..=6u32 {
        let out = engine::run(UNFLAGGED, &Value::Null, &stream(json!(n))).expect("posts");
        assert!(out.warnings.is_empty(), "{n}: {:?}", out.warnings);
        let word = format!("G{}", 53 + n);
        assert!(out.program.contains(&format!("G90 {word}\nPROP {word}\n")), "{n}: {}", out.program);
    }
    let out = engine::run(UNFLAGGED, &Value::Null, &stream(json!(7))).expect("engine runs");
    assert!(
        out.warnings.iter().any(|w| w.starts_with("setup Fixture: uses work offset 7") && w.contains("workOffsetIndex")),
        "{:?}",
        out.warnings
    );
    assert!(out.program.contains("G90 ?"), "the script never saw offset 7: {}", out.program);
    // The display table of an unflagged script is the six words it reads.
    let wo = work_offset::work_offsets(UNFLAGGED).expect("loads");
    assert_eq!(
        wo,
        WorkOffsets { indexed: false, max: 6, names: ["G54", "G55", "G56", "G57", "G58", "G59"].map(String::from).to_vec() }
    );
}

#[test]
fn an_unflagged_script_cannot_raise_its_maximum() {
    let cps = format!("workOffsetMax = 20;\n{UNFLAGGED}");
    assert_eq!(work_offset::work_offsets(&cps).expect("loads").max, 6);
    let out = engine::run(&cps, &Value::Null, &stream(json!(7))).expect("engine runs");
    assert!(out.warnings.iter().any(|w| w.contains("uses work offset 7")), "{:?}", out.warnings);
}

#[test]
fn a_flagged_script_without_a_max_or_names_gets_six_unnamed_offsets() {
    let cps = format!("workOffsetIndex = true;\n{UNFLAGGED}");
    let out = engine::run(&cps, &Value::Null, &stream(json!(4))).expect("posts");
    assert!(out.program.contains("G90 4\nPROP 4\n"), "the number, not a word: {}", out.program);
    let wo = work_offset::work_offsets(&cps).expect("loads");
    assert_eq!((wo.indexed, wo.max, wo.names.len()), (true, 6, 6));
    assert!(wo.names.iter().all(String::is_empty), "{wo:?}");
    let out = engine::run(&cps, &Value::Null, &stream(json!(7))).expect("engine runs");
    assert!(out.warnings.iter().any(|w| w.contains("work offsets 1-6 only")), "{:?}", out.warnings);
}

#[test]
fn the_dump_carries_the_index() {
    let tp = camcraft_post::cl::Toolpath::default();
    for n in [1u32, 2, 6, 7, 9] {
        let recs = dump(&tp, &Dump { wcs: n, ..Dump::default() });
        let v = recs.iter().find_map(|r| match r {
            Record::Param { name, value } if name == "WorkOffset" => Some(value.clone()),
            _ => None,
        });
        assert_eq!(v, Some(json!(f64::from(n))), "{n}");
        assert!(cnc::serialize(&recs).contains(&format!("PARAM WorkOffset {n}.000000\n")), "{n}");
    }
}

#[test]
fn a_recorded_toolpath_posts_under_each_offset() {
    // A recorded generator stream (app's face golden) through Fanuc and
    // Tormach, at offsets 1, 6, 7 and one past Tormach's maximum.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/tests/golden/face_square_40x40.cl");
    let text = std::fs::read_to_string(path).expect("golden");
    let moves: Vec<Record> = cnc::parse(&text).expect("parses").into_iter().filter(|r| matches!(r, Record::Move(_))).collect();
    assert!(!moves.is_empty());
    let run = |cps: &str, n: u32| {
        let mut rs = vec![
            param("SetupName", json!("Fixture")),
            param("SpindleSpeed", json!(6000.0)),
            param("ClearancePlane", json!(10.0)), // #1614: a section states its clearance
            param("WorkOffset", json!(n)),
            // #1633: a section states its clearance, as `post::dump` does.
            param("ClearancePlane", json!(10.0)),
            Record::Tool { number: 1, name: "EM6".into() },
            Record::Section { begin: true },
        ];
        rs.extend(moves.iter().cloned());
        rs.push(Record::Section { begin: false });
        engine::run(cps, &Value::Null, &cnc::serialize(&rs)).expect("engine runs")
    };
    let (_, fanuc) = SCRIPTS[3];
    let (_, tormach) = SCRIPTS[10];
    for (cps, n, word) in [(fanuc, 1, Some("G54")), (fanuc, 6, Some("G59")), (fanuc, 7, None), (tormach, 7, Some("G59.1")), (tormach, 9, Some("G59.3")), (tormach, 10, None)] {
        let out = run(cps, n);
        match word {
            Some(w) => {
                assert!(out.warnings.is_empty(), "{n}: {:?}", out.warnings);
                assert!(has_word(&out.program, w), "{n} writes {w}: {}", out.program);
            }
            None => assert!(out.warnings.iter().any(|w| w.contains(&format!("setup Fixture: uses work offset {n}"))), "{n}: {:?}", out.warnings),
        }
    }
}
