//! The Synthetos TinyG post: the program shape a TinyG reads (no % fence,
//! no O word, no N numbers, M30; only the word letters and G/M codes its
//! wiki lists), no tool length word, `T<n> M6` then `M0` after a Z lift
//! (M6 moves nothing on TinyG, so the stop is the operator's change),
//! drilling posted as the plain-move chain with `G4 P<s>` at the bottom
//! (no canned cycles), and the words TinyG does not have - G84, G41/G42,
//! tools over 31 - refused by name. The Z lift never trusts machine Z0
//! unless the machine file states `safe_retract` (grblhal.cps's law: an
//! unhomed board's machine zero is its power-on height). The generic
//! machine file mounts and posts clean.
//! Provenance: our own tests over our own post and machine file; the
//! dialect facts from TinyG's public wiki ("Gcode Support").
//!
//! Gate map: covers camcraft_post::post (tinyg.cps), camcraft_post::machine (safeRetractZ,
//! units), app::posting (registry, machine library).

mod common;

use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use camcraft_app::posting::{BUILTINS, MACHINES};
use serde_json::{Value, json};
use common::legacy_program::as_program;
use std::path::Path;

const STREAM: &str = "PARAM ClearancePlane 15\nPARAM StockZTop 0\nPARAM SpindleSpeed 8000\nTOOL 3 EM6\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 10000 Y 0 Z -1000 F 500000\nENDSECTION\n\
                      PARAM ClearancePlane 15\nPARAM SpindleSpeed 6000\nTOOL 4 DRILL5\nSECTION\nCYCLE DRILL X 20000 Y 5000 Z -3000 R 2000 P 1500 F 100000\nRapid X 20000 Y 5000 Z 2000\nPlunge X 20000 Y 5000 Z -3000 F 100000\nRapid X 20000 Y 5000 Z 2000\nENDSECTION\n";

/// The G and M codes TinyG's wiki lists (the dotted ones spelled as the
/// board spells them), and the word letters a block may carry.
const CODES: &[&str] = &[
    "G0", "G1", "G2", "G3", "G4", "G10", "G17", "G18", "G19", "G20", "G21", "G28", "G28.1", "G28.2", "G28.3",
    "G28.4", "G30", "G30.1", "G38.2", "G40", "G43.1", "G49", "G53", "G54", "G55", "G56", "G57", "G58", "G59",
    "G61", "G61.1", "G64", "G80", "G90", "G91", "G90.1", "G91.1", "G92", "G92.1", "G92.2", "G92.3", "G93",
    "G94", "M0", "M1", "M2", "M30", "M60", "M3", "M4", "M5", "M6", "M7", "M8", "M9", "M105", "M114", "M115",
    "M201.3", "M400",
];
const LETTERS: &str = "GMTFPSXYZABCIJKLRN";

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn cps() -> String {
    read("assets/posts/tinyg.cps")
}

/// The shipped generic machine, mounted the way the app hands it on.
fn generic() -> Value {
    let text = MACHINES
        .iter()
        .find(|(n, _)| *n == "TinyG 3-axis")
        .map(|(_, t)| *t)
        .expect("the library ships the TinyG machine");
    machine::mirror(&parse_mch(text).unwrap_or_else(|e| panic!("parses: {e}")))
}

fn post(m: &Value, stream: &str) -> String {
    let out = engine::run(&cps(), m, stream).unwrap_or_else(|e| panic!("posts: {e}"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out.program
}

/// #902: the program's unit is the document's (`postOptions.Units`),
/// never the machine file's.
fn post_with(m: &Value, stream: &str, options: &Value) -> String {
    let out = engine::run_with(&cps(), m, stream, options).unwrap_or_else(|e| panic!("posts: {e}"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out.program
}

/// Every word of every non-comment line is a letter TinyG accepts, and
/// every G or M code is one its wiki lists.
fn assert_tinyg_words(nc: &str) {
    for l in nc.lines() {
        if l.starts_with('(') {
            assert!(l.ends_with(')') && l.matches('(').count() == 1, "one closed comment: {l:?}");
            assert!(!l.contains('!') && !l.contains(';') && !l.contains('%'), "a control character in a comment: {l:?}");
            continue;
        }
        assert!(l.len() <= 191, "over TinyG's line length: {l:?}");
        for w in l.split_whitespace() {
            let letter = w.chars().next().expect("a word");
            assert!(LETTERS.contains(letter), "{letter} is not a TinyG word letter: {l:?}\n{nc}");
            if letter == 'G' || letter == 'M' {
                assert!(CODES.contains(&w), "{w} is not in TinyG's table: {l:?}\n{nc}");
            }
        }
    }
}

#[test]
fn the_registry_and_the_library_ship_it() {
    assert!(BUILTINS.iter().any(|(n, s)| *n == "TinyG" && *s == cps()), "the post is built in");
    let m = generic();
    assert!(m.get("safeRetractZ").is_none_or(Value::is_null), "the generic file assumes no homing: {m}");
}

#[test]
fn the_program_speaks_only_what_tinyg_lists() {
    let nc = post(&generic(), STREAM);
    let lines: Vec<&str> = nc.lines().collect();
    assert_eq!(lines.first(), Some(&"(TinyG 3-axis)"), "{nc}");
    assert_eq!(lines.get(1), Some(&"G21 G17 G90 G91.1 G94 G64"), "{nc}");
    assert_eq!(lines.last(), Some(&"M30"), "{nc}");
    assert_tinyg_words(&nc);
    for l in &lines {
        let first = l.split_whitespace().next().unwrap_or("");
        assert!(!first.starts_with('%') && !first.starts_with('O') && !first.starts_with('N'), "{l:?}");
        for w in l.split_whitespace() {
            // No length offset (the operator's), no predefined positions, no cycle words.
            assert!(!["G43", "G43.1", "G49", "G28", "G30", "G80", "G81", "G82", "G83", "G98", "G84"].contains(&w), "{w} in {l:?}\n{nc}");
        }
    }
    for golden in ["contour_disc_hole", "drill_peck", "pocket_zigzag_plate_with_island", "arc_planes"] {
        // #1359: the golden as a real program (tool, spindle) - a cut with
        // the spindle stopped is refused by name.
        let nc = post(&json!({"safeRetractZ": 0.0}), &as_program(golden, &read(&format!("crates/app/tests/golden/{golden}.cl"))));
        assert_tinyg_words(&nc);
    }
}

#[test]
fn each_tool_change_lifts_z_then_selects_the_tool_and_stops() {
    let nc = post(&generic(), STREAM);
    for call in ["T3 M6", "T4 M6"] {
        // Z up in the work offset (no homing assumed), the tool word alone,
        // then the stop that is the operator's change; the spindle starts
        // only after it.
        assert!(nc.contains(&format!("M5\nM9\nG0 Z15\n{call}\nM0\nG21 G17 G90 G91.1 G94 G64 G54\nG0 Z15\nS")), "{call}: lift, call, stop, spindle\n{nc}");
    }
    assert_eq!(nc.matches("\nM0\n").count(), 2, "one stop per tool change and no other\n{nc}");
    // #1616: plus each section's stated first-traverse height.
    assert_eq!(nc.matches("G0 Z15\n").count(), 7, "before and after two changes, two traverses and the end\n{nc}");
    assert!(!nc.contains("G53"), "no machine-coordinate move without homing\n{nc}");
    // Touch-off may lower Z: the first resumed move restores clearance before X Y.
    let lines: Vec<&str> = nc.lines().collect();
    let t4 = lines.iter().position(|l| *l == "T4 M6").expect("T4");
    let first_motion = lines[t4 + 1..].iter().find(|l| l.starts_with("G0 ") || l.starts_with("G1 ")).expect("a move");
    assert_eq!(*first_motion, "G0 Z15", "{nc}");
}

#[test]
fn drilling_posts_as_plain_moves_with_the_dwell_at_the_bottom() {
    let nc = post(&generic(), STREAM);
    // The cycle record itself writes nothing; its chain is the motion.
    assert!(nc.contains("G0 X20 Y5 Z2\nG1 X20 Y5 Z-3 F100\nG4 P1.5\nG0 X20 Y5 Z2\n"), "{nc}");
    // The committed drilling goldens post their chains, never a cycle word.
    let m = json!({"safeRetractZ": 0.0});
    for golden in ["drill_plate", "drill_peck"] {
        let nc = post(&m, &as_program(golden, &read(&format!("crates/app/tests/golden/{golden}.cl"))));
        assert!(nc.lines().filter(|l| l.starts_with("G1 ")).count() >= 2, "{golden}: the plunges post\n{nc}");
        assert!(!nc.contains("G8") && !nc.contains("G98"), "{golden}: no cycle word\n{nc}");
    }
    // A dwell whose chain never reaches the bottom is refused, not dropped.
    let short = STREAM.replacen("Plunge X 20000 Y 5000 Z -3000 F 100000", "Plunge X 20000 Y 5000 Z -2000 F 100000", 1);
    let out = engine::run(&cps(), &generic(), &short).expect("runs");
    assert!(out.warnings.iter().any(|w| w.contains("dwell") && w.contains("refused")), "{:?}", out.warnings);
    assert!(!out.program.contains("G4"), "{}", out.program);
}

#[test]
fn a_tap_is_refused_by_name() {
    let stream = read("crates/app/tests/golden/tap_plate.cl");
    let out = engine::run(&cps(), &generic(), &stream).expect("the script runs");
    let taps: Vec<&String> = out.warnings.iter().filter(|w| w.contains("tapping")).collect();
    assert_eq!(taps.len(), 1, "one named refusal per program: {:?}", out.warnings);
}

#[test]
fn a_tool_number_over_thirty_one_is_refused_by_name() {
    let run = |tool: u32| {
        let stream = STREAM.replacen("TOOL 3 EM6", &format!("TOOL {tool} EM6"), 1);
        engine::run(&cps(), &generic(), &stream).expect("runs")
    };
    assert!(run(31).warnings.is_empty(), "{:?}", run(31).warnings);
    assert!(run(31).program.contains("T31 M6\n"));
    let stray = run(32);
    assert!(stray.warnings.iter().any(|w| w.contains("1-31")), "{:?}", stray.warnings);
}

#[test]
fn a_homed_machine_lifts_in_machine_coordinates() {
    for z in [0.0, -5.0] {
        let mut m = generic();
        m["safeRetractZ"] = json!(z);
        let nc = post(&m, STREAM);
        let want = format!("G53 G0 Z{z}\nT3 M6\nM0\n");
        assert!(nc.contains(&want), "{want:?}\n{nc}");
        assert_eq!(nc.matches("G53 G0 Z").count(), 5, "{nc}");
    }
}

#[test]
fn without_homing_an_unknown_or_buried_lift_height_is_refused_by_name() {
    // No machine at all assumes no homing either.
    let nc = post(&Value::Null, STREAM);
    assert!(nc.contains("G0 Z15\nT3 M6\n") && !nc.contains("G53"), "{nc}");
    let refused = |stream: &str| {
        let out = engine::run(&cps(), &generic(), stream).expect("the script runs");
        assert!(!out.program.contains("G53"), "nothing guessed:\n{}", out.program);
        out.warnings
    };
    let w = refused(&STREAM.replacen("PARAM ClearancePlane 15\n", "", 1));
    assert!(w.iter().any(|w| w.contains("no clearance height is known")), "{w:?}");
    let w = refused(&STREAM.replacen("PARAM StockZTop 0", "PARAM StockZTop 20", 1));
    assert!(w.iter().any(|w| w.contains("is not above the stock top")), "{w:?}");
}

#[test]
fn the_after_operation_stops_are_the_boards_own_words_and_home_is_refused() {
    let with = |ask: &str| STREAM.replacen("PARAM SpindleSpeed 8000\n", &format!("PARAM AfterOperation {ask}\nPARAM SpindleSpeed 8000\n"), 1);
    // Program stop: one more M0, right after the first section's M5 - the
    // tool-change stops are not after-operation stops.
    let nc = post(&generic(), &with("stop"));
    assert!(nc.contains("M5\nM0\n"), "{nc}");
    assert_eq!(nc.matches("\nM0\n").count(), 3, "two tool changes plus the asked stop\n{nc}");
    let nc = post(&generic(), &with("optional-stop"));
    assert!(nc.contains("M5\nM1\n"), "{nc}");
    assert_eq!(nc.matches("\nM0\n").count(), 2, "{nc}");
    let out = engine::run(&cps(), &generic(), &with("home")).expect("runs");
    assert!(out.warnings.iter().any(|w| w.contains("TinyG") && w.contains("home")), "{:?}", out.warnings);
}

#[test]
fn hostile_names_never_carry_a_control_character_into_the_program() {
    // ! is a feed hold and % a queue flush wherever they stand, ( ) and ;
    // end a comment, and a msg prefix is echoed to the operator.
    let stream = STREAM
        .replacen("TOOL 3 EM6", "TOOL 3 msg (stop!) 50% ; now", 1)
        .replacen("PARAM SpindleSpeed 8000\n", "PARAM OperationName msg hello\nPARAM SpindleSpeed 8000\n", 1);
    let nc = post(&generic(), &stream);
    assert_tinyg_words(&nc);
    // Tool and operation notes carry their own prefix, so a msg inside stays text.
    assert!(nc.contains("(T3 msg stop 50 now)"), "{nc}");
    assert!(nc.contains("(OPERATION: msg hello - G54)"), "{nc}");
    // The machine name opens its comment: one that reads as a message is led by "- ".
    let mut m = generic();
    m["name"] = json!("MSG router");
    assert!(post(&m, STREAM).starts_with("(- MSG router)\n"), "{}", post(&m, STREAM));
}

#[test]
fn an_inch_document_posts_g20_and_inches() {
    let nc = post_with(&generic(), STREAM, &json!({"Units": "inch"}));
    assert!(nc.lines().nth(1).is_some_and(|l| l.starts_with("G20 ")), "{nc}");
    assert!(nc.contains("G1 X0.3937 Y0 Z-0.0394 F19.69"), "{nc}");
    assert!(nc.contains("G4 P1.5"), "a dwell is seconds in either unit\n{nc}");
    // An inch machine file alone no longer makes an inch program.
    let mut m = generic();
    m["units"] = json!("inch");
    assert_eq!(post(&m, STREAM), post(&generic(), STREAM), "the machine file's unit decides nothing");
}

#[test]
fn a_smoothing_ask_is_named_once_and_writes_no_tolerance_word() {
    // #616 as #715: TinyG's G64 is run as exact path and takes no P, so the
    // ask is named once and the program is otherwise the smoothing-off one.
    let off = post(&generic(), STREAM);
    let ask = STREAM
        .replacen("PARAM SpindleSpeed 8000\n", "PARAM Smoothing 0.01\nPARAM SmoothingMode finish\nPARAM SpindleSpeed 8000\n", 1)
        .replacen("PARAM SpindleSpeed 6000\n", "PARAM Smoothing 0.1\nPARAM SmoothingMode rough\nPARAM SpindleSpeed 6000\n", 1);
    let on = post(&generic(), &ask);
    let named: Vec<&str> = on.lines().filter(|l| l.contains("SMOOTHING NOT WRITTEN")).collect();
    assert_eq!(named, ["(SMOOTHING NOT WRITTEN - TINYG STATES NO SMOOTHING TOLERANCE)"], "{on}");
    let rest: String = on.lines().filter(|l| !l.contains("SMOOTHING NOT WRITTEN")).map(|l| format!("{l}\n")).collect();
    assert_eq!(rest, off, "only the note differs");
    assert!(!on.contains("G64 P"), "{on}");
    assert_tinyg_words(&on);
}

#[test]
fn feeds_that_round_to_zero_are_refused_in_each_document_unit() {
    for (inch, bad, good, word) in [(false, 499, 500, "F1"), (true, 126, 128, "F0.01")] {
        let m = generic();
        let units = json!({"Units": if inch { "inch" } else { "mm" }});
        for rate in [0, bad] {
            let stream = STREAM.replacen("F 500000", &format!("F {rate}"), 1);
            let out = engine::run_with(&cps(), &m, &stream, &units).expect("runs");
            assert!(out.warnings.iter().any(|w| w.contains("feed") && w.contains("rounding")), "{:?}", out.warnings);
            assert!(!out.program.split_whitespace().any(|w| w == "F0"), "{}", out.program);
        }
        let stream = STREAM.replacen("F 500000", &format!("F {good}"), 1);
        assert!(post_with(&m, &stream, &units).split_whitespace().any(|w| w == word));
    }
    // Hostile JS inputs cannot be represented by the integer CL grammar.
    for value in ["-1", "NaN", "Infinity", "-Infinity"] {
        let script = format!("{}\nvar openSaved = onOpen; onOpen = function() {{ openSaved(); fw({value}); }};", cps());
        let out = engine::run(&script, &generic(), STREAM).expect("runs");
        assert!(out.warnings.iter().any(|w| w.contains("feed") && w.contains("rounding")), "{value}: {:?}", out.warnings);
    }
}

#[test]
fn spindle_delay_is_optional_validated_and_rounds_up_in_seconds() {
    for inch in [false, true] {
        let m = generic();
        let unit = if inch { "inch" } else { "mm" };
        let baseline = post_with(&m, STREAM, &json!({"Units": unit}));
        for options in [json!({"Units": unit}), json!({"Units": unit, "SpindleDwellSeconds": 0})] {
            let out = engine::run_with(&cps(), &m, STREAM, &options).expect("runs");
            assert!(out.warnings.is_empty());
            assert_eq!(out.program, baseline);
        }
        for (delay, word) in [(0.0001, "0.001"), (0.0011, "0.002"), (1.5, "1.5"), (4294.0, "4294")] {
            let out = engine::run_with(&cps(), &m, STREAM, &json!({"Units": unit, "SpindleDwellSeconds": delay})).expect("runs");
            assert!(out.warnings.is_empty(), "{:?}", out.warnings);
            for rpm in [8000, 6000] {
                assert!(out.program.contains(&format!("S{rpm} M3\nG4 P{word}\n")), "{}", out.program);
            }
        }
    }
    for value in [json!(-1), json!(4294.001), json!("1.5"), json!(true)] {
        let out = engine::run_with(&cps(), &generic(), STREAM, &json!({"SpindleDwellSeconds": value})).expect("runs");
        assert!(out.warnings.iter().any(|w| w.contains("SpindleDwellSeconds")), "{:?}", out.warnings);
    }
    let script = format!("{}\nvar openSaved = onOpen; onOpen = function() {{ openSaved(); postOptions.SpindleDwellSeconds = Infinity; }};", cps());
    assert!(engine::run(&script, &generic(), STREAM).expect("runs").warnings.iter().any(|w| w.contains("SpindleDwellSeconds")));
}

#[test]
fn full_xy_circles_and_helices_split_without_losing_direction_or_depth() {
    for direction in ["CW", "CCW"] {
        for end_z in [5000, -5000] {
            let stream = format!("PARAM ClearancePlane 20\nPARAM SpindleSpeed 8000\nRapid X 10000 Y 0 Z 5000\nArc {direction} X 10000 Y 0 Z {end_z} F 100000 I 0 J 0\n");
            let nc = post(&json!({"safeRetractZ": 0.0}), &stream);
            let arcs: Vec<_> = nc.lines().filter(|l| l.starts_with("G2 ") || l.starts_with("G3 ")).collect();
            let code = if direction == "CW" { "G2" } else { "G3" };
            assert_eq!(arcs, [format!("{code} X-10 Y0 Z{} I-10 J0 F100", (5000 + end_z) / 2000), format!("{code} X10 Y0 Z{} I10 J0 F100", end_z / 1000)]);
        }
    }
}

#[test]
fn rounded_coincident_xy_arc_endpoints_are_refused() {
    // 1 micron separates the endpoints in mm, but both round to zero in inches.
    let stream = "PARAM ClearancePlane 20\nPARAM SpindleSpeed 8000\nRapid X 1 Y 0 Z 5000\nArc CCW X 0 Y 1 Z 5000 F 100000 I 0 J 0\n";
    let out = engine::run_with(&cps(), &json!({"safeRetractZ": 0.0}), stream, &json!({"Units": "inch"})).expect("runs");
    assert!(out.warnings.iter().any(|w| w.contains("too small to distinguish")), "{:?}", out.warnings);
}

#[test]
fn vertical_arcs_are_chords_within_five_microns() {
    for (start, arc, axes) in [
        ("X 10000 Y 0 Z 0", "CW X 0 Y 0 Z 10000 I 0 K 0", ['X', 'Z']),
        ("X 0 Y 10000 Z 0", "CCW X 0 Y 0 Z 10000 J 0 K 0", ['Y', 'Z']),
    ] {
        let stream = format!("PARAM ClearancePlane 20\nPARAM SpindleSpeed 8000\nRapid {start}\nArc {arc} F 100000\n");
        let nc = post(&json!({"safeRetractZ": 20.0}), &stream);
        assert!(!nc.lines().any(|l| l.starts_with("G2 ") || l.starts_with("G3 ")), "{nc}");
        let mut previous = [10.0_f64, 0.0];
        let mut count = 0;
        for line in nc.lines().filter(|l| l.starts_with("G1 ")) {
            let point = axes.map(|axis| line.split_whitespace().find_map(|w| w.strip_prefix(axis).and_then(|n| n.parse::<f64>().ok())).expect("axis"));
            assert!((point[0].hypot(point[1]) - 10.0).abs() <= 0.0002, "endpoint: {line}");
            let mid = [(previous[0] + point[0]) / 2.0, (previous[1] + point[1]) / 2.0];
            assert!(10.0 - mid[0].hypot(mid[1]) <= 0.005, "chord: {previous:?} -> {point:?}");
            assert!(point[0] <= previous[0] && point[1] >= previous[1], "direction: {line}");
            previous = point;
            count += 1;
        }
        assert!(count > 1);
        assert_eq!(previous, [0.0, 10.0]);
    }
}

#[test]
fn first_cut_after_a_tool_change_seats_at_a_safe_start_height() {
    for (start, end) in [(5000, 5000), (5000, 10000), (-1000, -1000)] {
        let stream = format!("PARAM ClearancePlane 15\nPARAM StockZTop 0\nPARAM SpindleSpeed 8000\nTOOL 3 EM6\nSECTION\nRapid X 0 Y 0 Z {start}\nENDSECTION\nPARAM ClearancePlane 15\nPARAM SpindleSpeed 8000\nTOOL 4 EM6\nSECTION\nFeed X 10000 Y 0 Z {end} F 100000\nENDSECTION\n");
        let out = engine::run(&cps(), &generic(), &stream).expect("runs");
        if start < 0 {
            assert!(out.warnings.iter().any(|w| w.contains("first cutting move must start above the stock")), "{:?}", out.warnings);
        } else {
            assert!(out.warnings.is_empty(), "{:?}", out.warnings);
            assert!(out.program.contains(&format!("G0 X0 Y0\nG0 Z5\nG1 X10 Y0 Z{} F100", end / 1000)), "{}", out.program);
        }
    }
}
