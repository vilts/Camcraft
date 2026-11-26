//! Pins of #1882, the engine seams a waterjet dialect needs: the
//! `lineEnding` manifest entry (CR LF output), the linear axes' travel
//! in `machine.travel`, the cut XY box in `programBounds`, arc
//! linearization (`linearizeArcs` / `chordTolerance`), and the
//! `WATERJET` / `JET` records with the `onJetOn` / `onJetOff` hooks and
//! their law.
//!
//! Gate map (M4.4): covers camcraft_post::post::engine,
//! camcraft_post::post::cnc, camcraft_post::post::machine,
//! camcraft_post::cl.

use camcraft_post::post::cnc::{self, Record};
use camcraft_post::post::{engine, PostError};
use serde_json::{json, Value};

/// A square cut (10 x 10 mm) with one CCW half circle that bulges to
/// Y = -5 below the start, a rapid far outside the box, and one jet
/// span around the cut moves.
const STREAM: &str = "; HSMW-CNC v1
PARAM StartX 0.000000
PARAM StartY 0.000000
PARAM StartZ 0.000000
PARAM OperationName Plate
WATERJET ABRASIVE DWELL500 LAG250
TOOL 1 NOZZLE
SECTION
Rapid X 100000 Y 100000 Z 0
Rapid X 0 Y 0 Z 0
JET ON C0 OUTER
Feed X 0 Y 10000 Z 0 F 1000000
Feed X 10000 Y 10000 Z 0 F 1000000
Feed X 10000 Y 0 Z 0 F 1000000
Arc CW X 0 Y 0 Z 0 F 1000000 I 5000 J 0
JET OFF C0
ENDSECTION
";

const ECHO: &str = r#"
function onOpen() {
  writeln("B " + JSON.stringify(programBounds));
  writeln("T " + JSON.stringify(machine.travel));
}
function onJetOn() { writeln("ON C" + properties.Contour + " " + properties.ContourRole + " A" + properties.WaterjetAbrasive + " D" + properties.PierceDwell + " L" + properties.ValveLagDwell); }
function onJetOff() { writeln("OFF C" + properties.Contour); }
function onRapid() { writeln("G0 " + properties.X + " " + properties.Y); }
function onLinear() { writeln("G1 " + properties.X + " " + properties.Y); }
function onCircular() { writeln("G2 " + properties.X + " " + properties.Y); }
"#;

fn run(cps: &str, mch: &Value, stream: &str) -> Result<engine::PostOutput, PostError> {
    engine::run(cps, mch, stream)
}

#[test]
fn line_ending_crlf_ends_every_line_with_cr_lf() {
    let lf = run(ECHO, &Value::Null, STREAM).expect("lf posts");
    assert!(!lf.program.contains('\r'), "the default is LF:\n{}", lf.program);
    let crlf = run(&format!("lineEnding = \"crlf\";\n{ECHO}"), &Value::Null, STREAM).expect("crlf posts");
    assert_eq!(crlf.program, lf.program.replace('\n', "\r\n"));
    assert!(crlf.program.ends_with("\r\n"));
    let said = run(&format!("lineEnding = \"lf\";\n{ECHO}"), &Value::Null, STREAM).expect("lf said");
    assert_eq!(said.program, lf.program);
    // A script may choose it in onOpen (a declared option): the engine
    // reads it when it writes the program.
    let late = "function onOpen() { lineEnding = \"crlf\"; writeln(\"A\"); writeln(\"B\"); }";
    assert_eq!(run(late, &Value::Null, "").expect("late").program, "A\r\nB\r\n");
}

#[test]
fn line_ending_other_words_are_refused_by_name() {
    for bad in ["\"cr\"", "\"CRLF\"", "1", "true"] {
        match run(&format!("lineEnding = {bad};\nfunction onOpen() {{ writeln(\"X\"); }}"), &Value::Null, "") {
            Err(PostError::Refused(why)) => assert!(why.contains("lineEnding"), "{bad}: {why}"),
            other => panic!("{bad}: expected a refusal, got {other:?}"),
        }
    }
}

#[test]
fn program_bounds_box_the_cut_moves_and_skip_rapids() {
    let out = run(ECHO, &Value::Null, STREAM).expect("posts");
    let line = out.program.lines().find(|l| l.starts_with("B ")).expect("bounds line");
    let b: Value = serde_json::from_str(&line[2..]).expect("bounds json");
    // The rapid to (100, 100) is not cut; the CW half circle from
    // (10, 0) to (0, 0) about (5, 0) passes its quarter point (5, -5).
    assert_eq!(b, json!({ "minX": 0, "maxX": 10, "minY": -5, "maxY": 10 }));
    let none = run(ECHO, &Value::Null, "SECTION\nRapid X 1000 Y 1000 Z 0\nENDSECTION\n").expect("posts");
    assert!(none.program.starts_with("B null\n"), "{}", none.program);
}

#[test]
fn machine_travel_publishes_the_linear_strokes() {
    let mch = json!({
        "name": "jet",
        "kinematics": { "static": { "axes": [
            { "letter": "X", "direction": [1, 0, 0], "travel": [0.0, 3000.0] },
            { "letter": "Y", "direction": [0, 1, 0], "travel": [-10.0, 1500.0] },
            { "letter": "Z", "direction": [0, 0, 1] }
        ], "rotaries": [] } }
    });
    let out = run(ECHO, &mch, STREAM).expect("posts");
    let line = out.program.lines().find(|l| l.starts_with("T ")).expect("travel line");
    let t: Value = serde_json::from_str(&line[2..]).expect("travel json");
    assert_eq!(t, json!({ "X": [0, 3000], "Y": [-10, 1500], "Z": null }));
    let bare = run(ECHO, &Value::Null, STREAM).expect("posts");
    assert!(bare.program.contains("T {\"X\":null,\"Y\":null,\"Z\":null}\n"), "{}", bare.program);
}

#[test]
fn machine_travel_rides_the_mirror_and_the_raw_file() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/haas-vf2.mch"))
        .expect("machine file");
    let travel = |mch: &Value| -> Value {
        let out = run(ECHO, mch, STREAM).expect("posts");
        let line = out.program.lines().find(|l| l.starts_with("T ")).expect("travel line").to_owned();
        serde_json::from_str(&line[2..]).expect("travel json")
    };
    // The vendor tree states its strokes: the raw door reads them.
    let raw: Value = serde_json::from_str(&text).expect("json");
    let stated = travel(&raw);
    assert!(stated["X"].is_array() && stated["Y"].is_array() && stated["Z"].is_array(), "{stated}");
    // The mirror carries them, and a changed stroke rides through.
    let mut m = camcraft_post::machine::parse_mch(&text).expect("parses");
    assert_eq!(travel(&camcraft_post::post::machine::mirror(&m)), stated);
    for a in m.axes.iter_mut().filter(|a| a.name == "X") {
        a.travel = Some((-5.0, 500.0));
    }
    for a in m.axes.iter_mut().filter(|a| a.name == "Y") {
        a.travel = None;
    }
    let t = travel(&camcraft_post::post::machine::mirror(&m));
    assert_eq!(t["X"], json!([-5, 500]), "{t}");
    assert_eq!(t["Y"], Value::Null, "an unstated stroke stays null: {t}");
    assert_eq!(t["Z"], stated["Z"], "{t}");
}

#[test]
fn linearize_arcs_posts_chords_ending_on_the_arc_end() {
    let cps = format!("linearizeArcs = true;\nchordTolerance = 0.05;\n{ECHO}");
    let out = run(&cps, &Value::Null, STREAM).expect("posts");
    assert!(!out.program.contains("G2 "), "{}", out.program);
    let lines: Vec<&str> = out.program.lines().collect();
    let from = lines.iter().position(|l| *l == "G1 10 0").expect("the arc's start");
    let chords: Vec<&str> = lines[from + 1..].iter().take_while(|l| l.starts_with("G1 ")).copied().collect();
    assert!(chords.len() > 4, "a half circle of radius 5 at 0.05 mm sagitta is many chords: {chords:?}");
    assert_eq!(chords.last(), Some(&"G1 0 0"), "the last chord ends on the arc's own end point");
    // Each chord end stands on the circle (radius 5 about (5, 0)).
    for c in &chords {
        let w: Vec<f64> = c[3..].split(' ').map(|v| v.parse().expect("number")).collect();
        let r = ((w[0] - 5.0).powi(2) + w[1].powi(2)).sqrt();
        assert!((r - 5.0).abs() < 1e-9, "{c}: radius {r}");
        assert!(w[1] <= 1e-9, "{c}: a CW arc from (10, 0) to (0, 0) runs below Y 0");
    }
    // The chords stay one record: every chord line maps to the arc.
    let arc_rec = cnc::parse(STREAM).expect("parses").iter().position(|r| matches!(r, Record::Move(m) if m.center.is_some()));
    let mapped = out.line_map.iter().filter(|li| li.record == arc_rec).count();
    assert_eq!(mapped, chords.len());
    // Unstated tolerance: the default, still chords.
    let dflt = run(&format!("linearizeArcs = true;\n{ECHO}"), &Value::Null, STREAM).expect("posts");
    assert!(!dflt.program.contains("G2 ") && dflt.program.ends_with("G1 0 0\nOFF C0\n"), "{}", dflt.program);
}

#[test]
fn linearize_arcs_refuses_a_bad_tolerance_by_name() {
    for bad in ["0", "-1", "\"fine\"", "NaN"] {
        let cps = format!("linearizeArcs = true;\nchordTolerance = {bad};\n{ECHO}");
        match run(&cps, &Value::Null, STREAM) {
            Err(PostError::Refused(why)) => assert!(why.contains("chordTolerance"), "{bad}: {why}"),
            other => panic!("{bad}: expected a refusal, got {other:?}"),
        }
    }
}

#[test]
fn waterjet_records_fire_the_jet_hooks_with_their_properties() {
    let out = run(ECHO, &Value::Null, STREAM).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G0 0 0\nON C0 outer Atrue D0.5 L0.25\nG1 0 10\n"), "{}", out.program);
    assert!(out.program.ends_with("G2 0 0\nOFF C0\n"), "{}", out.program);
    let labels: Vec<&str> = out.blocks.iter().map(|b| b.label.as_str()).filter(|l| l.starts_with("jet")).collect();
    assert_eq!(labels, ["jet on · C0 outer", "jet off · C0"]);
}

#[test]
fn waterjet_records_round_trip_in_both_spellings() {
    let recs = cnc::parse(STREAM).expect("parses");
    assert_eq!(cnc::parse(&cnc::serialize(&recs)).expect("re-parses"), recs);
    let cl = STREAM.replace("WATERJET ", "; WATERJET ").replace("JET ON", "; JET ON").replace("JET OFF", "; JET OFF");
    assert_eq!(cnc::parse(&cl).expect("the .cl spelling parses"), recs);
    let water = cnc::parse("WATERJET\n").expect("a bare header");
    assert_eq!(cnc::serialize(&water), "; HSMW-CNC v1\nWATERJET\n");
    for bad in ["WATERJET DWELL-1\n", "WATERJET KERF5\n", "JET ON C1\n", "JET UP C1\n", "JET OFF 1\n"] {
        assert!(cnc::parse(bad).is_err(), "{bad:?} must be refused");
    }
}

/// The counted warnings of one post (the program is never released).
fn warnings(cps: &str, stream: &str) -> Vec<String> {
    run(cps, &Value::Null, stream).expect("posts with warnings").warnings
}

#[test]
fn waterjet_law_refuses_what_the_stream_cannot_mean() {
    let mill = "function onLinear() { writeln(\"G1\"); }";
    let w = warnings(mill, STREAM);
    assert!(w.iter().any(|w| w.contains("does not define onJetOn / onJetOff") && w.contains("operation Plate")), "{w:?}");
    let cases = [
        (STREAM.replace("WATERJET ABRASIVE DWELL500 LAG250\n", ""), "no WATERJET header"),
        (STREAM.replace("JET OFF C0\n", ""), "never turned off"),
        (STREAM.replace("JET OFF C0\n", "JET OFF C1\n"), "C0 is the open contour"),
        (STREAM.replace("JET OFF C0\n", "Rapid X 0 Y 0 Z 0\nJET OFF C0\n"), "a rapid move while the jet on C0 is on"),
        (STREAM.replace("Feed X 0 Y 10000", "JET ON C1 INNER\nFeed X 0 Y 10000"), "still on"),
        (STREAM.replace("TOOL 1", "WATERJET\nTOOL 1"), "a second WATERJET header"),
    ];
    for (stream, why) in cases {
        let w = warnings(ECHO, &stream);
        assert!(w.iter().any(|w| w.contains(why)), "{why}: {w:?}");
    }
    // The waterjet header does not satisfy the plasma law, nor the
    // reverse: the two records are independent.
    let torch = STREAM.replace("JET ON C0 OUTER", "TORCH ON C0 OUTER").replace("JET OFF C0", "TORCH OFF C0");
    let w = warnings(ECHO, &torch);
    assert!(w.iter().any(|w| w.contains("no PLASMA header")), "{w:?}");
}

#[test]
fn waterjet_header_and_open_jet_end_with_their_section() {
    // A second section with no WATERJET header: its jet is refused, the
    // first section's header does not carry over.
    let second = format!("{STREAM}SECTION\nJET ON C5 OPEN\nFeed X 0 Y 5000 Z 0 F 1000000\nJET OFF C5\nENDSECTION\n");
    let w = warnings(ECHO, &second);
    assert!(w.iter().any(|w| w.contains("JET C5 with no WATERJET header")), "{w:?}");
    // A jet left on is refused at its own ENDSECTION, before the next
    // section starts.
    let left_on = format!("{}SECTION\nRapid X 0 Y 0 Z 0\nENDSECTION\n", STREAM.replace("JET OFF C0\n", ""));
    let out = run(ECHO, &Value::Null, &left_on).expect("posts with warnings");
    assert!(out.warnings.iter().any(|w| w.contains("the jet on C0 is never turned off")), "{:?}", out.warnings);
    assert!(!out.warnings.iter().any(|w| w.contains("a rapid move while the jet")), "{:?}", out.warnings);
}

#[test]
fn waterjet_section_under_a_flipped_wcs_is_refused_by_name() {
    let flipped = STREAM.replace(
        "PARAM OperationName Plate\n",
        "PARAM OperationName Plate\nPARAM OriginX 0.000000\nPARAM OriginY 0.000000\nPARAM WcsFlip X\n",
    );
    match run(&format!("axisInversion = true;\n{ECHO}"), &Value::Null, &flipped) {
        Err(PostError::Refused(why)) => assert!(why.contains("a waterjet section is not supported for a flipped WCS axis"), "{why}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn linearize_full_circles_keeps_both_directions() {
    let cps = format!("linearizeArcs = true;\nchordTolerance = 0.01;\n{ECHO}");
    for direction in ["CW", "CCW"] {
        let stream = format!("PARAM StartX 10\nArc {direction} X 10000 Y 0 Z 0 F 300000 I 0 J 0\n");
        let out = run(&cps, &Value::Null, &stream).expect("full circle posts");
        let points: Vec<(f64, f64)> = out
            .program
            .lines()
            .filter_map(|line| {
                let mut words = line.strip_prefix("G1 ")?.split_whitespace();
                Some((
                    words.next()?.parse().expect("X"),
                    words.next()?.parse().expect("Y"),
                ))
            })
            .collect();
        assert!(points.len() > 20, "{direction}: {points:?}");
        assert_eq!(points.last(), Some(&(10.0, 0.0)));
        let mut previous = (10.0, 0.0);
        let mut length = 0.0;
        for &(x, y) in &points {
            assert!((x.hypot(y) - 10.0).abs() < 1e-9);
            let cross = previous.0 * y - previous.1 * x;
            assert!(
                if direction == "CCW" {
                    cross > 0.0
                } else {
                    cross < 0.0
                },
                "{direction}: {previous:?} -> ({x}, {y})"
            );
            let chord = (x - previous.0).hypot(y - previous.1);
            assert!(10.0 - (100.0 - chord * chord * 0.25).sqrt() <= 0.01);
            length += chord;
            previous = (x, y);
        }
        assert!(
            length > 62.0 && length < std::f64::consts::TAU * 10.0,
            "{direction}: {length}"
        );
    }
}

#[test]
fn linearize_arcs_refuses_a_tolerance_below_the_chord_cap() {
    for direction in ["CW", "CCW"] {
        let cps = format!("linearizeArcs = true;\nchordTolerance = 0.0001;\n{ECHO}");
        let stream =
            format!("PARAM StartX 1000\nArc {direction} X -1000000 Y 0 Z 0 F 300000 I 0 J 0\n");
        match run(&cps, &Value::Null, &stream) {
            Err(PostError::Refused(why)) => {
                assert!(
                    why.contains("arc subdivision limit") && why.contains("chordTolerance 0.0001"),
                    "{why}"
                );
            }
            other => panic!("{direction}: expected a named cap refusal, got {other:?}"),
        }
    }
}
