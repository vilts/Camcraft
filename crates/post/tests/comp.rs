//! The `COMP` record (M13.11): its two directions in the intermediate,
//! the driver's `onRadiusCompensation` contract, the refusal of a script
//! that lacks the hook, and the Fanuc script's own laws - the comp word
//! rides the next straight block with the tool's D register, lands on
//! nothing else, and never outlives its section - and the same laws in
//! the Haas, Mazak and Siemens 840D words (#39).
//!
//! Gate map (M4.4): covers camcraft_post::cl (CompSide), ::post (cnc,
//! engine).

use camcraft_post::cl::CompSide;
use camcraft_post::post::cnc::{self, Record};
use camcraft_post::post::engine;
use serde_json::Value;

fn post(stem: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../assets/posts/{stem}.cps"));
    std::fs::read_to_string(path).expect("post script")
}

fn fanuc() -> String {
    post("fanuc")
}

#[test]
fn comp_records_round_trip_and_malformed_ones_refuse() {
    let recs = vec![
        Record::Comp { side: Some(CompSide::Left), radius_mm: 3.0, wear: false },
        Record::Comp { side: Some(CompSide::Right), radius_mm: 2.5, wear: true },
        Record::Comp { side: None, radius_mm: 0.0, wear: false },
    ];
    let text = cnc::serialize(&recs);
    assert!(text.contains("COMP LEFT 3000\n") && text.contains("COMP RIGHT 2500 WEAR\n") && text.contains("COMP OFF\n"), "{text}");
    assert_eq!(cnc::parse(&text).unwrap(), recs);
    for bad in [
        "COMP LEFT\n",
        "COMP UP 3000\n",
        "COMP LEFT 0\n",
        "COMP RIGHT -3000\n",
        "COMP LEFT x\n",
        "COMP LEFT 3000 FULL\n",
        "COMP OFF WEAR\n",
    ] {
        assert!(cnc::parse(bad).is_err(), "{bad:?} must refuse");
    }
}

const STREAM: &str = "; HSMW-CNC v1
PARAM OperationName C1
PARAM SpindleSpeed 8000
PARAM ClearancePlane 5
TOOL 2 EM6
SECTION
Rapid X -8000 Y 0 Z 5000
Plunge X -8000 Y 0 Z -2000 F 100000
COMP RIGHT 3000
Feed X 0 Y 0 Z -2000 F 500000
Feed X 40000 Y 0 Z -2000 F 500000
Feed X 40000 Y 40000 Z -2000 F 500000
Feed X 0 Y 40000 Z -2000 F 500000
Feed X 0 Y 0 Z -2000 F 500000
COMP OFF
Feed X 8000 Y 0 Z -2000 F 500000
Rapid X 8000 Y 0 Z 5000
ENDSECTION
";

#[test]
fn the_driver_voices_every_comp_record_with_its_side_and_radius() {
    let cps = r#"
function onRadiusCompensation(side) { writeln("COMP " + side + " " + properties.CompensationRadius + " " + properties.Compensation); }
function onLinear() { writeln("L"); }
function onRapid() { writeln("R"); }
"#;
    let out = engine::run(cps, &Value::Null, STREAM).unwrap();
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("COMP right 3 right\nL\n"), "{}", out.program);
    assert!(out.program.contains("COMP off 0 off\nL\nR\n"), "{}", out.program);
}

#[test]
fn a_script_without_the_hook_is_refused_the_moment_a_comp_record_arrives() {
    let cps = "function onLinear() { writeln(\"L\"); }\nfunction onRapid() { writeln(\"R\"); }\n";
    let out = engine::run(cps, &Value::Null, STREAM).unwrap();
    assert!(
        out.warnings.iter().any(|w| w.contains("does not define onRadiusCompensation")),
        "{:?}",
        out.warnings
    );
    // The same script posts a comp-free stream clean.
    let plain: String = STREAM.lines().filter(|l| !l.starts_with("COMP")).map(|l| format!("{l}\n")).collect();
    assert!(engine::run(cps, &Value::Null, &plain).unwrap().warnings.is_empty());
}

#[test]
fn fanuc_writes_the_words_on_the_leads_with_the_tool_register() {
    let out = engine::run(&fanuc(), &Value::Null, STREAM).unwrap();
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    assert!(lines.contains(&"G01 G42 X0. Y0. Z-2. D02 F500"), "{}", out.program);
    assert!(lines.contains(&"G01 G40 X8. Y0. Z-2. F500"), "{}", out.program);
    assert_eq!(out.program.matches("G42").count(), 1);
    assert_eq!(out.program.matches("G40").count(), 2, "the preamble's and the cancel's: {}", out.program);
    // The contour between is written as given - the part, not the centre.
    assert!(lines.contains(&"G01 X40. Y40. Z-2. F500"), "{}", out.program);
}

#[test]
fn fanuc_refuses_a_comp_word_that_would_land_on_an_arc_a_rapid_or_the_section_end() {
    let on_arc = STREAM.replace(
        "COMP RIGHT 3000\nFeed X 0 Y 0 Z -2000 F 500000\n",
        "COMP RIGHT 3000\nArc CW X 0 Y 0 Z -2000 F 500000 I -4000 J 4000\n",
    );
    let out = engine::run(&fanuc(), &Value::Null, &on_arc).unwrap();
    assert!(out.warnings.iter().any(|w| w.contains("straight G01 move, not an arc")), "{:?}", out.warnings);
    let on_rapid = STREAM.replace("COMP OFF\nFeed X 8000", "COMP OFF\nRapid X 8000");
    let out = engine::run(&fanuc(), &Value::Null, &on_rapid).unwrap();
    assert!(out.warnings.iter().any(|w| w.contains("not a rapid")), "{:?}", out.warnings);
    let never_off = STREAM.replace("COMP OFF\n", "");
    let out = engine::run(&fanuc(), &Value::Null, &never_off).unwrap();
    assert!(out.warnings.iter().any(|w| w.contains("still engaged at section end")), "{:?}", out.warnings);
}

/// (post stem, the engage block, the cancel block): the stream's lead
/// (X0 Y0) and lead-out (X8 Y0) in each dialect's own words. 840D's
/// engage carries no D (the tool call's `T2 D2` selected the edge) and
/// states the NORM approach.
const DIALECTS: &[(&str, &str, &str)] = &[
    ("haas", "G1 G42 X0. Y0. Z-2. D2 F500", "G1 G40 X8. Y0. Z-2. F500"),
    ("mazak", "G01 G42 X0. Y0. Z-2. D2 F500", "G01 G40 X8. Y0. Z-2. F500"),
    ("sinumerik", "G1 G42 NORM X0 Y0 Z-2 F500", "G1 G40 X8 Y0 Z-2 F500"),
];

#[test]
fn haas_mazak_and_840d_write_their_words_on_the_leads() {
    for (stem, engage, cancel) in DIALECTS {
        let out = engine::run(&post(stem), &Value::Null, STREAM).unwrap();
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        // 840D numbers its blocks: compare the block after its N word.
        let bodies: Vec<&str> =
            out.program.lines().map(|l| if l.starts_with('N') { l.split_once(' ').map_or(l, |(_, b)| b) } else { l }).collect();
        assert!(bodies.contains(engage), "{stem}: {}", out.program);
        assert!(bodies.contains(cancel), "{stem}: {}", out.program);
        assert_eq!(out.program.matches("G42").count(), 1, "{stem}");
        assert_eq!(out.program.matches("G40").count(), 2, "{stem}: the preamble's and the cancel's: {}", out.program);
        let left = engine::run(&post(stem), &Value::Null, &STREAM.replace("COMP RIGHT", "COMP LEFT")).unwrap();
        assert!(left.program.contains(&engage.replace("G42", "G41")) && !left.program.contains("G42"), "{stem}: {}", left.program);
    }
}

#[test]
fn haas_mazak_and_840d_refuse_a_comp_word_on_an_arc_a_rapid_or_the_section_end() {
    let on_arc = STREAM.replace(
        "COMP RIGHT 3000
Feed X 0 Y 0 Z -2000 F 500000
",
        "COMP RIGHT 3000
Arc CW X 0 Y 0 Z -2000 F 500000 I -4000 J 4000
",
    );
    let on_rapid = STREAM.replace("COMP OFF
Feed X 8000", "COMP OFF
Rapid X 8000");
    let never_off = STREAM.replace("COMP OFF
", "");
    for (stem, _, _) in DIALECTS {
        for (stream, why) in [(&on_arc, "not an arc"), (&on_rapid, "not a rapid"), (&never_off, "still engaged at section end")] {
            let out = engine::run(&post(stem), &Value::Null, stream).unwrap();
            assert!(out.warnings.iter().any(|w| w.contains(why)), "{stem} / {why}: {:?}", out.warnings);
        }
    }
}

#[test]
fn every_wear_post_notes_the_register_once_and_control_writes_no_note() {
    for (stem, _, _) in DIALECTS {
        let out = engine::run(&post(stem), &Value::Null, STREAM).unwrap();
        assert!(!out.program.contains("WEAR COMP"), "{stem}: {}", out.program);
    }
    let wear = STREAM.replace("COMP RIGHT 3000
", "COMP RIGHT 3000 WEAR
");
    for (stem, note) in [("haas", "(WEAR COMP - D2 "), ("mazak", "(WEAR COMP - D2 "), ("sinumerik", "; WEAR COMP - D2 ")] {
        let out = engine::run(&post(stem), &Value::Null, &wear).unwrap();
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        assert_eq!(out.program.matches(note).count(), 1, "{stem}: {}", out.program);
    }
}

#[test]
fn every_other_shipped_post_refuses_control_compensation_by_name() {
    for stem in ["heidenhain", "fanuc-lathe", "tormach", "grblhal", "tinyg"] {
        let cps = post(stem);
        let out = engine::run(&cps, &Value::Null, STREAM).unwrap_or_else(|e| panic!("{stem}: {e}"));
        assert!(
            out.warnings.iter().any(|w| w.contains("compensation") && w.contains("not supported")),
            "{stem} must refuse by name: {:?}",
            out.warnings
        );
    }
}
