//! #1411: the Mazak post writes its spindle in RS-274 / ISO 6983 words.
//! A spindle start is `S.. M3` (clockwise) or `S.. M4` (a left-hand-cut
//! mill, PARAM MillSpindle reverse, #988), and a stop is `M5` - never the
//! multi-letter `MOTON S..` / `MOTOFF` the post wrote before, which no
//! public document gives and which state no direction ahead of a G84
//! right-hand tap. Every word of a cutting program is an address letter
//! and a number.
//!
//! Provenance: RS-274 / ISO 6983 (word = address letter + number; M3 / M4
//! / M5 spindle clockwise / counter-clockwise / stop; G84 right-hand
//! tapping with the spindle clockwise); our own synthetic streams.
//!
//! Gate map (M4.4): covers assets/posts/mazak.cps (startPreamble,
//! endPreamble, onParameter MillSpindle), camcraft_post::post::engine.

use camcraft_post::post::engine;
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

fn post(stream: &str) -> engine::PostOutput {
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    engine::run(&read("assets/posts/mazak.cps"), &mch, stream).expect("mazak posts")
}

const HEAD: &str = "; HSMW-CNC v1
PARAM WorkOffset G54
PARAM OriginX 0.000000
PARAM OriginY 0.000000
PARAM OriginZ 0.000000
PARAM ClearancePlane 45.000000
";

/// One right-hand rigid tap at X0 Y0 from R20 to Z8 (the issue's case).
fn tap_stream() -> String {
    format!(
        "{HEAD}PARAM OperationName T1
PARAM SpindleSpeed 600.000000
PARAM RetractPlane 20.000000
TOOL 2 TAP-M6
SECTION
Rapid X 0 Y 0 Z 45000
Rapid X 0 Y 0 Z 20000
CYCLE TAP X 0 Y 0 Z 8000 R 20000 ENTRY 0 Q 0 P 0 F 600000
Plunge X 0 Y 0 Z 8000 F 600000
Plunge X 0 Y 0 Z 20000 F 600000
Rapid X 0 Y 0 Z 45000
ENDSECTION
"
    )
}

/// A cutting section of its own: an approach, one feed, a retract.
fn mill_section(op: &str, tool: u32, reverse: bool) -> String {
    let rev = if reverse { "PARAM MillSpindle reverse\n" } else { "" };
    format!(
        "PARAM OperationName {op}
PARAM SpindleSpeed 3000.000000
PARAM RetractPlane 20.000000
{rev}TOOL {tool} MILL-{tool}
SECTION
Rapid X 10000 Y 10000 Z 45000
Rapid X 10000 Y 10000 Z 5000
Feed X 10000 Y 10000 Z -1000 F 300000
Feed X 30000 Y 10000 Z -1000 F 300000
Rapid X 30000 Y 10000 Z 45000
ENDSECTION
"
    )
}

/// The program's blocks as word lists, comments dropped.
fn blocks(program: &str) -> Vec<Vec<String>> {
    program
        .lines()
        .map(|l| {
            let mut s = String::new();
            let mut depth = 0;
            for c in l.chars() {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ if depth == 0 => s.push(c),
                    _ => {}
                }
            }
            s.split_whitespace().map(str::to_string).collect()
        })
        .collect()
}

/// RS-274 / ISO 6983: a word is one address letter and a number.
fn is_word(w: &str) -> bool {
    let mut c = w.chars();
    let letter = c.next().is_some_and(|l| l.is_ascii_uppercase());
    let rest = c.as_str();
    letter && !rest.is_empty() && rest.trim_start_matches(['-', '+']).chars().all(|d| d.is_ascii_digit() || d == '.')
        && rest.parse::<f64>().is_ok()
}

fn assert_iso_words(program: &str) {
    for ws in blocks(program) {
        for w in &ws {
            assert!(w == "%" || is_word(w), "`{w}` is no letter+number word:\n{program}");
        }
    }
}

fn index_of(bs: &[Vec<String>], pred: impl Fn(&[String]) -> bool) -> Option<usize> {
    bs.iter().position(|ws| pred(ws))
}

#[test]
fn a_rigid_tap_meets_a_spindle_started_clockwise_and_the_section_ends_m5() {
    let out = post(&tap_stream());
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    let bs = blocks(p);
    let start = index_of(&bs, |ws| ws.iter().any(|w| w == "S600") && ws.iter().any(|w| w == "M3"))
        .unwrap_or_else(|| panic!("no `S600 M3` block:\n{p}"));
    let tap = index_of(&bs, |ws| ws.first().is_some_and(|w| w == "G84")).unwrap_or_else(|| panic!("no G84:\n{p}"));
    assert!(start < tap, "the spindle start comes after the tap:\n{p}");
    let g43 = index_of(&bs, |ws| ws.first().is_some_and(|w| w == "G43")).unwrap_or_else(|| panic!("no G43:\n{p}"));
    assert!(g43 < start, "the spindle starts before the length offset:\n{p}");
    let stop = bs.iter().rposition(|ws| ws == &["M5"]).unwrap_or_else(|| panic!("no M5 block:\n{p}"));
    assert!(stop > tap, "no M5 after the tap:\n{p}");
    assert!(!bs.iter().flatten().any(|w| w == "M4"), "a right-hand tap never runs M4:\n{p}");
    assert_iso_words(p);
}

#[test]
fn a_left_hand_cut_mill_starts_m4_and_the_next_section_m3_again() {
    let stream = format!("{HEAD}{}{}", mill_section("Left", 4, true), mill_section("Right", 5, false));
    let out = post(&stream);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    let bs = blocks(p);
    let starts: Vec<&Vec<String>> = bs.iter().filter(|ws| ws.iter().any(|w| w == "S3000")).collect();
    assert_eq!(starts.len(), 2, "{p}");
    assert_eq!(starts[0], &vec!["S3000".to_string(), "M4".to_string()], "{p}");
    assert_eq!(starts[1], &vec!["S3000".to_string(), "M3".to_string()], "{p}");
    assert_eq!(bs.iter().filter(|ws| *ws == &["M5"]).count(), 2, "one M5 per section:\n{p}");
    assert_iso_words(p);
}

#[test]
fn no_mazak_program_carries_the_multi_letter_spindle_words() {
    let out = post(&tap_stream());
    for gone in ["MOTON", "MOTOFF"] {
        assert!(!out.program.contains(gone), "{gone}:\n{}", out.program);
    }
    // The recorded goldens too: every block of a cutting program is
    // letter+number words (the probe golden's macro-B statements - `#`
    // variables, `IF[..]GOTO`, bracketed axis expressions - are its own
    // documented form and skipped).
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/golden/mazak");
    let mut seen = 0;
    for e in std::fs::read_dir(&dir).expect("golden dir") {
        let path = e.expect("entry").path();
        if path.extension().and_then(|x| x.to_str()) != Some("nc") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("golden").replace("\r\n", "\n");
        let plain: String =
            text.lines().filter(|l| !l.contains('#') && !l.contains('[')).collect::<Vec<_>>().join("\n");
        assert!(!text.contains("MOTON") && !text.contains("MOTOFF"), "{}", path.display());
        assert_iso_words(&plain);
        seen += 1;
    }
    assert!(seen >= 5, "read {seen} goldens");
}
