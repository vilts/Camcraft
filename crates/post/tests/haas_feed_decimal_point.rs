//! #3681: the Haas post writes every F word with its decimal point
//! (`F600.`, never `F600`). Haas Setting 77 (Scale Integer F; Haas Mill
//! Operator's Manual 96-8000, Settings, "77 - Scale Integer F") scales an
//! F word with no point: at its default an mm control reads F600 as
//! 0.6 mm/min. With the point, each value of the setting reads the same feed.
//! #3694: the F word keeps its fraction (mm/min at three decimals, in/min
//! at four), so a G84 tap feed of pitch x rpm cuts the tap's pitch.
//! Provenance: our own tests over our own post and stream format.
//!
//! Gate map: covers assets/posts/haas.cps, camcraft_post::post::engine.
//! Lane shape: all tests here run in milliseconds.

use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn haas() -> String {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts/haas.cps");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The code words of a block, with `( ... )` comments removed.
fn code_words(line: &str) -> Vec<String> {
    let mut code = String::new();
    let mut depth = 0u32;
    for c in line.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => code.push(c),
            _ => {}
        }
    }
    code.split_whitespace().map(str::to_string).collect()
}

/// `F<digits>.<digits>*` - the only F word shape this post may write.
fn is_pointed_feed(w: &str) -> bool {
    let Some(rest) = w.strip_prefix('F') else { return false };
    let Some((int, frac)) = rest.split_once('.') else { return false };
    !int.is_empty() && int.chars().all(|c| c.is_ascii_digit()) && frac.chars().all(|c| c.is_ascii_digit())
}

/// Every F word of `program`, and the ones that break the shape.
fn feeds(program: &str) -> (Vec<String>, Vec<String>) {
    let (mut all, mut bad) = (Vec::new(), Vec::new());
    for line in program.lines() {
        let t = line.trim();
        if t.starts_with(';') || t.starts_with('%') {
            continue;
        }
        for w in code_words(t) {
            if w.starts_with('F') && w.len() > 1 {
                if !is_pointed_feed(&w) {
                    bad.push(format!("{w} in `{t}`"));
                }
                all.push(w);
            }
        }
    }
    (all, bad)
}

/// One operation: linear feeds, an arc, a zero-feed-free plunge. Feeds are
/// whole mm/min (600, 254) and 50.8 mm/min (2 in/min, a whole inch feed).
const MILL: &str = "PARAM OperationName Contour
PARAM ToolDiameter 10.000000
PARAM SpindleSpeed 6000.000000
PARAM WorkOffset G54
PARAM ClearancePlane 50.000000
PARAM RetractPlane 25.000000
TOOL 1 10MM END MILL
SECTION
Rapid X 30000 Y 30000 Z 50000
Rapid X 30000 Y 30000 Z 14000
Plunge X 30000 Y 30000 Z -8000 F 50800
Feed X 60000 Y 30000 Z -8000 F 600000
Arc CCW X 60000 Y 50800 Z -8000 F 254000 I 60000 J 40400
Feed X 30000 Y 50800 Z -8000 F 600000
Rapid X 30000 Y 50800 Z 50000
ENDSECTION
";

/// A probing section: protected move, touch, back-off.
const PROBE: &str = "PARAM OperationName Probe
PARAM ToolDiameter 6.000000
PARAM SpindleSpeed 0.000000
PARAM WorkOffset G54
PARAM ClearancePlane 10.000000
PARAM RetractPlane 3.000000
TOOL 20 PROBE-6
SECTION
Rapid X 10000 Y 10000 Z 10000
GUARD 10000 10000 5000 F 300000
TOUCH 10000 10000 -5000 EXPECT 10000 10000 0 F 25400 M -
RETREAT 0 0 1000 EXPECT 10000 10000 1000 F 300000
Rapid X 10000 Y 10000 Z 10000
ENDSECTION
";

/// One canned-cycle section of `kind`, with its chain at the hole. A
/// FINEBORE chain shifts off the wall at the bottom (I0.3 J-0.4).
fn cycle(n: usize, kind: &str, dwell: &str) -> String {
    let out = if kind == "FINEBORE" {
        "Rapid X 30300 Y 9600 Z -8000\nRapid X 30300 Y 9600 Z 1000\nRapid X 30000 Y 10000 Z 1000\n"
    } else {
        "Rapid X 30000 Y 10000 Z 1000\n"
    };
    format!(
        "PARAM OperationName Cycle{n}
PARAM ToolDiameter 6.000000
PARAM SpindleSpeed 1500.000000
PARAM WorkOffset G54
PARAM ClearancePlane 10.000000
PARAM RetractPlane 3.000000
TOOL {t} TOOL-{n}
SECTION
Rapid X 30000 Y 10000 Z 10000
Rapid X 30000 Y 10000 Z 1000
CYCLE {kind} X 30000 Y 10000 Z -8000 R 1000 ENTRY 3000 Q 4000 P {dwell} F 60000 AT 2
Plunge X 30000 Y 10000 Z -8000 F 60000
{out}Rapid X 30000 Y 10000 Z 10000
ENDSECTION
",
        t = n + 1
    )
}

const CYCLES: &[(&str, &str)] = &[
    ("SPOT", "0"),
    ("DRILL", "500"),
    ("PECK", "0"),
    ("DEEP", "0"),
    ("TAP", "0"),
    ("FLOATINGTAP", "0"),
    ("BORE", "0"),
    ("REAM", "0"),
    ("FINEBORE", "0"),
    ("BACKBORE", "0"),
];

struct Posted {
    program: String,
    warnings: Vec<String>,
}

fn post(stream: &str, units: &str) -> Result<Posted, String> {
    let out = engine::run_with(&haas(), &Value::Null, stream, &json!({ "Units": units })).map_err(|e| e.to_string())?;
    Ok(Posted { program: out.program, warnings: out.warnings.iter().map(|w| w.to_string()).collect() })
}

fn has_feed(program: &str, word: &str) -> bool {
    feeds(program).0.iter().any(|w| w == word)
}

/// Posts the mill and probe sections and each cycle on its own. Returns
/// all programs that posted clean, and the cycle words they hold.
fn post_all(units: &str) -> (Vec<String>, Vec<String>) {
    let mut programs = Vec::new();
    for (name, s) in [("mill", MILL), ("probe", PROBE)] {
        let out = post(s, units).unwrap_or_else(|e| panic!("{units} {name}: {e}"));
        assert!(out.warnings.is_empty(), "{units} {name}: {:?}\n{}", out.warnings, out.program);
        programs.push(out.program);
    }
    let mut words = Vec::new();
    for (i, (kind, p)) in CYCLES.iter().enumerate() {
        // A cycle that this post refuses by name is not a feed word.
        let out = match post(&cycle(i, kind, p), units) {
            Ok(out) if out.warnings.is_empty() => out,
            Ok(out) => {
                eprintln!("{units} {kind}: refused: {:?}", out.warnings);
                continue;
            }
            Err(e) => {
                eprintln!("{units} {kind}: refused: {e}");
                continue;
            }
        };
        for line in out.program.lines() {
            for w in code_words(line) {
                if matches!(w.as_str(), "G73" | "G76" | "G81" | "G82" | "G83" | "G84" | "G85" | "G86" | "G89") && !words.contains(&w) {
                    words.push(w);
                }
            }
        }
        programs.push(out.program);
    }
    (programs, words)
}

fn assert_every_feed_pointed(units: &str) {
    let (programs, cycles) = post_all(units);
    for g in ["G73", "G76", "G81", "G82", "G83", "G84", "G85", "G86", "G89"] {
        assert!(cycles.contains(&g.to_string()), "{units}: no {g} posted, so its F word is not pinned: {cycles:?}");
    }
    let mut count = 0;
    for p in &programs {
        let (all, bad) = feeds(p);
        assert!(bad.is_empty(), "{units}: F words with no decimal point: {bad:?}\n{p}");
        count += all.len();
    }
    assert!(count >= 12, "{units}: only {count} F words were posted");
    let probe = &programs[1];
    assert!(probe.contains("G31"), "{units}: the probe section writes G31\n{probe}");
}

#[test]
fn haas_writes_every_mm_feed_word_with_its_point() {
    assert_every_feed_pointed("mm");
    let out = post(MILL, "mm").unwrap_or_else(|e| panic!("{e}"));
    assert!(has_feed(&out.program, "F600."), "a whole mm feed keeps its point\n{}", out.program);
    // #3694: an mm feed keeps its fraction (was F51.).
    assert!(has_feed(&out.program, "F50.8"), "50.8 mm/min posts as F50.8\n{}", out.program);
    assert!(!has_feed(&out.program, "F51."), "50.8 mm/min is not rounded\n{}", out.program);
}

#[test]
fn haas_keeps_the_fraction_of_an_mm_feed() {
    // #3692 / #3694: 12.4 mm/min posts as F12.4, never F12.
    let s = MILL.replace("F 600000", "F 12400");
    let out = post(&s, "mm").unwrap_or_else(|e| panic!("{e}"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(has_feed(&out.program, "F12.4"), "{}", out.program);
    assert!(!has_feed(&out.program, "F12."), "{}", out.program);
}

/// One G84 section: the tap feed `feed_um` (0.001 mm/min) at `rpm`.
fn tap(feed_um: u64, rpm: &str) -> String {
    cycle(4, "TAP", "0")
        .replace("F 60000", &format!("F {feed_um}"))
        .replace("PARAM SpindleSpeed 1500.000000", &format!("PARAM SpindleSpeed {rpm}"))
}

/// The F word of the G84 block of `program`.
fn g84_feed(program: &str) -> Option<String> {
    program
        .lines()
        .map(code_words)
        .find(|w| w.iter().any(|x| x == "G84"))
        .and_then(|w| w.into_iter().find(|x| x.starts_with('F')))
}

fn tap_refused(out: &Result<Posted, String>) -> bool {
    match out {
        Err(e) => e.contains("G84 (tap)"),
        Ok(out) => out.warnings.iter().any(|w| w.contains("G84 (tap)")),
    }
}

#[test]
fn haas_g84_writes_the_exact_tap_feed_in_mm() {
    // M1.4 x 0.3 at S115 = 34.5; M2 x 0.4 at S333 = 133.2; M8 x 1.25 at
    // S800 = 1000 (a whole feed keeps its point).
    for (feed, rpm, word) in [(34_500, "115", "F34.5"), (133_200, "333", "F133.2"), (1_000_000, "800", "F1000.")] {
        let out = post(&tap(feed, rpm), "mm").unwrap_or_else(|e| panic!("{e}"));
        assert!(out.warnings.is_empty(), "{word}: {:?}", out.warnings);
        assert_eq!(g84_feed(&out.program).as_deref(), Some(word), "{}", out.program);
        assert!(out.program.contains(&format!("S{rpm} M3")), "{}", out.program);
    }
}

#[test]
fn haas_g84_writes_the_tap_feed_at_four_inch_decimals() {
    // 1/4-20 (1.27 mm) at S500 = 635 mm/min = 25 in/min exactly. The metric
    // taps of an inch program round at the fourth decimal: 34.5 mm/min is
    // 1.35827 in/min (F1.3583, 0.002 percent pitch error) and 133.2 mm/min
    // is 5.24409 in/min (F5.2441).
    for (feed, rpm, word) in [(635_000, "500", "F25."), (34_500, "115", "F1.3583"), (133_200, "333", "F5.2441")] {
        let out = post(&tap(feed, rpm), "inch").unwrap_or_else(|e| panic!("{e}"));
        assert!(out.warnings.is_empty(), "{word}: {:?}", out.warnings);
        assert!(out.program.contains("G20"), "{}", out.program);
        assert_eq!(g84_feed(&out.program).as_deref(), Some(word), "{}", out.program);
    }
}

#[test]
fn haas_refuses_a_tap_pitch_its_words_cannot_hold() {
    // 2 mm/min in inch is 0.07874 in/min, written F0.0787: 0.05 percent off.
    let inch = post(&tap(2_000, "100"), "inch");
    assert!(tap_refused(&inch), "a pitch more than 0.01 percent off is refused by name");
    // A fractional rpm posts as a whole S: 34.5 mm/min at 115.4 rpm is a
    // 0.299 mm pitch, written F34.5 S115 (0.3 mm): 0.35 percent off.
    let speed = post(&tap(34_500, "115.4"), "mm");
    assert!(tap_refused(&speed), "a rounded S that moves the pitch is refused by name");
    // The same tap at a whole rpm posts.
    let ok = post(&tap(34_500, "115"), "mm").unwrap_or_else(|e| panic!("{e}"));
    assert!(ok.warnings.is_empty(), "{:?}", ok.warnings);
}

#[test]
fn haas_writes_every_inch_feed_word_with_its_point() {
    assert_every_feed_pointed("inch");
    let out = post(MILL, "inch").unwrap_or_else(|e| panic!("{e}"));
    assert!(out.program.contains("G20"), "an inch program\n{}", out.program);
    // 50.8 mm/min is 2 in/min and 254 mm/min is 10 in/min: whole values.
    assert!(has_feed(&out.program, "F2."), "{}", out.program);
    assert!(has_feed(&out.program, "F10."), "{}", out.program);
}

#[test]
fn haas_keeps_the_fraction_of_a_feed_that_rounds_to_zero() {
    // 0.3 mm/min would round to F0, a stopped feed.
    let s = MILL.replace("F 600000", "F 300");
    let out = post(&s, "mm").unwrap_or_else(|e| panic!("{e}"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(has_feed(&out.program, "F0.3"), "{}", out.program);
    let (_, bad) = feeds(&out.program);
    assert!(bad.is_empty(), "{bad:?}");
    assert!(!has_feed(&out.program, "F0."), "no positive feed posts as F0.\n{}", out.program);
}

#[test]
fn haas_refuses_a_feed_below_its_smallest_inch_step() {
    // 0.001 mm/min is under 0.0001 in/min: refused, never written as F0.
    let s = MILL.replace("F 600000", "F 1");
    let refused = match post(&s, "inch") {
        Err(e) => e.contains("below the smallest feed"),
        Ok(out) => out.warnings.iter().any(|w| w.contains("below the smallest feed")),
    };
    assert!(refused, "a feed below the smallest step is refused by name");
}
