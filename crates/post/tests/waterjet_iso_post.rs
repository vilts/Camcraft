//! Pins of #1882, the generic ISO abrasive-waterjet post
//! (`assets/posts/waterjet-iso.cps`): its program for a recorded plate
//! (one hole, one outer contour) as goldens, in mm with G2/G3 and in
//! inch with G1 chords and CR LF, and its refusals by name - spindle
//! words, a section that is not a waterjet cut, a machine that is not a
//! waterjet, a cut outside the stated travel, a Z cut, a canned cycle.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test waterjet_iso_post`
//!
//! Gate map (M4.4): covers assets/posts/waterjet-iso.cps,
//! camcraft_post::post::engine, camcraft_post::post::cnc.

use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn here() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/waterjet")
}

fn cps() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts/waterjet-iso.cps")).expect("the waterjet post")
}

fn plate() -> String {
    std::fs::read_to_string(here().join("plate.cnc")).expect("the plate stream").replace("\r\n", "\n")
}

fn jet_machine() -> Value {
    json!({ "name": "Test waterjet", "kind": "waterjet" })
}

fn post(mch: &Value, stream: &str, options: &Value) -> engine::PostOutput {
    engine::run_with(&cps(), mch, stream, options).expect("the post runs")
}

fn golden(name: &str, text: &str) {
    let path = here().join(name);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&path, text).expect("write the golden");
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read golden {}: {e}; create it with WRITE_GOLDEN=1", path.display()));
    assert_eq!(want.replace("\r\n", "\n"), text, "golden {} is stale; regenerate with WRITE_GOLDEN=1", path.display());
}

#[test]
fn waterjet_plate_posts_the_golden() {
    let out = post(&jet_machine(), &plate(), &json!({}));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(!out.program.contains('\r'));
    golden("plate.nc", &out.program);
}

#[test]
fn waterjet_plate_inch_chords_crlf_posts_the_golden() {
    let options = json!({
        "Units": "inch", "arcs": "linear", "lineEnding": "crlf", "dwellWord": "p-milliseconds",
        "waterOn": 64, "waterOff": 65, "abrasiveOn": 66, "abrasiveOff": 67,
    });
    let out = post(&jet_machine(), &plate(), &options);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.ends_with("\r\n") && out.program.matches('\n').count() == out.program.matches("\r\n").count());
    let lf = out.program.replace("\r\n", "\n");
    assert!(!lf.lines().any(|l| l.starts_with("G2 ") || l.starts_with("G3 ")), "no circular word under arcs = linear:\n{lf}");
    golden("plate_inch_chords.nc", &lf);
}

/// The counted warnings of one post.
fn refusals(mch: &Value, stream: &str) -> Vec<String> {
    post(mch, stream, &json!({})).warnings
}

fn assert_refused(w: &[String], why: &str) {
    assert!(w.iter().any(|w| w.contains(why)), "expected a refusal with {why:?}, got {w:?}");
}

#[test]
fn waterjet_post_refuses_spindle_words_by_name() {
    let rpm = plate().replace("PARAM OriginZ", "PARAM SpindleSpeed 1200.000000\nPARAM OriginZ");
    assert_refused(&refusals(&jet_machine(), &rpm), "operation Plate 100x60: states 1200 rpm - a waterjet turns no spindle (no S word, no G95");
    let css = plate().replace("PARAM OriginZ", "PARAM SurfaceSpeed 200.000000\nPARAM OriginZ");
    assert_refused(&refusals(&jet_machine(), &css), "constant surface speed - a waterjet turns no spindle (no G96)");
}

#[test]
fn waterjet_post_refuses_a_section_that_is_not_a_waterjet_cut() {
    let mill = plate()
        .replace("WATERJET ABRASIVE DWELL800 LAG300\n", "")
        .replace("JET ON C0 INNER\n", "")
        .replace("JET OFF C0\n", "")
        .replace("JET ON C1 OUTER\n", "")
        .replace("JET OFF C1\n", "")
        .replace("Waterjet Profile", "2D Contour");
    assert_refused(&refusals(&jet_machine(), &mill), "a 2D Contour operation is not waterjet cutting");
    let torch = plate().replace("WATERJET ABRASIVE DWELL800 LAG300", "PLASMA KERF1000");
    let w = refusals(&jet_machine(), &torch);
    assert_refused(&w, "does not define onTorchOn / onTorchOff");
    assert_refused(&w, "no WATERJET header");
}

#[test]
fn waterjet_post_refuses_a_machine_that_is_not_a_waterjet() {
    assert_refused(&refusals(&Value::Null, &plate()), "no machine is stated, so none is declared a waterjet machine");
    let mill = json!({ "name": "VMC", "kind": "mill" });
    assert_refused(&refusals(&mill, &plate()), "VMC is not declared a waterjet machine");
}

#[test]
fn waterjet_post_refuses_a_cut_outside_the_stated_travel() {
    let small = json!({
        "name": "Small jet", "kind": "waterjet",
        "kinematics": { "static": { "axes": [
            { "letter": "X", "direction": [1, 0, 0], "travel": [-10.0, 90.0] },
            { "letter": "Y", "direction": [0, 1, 0], "travel": [-10.0, 500.0] }
        ], "rotaries": [] } }
    });
    let w = refusals(&small, &plate());
    assert_refused(&w, "the cut runs X -5.5 to 100.5 - outside the machine's stated X travel -10 to 90");
    assert!(!w.iter().any(|w| w.contains("Y travel")), "{w:?}");
    let big = json!({
        "name": "Big jet", "kind": "waterjet",
        "kinematics": { "static": { "axes": [
            { "letter": "X", "direction": [1, 0, 0], "travel": [-10.0, 3000.0] },
            { "letter": "Y", "direction": [0, 1, 0], "travel": [-10.0, 1500.0] }
        ], "rotaries": [] } }
    });
    assert!(refusals(&big, &plate()).is_empty());
}

#[test]
fn waterjet_post_refuses_a_z_cut_and_a_cycle() {
    let z = plate().replace("Feed X 100500 Y 60500 Z 0", "Feed X 100500 Y 60500 Z -1000");
    assert_refused(&refusals(&jet_machine(), &z), "a cutting move changes Z - a waterjet program cuts in X and Y only");
    let dry = plate().replace("JET ON C1 OUTER\n", "").replace("JET OFF C1\n", "");
    assert_refused(&refusals(&jet_machine(), &dry), "a cutting move with the jet off");
    let cycle = plate().replace("Rapid X 0 Y 0 Z 0\n", "Rapid X 0 Y 0 Z 0\nCYCLE DRILL X 0 Y 0 Z -5000 R 2000\n");
    assert_refused(&refusals(&jet_machine(), &cycle), "a canned cycle - a waterjet has no drilling cycle");
}

#[test]
fn waterjet_post_refuses_a_helical_arc_in_both_arc_modes() {
    let machine = json!({
        "name": "Large jet", "kind": "waterjet",
        "kinematics": { "static": { "axes": [
            { "letter": "X", "direction": [1, 0, 0], "travel": [-2000.0, 2000.0] },
            { "letter": "Y", "direction": [0, 1, 0], "travel": [-2000.0, 2000.0] }
        ], "rotaries": [] } }
    });
    let stream = "PARAM StartX 1000\nWATERJET ABRASIVE\nSECTION\nRapid X 1000000 Y 0 Z 0\nJET ON C0 OUTER\nArc CCW X -1000000 Y 0 Z 100 F300000 I 0 J 0\nJET OFF C0\nENDSECTION\n";
    for mode in ["circular", "linear"] {
        let out = post(&machine, stream, &json!({ "arcs": mode }));
        assert_refused(&out.warnings, "a cutting move changes Z - a waterjet program cuts in X and Y only");
        assert!(
            !out.program.lines().any(|line| ["G1 ", "G2 ", "G3 "].iter().any(|word| line.starts_with(word))),
            "{mode}: the refused arc must not write a cut:\n{}",
            out.program
        );
    }
}

/// A positive feed keeps its value after formatting, or the post refuses it.
/// The stream states feed in um/min: F100 is 0.1 mm/min.
#[test]
fn waterjet_post_keeps_a_small_positive_feed_or_refuses_it() {
    let stream = |f: u32| format!("WATERJET\nSECTION\nRapid X 0 Y 0 Z 0\nJET ON C0 OUTER\nFeed X 1000 Y 0 Z 0 F{f}\nJET OFF C0\nENDSECTION\n");
    let mm = post(&jet_machine(), &stream(100), &json!({}));
    assert!(mm.warnings.is_empty(), "{:?}", mm.warnings);
    assert!(mm.program.contains("G1 X1 Y0 F0.1\n"), "metric keeps 0.1 mm/min:\n{}", mm.program);
    assert!(!mm.program.contains("F0\n") && !mm.program.contains("F0 "), "{}", mm.program);
    let tiny = post(&jet_machine(), &stream(0), &json!({}));
    assert_refused(&tiny.warnings, "states no feed");
    let thou = post(&jet_machine(), &stream(1), &json!({}));
    assert!(thou.program.contains("F0.001\n"), "metric keeps 0.001 mm/min:\n{}", thou.program);
    // 0.1 mm/min is 0.0039 in/min: below the inch format, so refused.
    let inch = post(&jet_machine(), &stream(100), &json!({ "Units": "inch" }));
    assert_refused(&inch.warnings, "below the program's feed resolution (F0)");
    assert!(!inch.program.contains("F0\n") && !inch.program.contains("F0 "), "{}", inch.program);
    // 254 mm/min is 10 in/min and posts as such.
    let ten = post(&jet_machine(), &stream(254_000), &json!({ "Units": "inch" }));
    assert!(ten.warnings.is_empty(), "{:?}", ten.warnings);
    assert!(ten.program.contains("F10\n"), "{}", ten.program);
}

/// A section with no WATERJET header after a waterjet section: the
/// previous ENDSECTION cleared the header, so `onSection` must see no
/// header and the post must refuse the section by name. It must not drop
/// the section's moves without a warning.
#[test]
fn waterjet_post_refuses_an_adjacent_section_with_no_header() {
    let stream = "WATERJET\nSECTION\nRapid X 0 Y 0 Z 0\nJET ON C0 OUTER\nFeed X 1000 Y 0 Z 0 F300000\nJET OFF C0\nENDSECTION\nSECTION\nFeed X 10000 Y 0 Z 0 F300000\nENDSECTION\n";
    let out = post(&jet_machine(), stream, &json!({}));
    assert_refused(&out.warnings, "a path with no WATERJET header is not waterjet cutting");
    assert!(out.program.contains("G1 X1 Y0 F300\n"), "the first section posts:\n{}", out.program);
    // The second section's hook saw the current properties: no second preamble.
    assert_eq!(out.program.matches("NOZZLE - ").count(), 1, "{}", out.program);
    assert_eq!(out.program.matches("G54\n").count(), 1, "{}", out.program);
}

#[test]
fn waterjet_rapid_cache_resets_when_the_coordinate_frame_changes() {
    // Each new frame needs a physical traverse, even when its words match
    // the last rapid. Exercise offset and each origin independently.
    for (parameter, value, x, y, word) in [
        ("WorkOffset", 2, 0, 0, "G55"),
        ("OriginX", 100, 100000, 0, "G54"),
        ("OriginY", 100, 0, 100000, "G54"),
    ] {
        let stream = format!(
            "PARAM WorkOffset 1\nWATERJET\nSECTION\nRapid X 0 Y 0 Z 0\nENDSECTION\nPARAM {parameter} {value}\nWATERJET\nSECTION\nRapid X {x} Y {y} Z 0\nJET ON C0 OUTER\nFeed X {} Y {y} Z 0 F300000\nJET OFF C0\nENDSECTION\n",
            x + 10000
        );
        let out = post(&jet_machine(), &stream, &json!({}));
        assert!(out.warnings.is_empty(), "{parameter}: {:?}", out.warnings);
        let frame = out
            .program
            .rfind(&format!("{word}\n"))
            .expect("second frame");
        let tail = &out.program[frame..];
        let rapid = tail
            .find("G0 X0 Y0\n")
            .expect("the new frame must rapid before opening the jet");
        let jet = tail.find("M10\n").expect("water on");
        assert!(rapid < jet, "{parameter}: {tail}");
        assert_eq!(
            out.program.matches("G0 X0 Y0\n").count(),
            2,
            "{parameter}: {}",
            out.program
        );
    }
}

/// A jet span from X20 to `start`, then `cut`, in the plate's frame.
fn span(start: (&str, &str), cut: &str) -> String {
    let (sx, sy) = start;
    format!(
        "PARAM OperationName Arc pin\nPARAM WorkOffset 1\nWATERJET ABRASIVE DWELL800 LAG300\nSECTION\n\
         Rapid X 20000 Y {sy} Z 0\nJET ON C0 OPEN\nFeed X {sx} Y {sy} Z 0 F 300000\n{cut}JET OFF C0\nENDSECTION\n"
    )
}

/// The blocks and the warnings of one post, without the `(EXTENT ...)`
/// comment: `programBounds` boxes a short arc record by its full circle
/// (the safe side for the travel check), so that comment differs between
/// an arc record and its straight reference.
fn program(stream: &str, unit: &str) -> String {
    let out = post(&jet_machine(), stream, &json!({ "Units": unit }));
    let blocks: String = out.program.lines().filter(|l| !l.starts_with("(EXTENT ")).map(|l| format!("{l}\n")).collect();
    format!("{blocks}--- warnings: {:?}\n", out.warnings)
}

fn arc_lines(text: &str) -> Vec<&str> {
    text.lines().filter(|l| l.starts_with("G2 ") || l.starts_with("G3 ")).collect()
}

/// Review of #3046 (the rule of #3442): the reviewer's stream. The record
/// is an arc of 0.03 um whose written ends are the same point. The post
/// wrote `G3 X59.5 Y30 I-9.5 J0`, which a control cuts as a full circle
/// of radius 9.5 mm with the jet on. It is no block now, in each unit.
#[test]
fn waterjet_post_writes_no_full_circle_for_an_arc_under_the_output_step() {
    let cut = |arc: bool| {
        let short = if arc { "Arc CCW X 59500 Y 30000.03 Z 0 F 300000 I 50000 J 30000\n" } else { "" };
        format!(
            "WATERJET\nSECTION\nRapid X 50000 Y 30000 Z 0\nJET ON C0 INNER\nFeed X 59500 Y 30000 Z 0 F 300000\n\
             {short}Feed X 70000 Y 30000 Z 0 F 300000\nJET OFF C0\nENDSECTION\n"
        )
    };
    for unit in ["mm", "inch"] {
        let (got, want) = (program(&cut(true), unit), program(&cut(false), unit));
        assert!(arc_lines(&got).is_empty(), "{unit}: an arc block for a 0.03 um arc:\n{got}");
        assert_eq!(got, want, "{unit}: the short arc posts as no block");
        assert!(got.ends_with("warnings: []\n"), "{unit}: {got}");
    }
    let mm = program(&cut(true), "mm");
    assert!(mm.contains("G1 X59.5 Y30 F300\nG1 X70 Y30\n"), "{mm}");
}

/// The rule of #3474 for this post: a short arc whose written ends are on
/// one ray from the centre that the control computes (the written start
/// plus the written I J) posts as the straight move to its end. The
/// witnesses are those of `same_angle_arcs.rs`: (start, end, centre,
/// direction) in micrometres, the plain pair and the offset-centre pair
/// of #3500.
#[test]
fn waterjet_post_writes_a_same_angle_arc_as_its_straight_move() {
    for (unit, start, end, centre, dir, word) in [
        ("mm", ("10010.51", "5000"), ("10010.498393", "5000.493788"), ("10000", "5000"), "CCW", "G1 X10.01 Y5\n"),
        ("inch", ("10186.68", "5080"), ("10186.658660", "5081.066911"), ("10160", "5080"), "CCW", "G1 X0.401 Y0.2\n"),
        ("mm", ("10010.51", "5000.49"), ("10010.475372320", "4999.996752949"), ("10000", "5000.98"), "CW", "G1 X10.01 Y5\n"),
        ("inch", ("10186.72", "5081.143"), ("10186.645370", "5079.768811"), ("10160", "5081.905"), "CW", "G1 X0.401 Y0.2\n"),
    ] {
        let ((ex, ey), (cx, cy)) = (end, centre);
        let arc = span(start, &format!("Arc {dir} X {ex} Y {ey} Z 0 F 300000 I {cx} J {cy}\n"));
        let feed = span(start, &format!("Feed X {ex} Y {ey} Z 0 F 300000\n"));
        let (got, want) = (program(&arc, unit), program(&feed, unit));
        assert!(arc_lines(&got).is_empty(), "{unit} {dir}: an arc block:\n{got}");
        assert_eq!(got, want, "{unit} {dir}: the same-angle arc posts as the straight feed");
        assert!(got.ends_with("warnings: []\n") && got.contains(word), "{unit} {dir}: {got}");
    }
    // A normal quarter arc from the same start keeps its arc block.
    let quarter = program(&span(("20000", "5000"), "Arc CCW X 10000 Y 15000 Z 0 F 300000 I 10000 J 5000\n"), "mm");
    assert_eq!(arc_lines(&quarter), ["G3 X10 Y15 I-10 J0"], "{quarter}");
}
