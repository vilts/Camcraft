//! #885: a feed block that turns a rotary is posted at an F the machine's
//! stated rates allow - no rotary past its stated deg/min, no linear part
//! past the fastest stated feed - read the way each control reads F on a
//! combined block. One prelude rule (`post::rotary_cap`), every dialect
//! that writes a rotary word on a feed: fanuc.cps and doosan-dnm.cps under
//! G43.4 (F is the tip's speed, the rotaries carried along), doosan-dnm.cps
//! without it (one composite length, degrees counted as mm), fanuc-lathe.cps
//! (composite, pinned by `c_axis_unwind`). tormach.cps writes A only on a
//! G0 index and refuses an A that changes inside an operation
//! (`tormach_fourth_axis`), so no feed of it turns a rotary.
//! Provenance: our own tests over our own posts; the feed readings from
//! the FANUC 0i-F / 30i-B operator's manuals (feedrate for rotation axes,
//! tool center point control), behaviour only.
//!
//! Gate map (M4.4): covers post::rotary_cap (the prelude's cap), camcraft_post::post
//! (fanuc.cps, doosan-dnm.cps feed words on tilted blocks).
//! Lane shape (M4.4): all tests here run in well under a second.

use camcraft_app::post::engine;
use serde_json::{Value, json};
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

const FEED_MAX: f64 = 5000.0;
const A_RATE: f64 = 1000.0;

/// A trunnion stating an A about X (TCP-capable) at `rate` deg/min, or no
/// rate when `None`.
fn trunnion(rate: Option<f64>) -> Value {
    let mut a = json!({ "letter": "A", "type": "rotary", "direction": [1.0, 0.0, 0.0], "travel": [-120.0, 120.0] });
    if let Some(r) = rate {
        a["speeds"] = json!({ "normal": r });
    }
    json!({ "name": "TEST TRUNNION", "feedRateMax": FEED_MAX, "kinematics": { "static": { "rotaries": [a] } } })
}

/// A 4th axis stating no direction (no TCP: `machine.rtcp` false), rated.
fn plain_fourth(rate: f64) -> Value {
    json!({ "name": "TEST 4TH", "feedRateMax": FEED_MAX, "kinematics": { "static": { "rotaries": [
        { "letter": "A", "type": "rotary", "speeds": { "normal": rate } } ] } } })
}

/// Short tip moves with large turns, a turn with the tip still, a long
/// move with a small turn, and a tilted arc that turns A.
const STREAM: &str = "\
PARAM ClearancePlane 5
TOOL 1 10MM BALL
PARAM SpindleSpeed 6000
SECTION
Rapid X 10000 Y 0 Z 5000 A 0 B 0 C 0
Feed X 10000 Y 0 Z 0 F 2000000 A 0 B 0 C 0
Feed X 10100 Y 0 Z 0 F 2000000 A 30000 B 0 C 0
Feed X 10100 Y 0 Z 0 F 2000000 A 60000 B 0 C 0
Feed X 40000 Y 0 Z 0 F 2000000 A 61000 B 0 C 0
Arc CCW X 40000 Y 1000 Z 0 F 2000000 I 40000 J 500 A 70000 B 0 C 0
Feed X 40000 Y 1000 Z 1000 F 2000000 A 69000 B 0 C 0
ENDSECTION
";

#[derive(Clone, Copy)]
enum Reading {
    /// F is the speed along the linear path; rotaries carried along.
    Path,
    /// F over sqrt(lin^2 + sum d^2), degrees counted as mm.
    Composite,
}

fn word(l: &str, k: char) -> Option<f64> {
    l.split(' ').find_map(|w| w.strip_prefix(k).and_then(|v| v.parse::<f64>().ok()))
}

/// Reads every posted motion block back, the axes modal, and checks each
/// feed block's rates under `reading` against what the machine states.
/// Returns how many blocks turned A under a feed.
fn assert_within(program: &str, reading: Reading, a_rate: f64) -> usize {
    assert_within_in(program, reading, a_rate, 1.0)
}

/// [`assert_within`] for a program whose lengths and F are in a unit of
/// `mm` millimetres (25.4 for an inch program): every X Y Z and F word is
/// read back in mm, the angles as written.
fn assert_within_in(program: &str, reading: Reading, a_rate: f64, mm: f64) -> usize {
    let word = |l: &str, k: char| word(l, k).map(|v| if k == 'A' { v } else { v * mm });
    let mut at = [0.0_f64; 4]; // X Y Z A
    let mut turned = 0;
    for l in program.lines() {
        let g = l.split(' ').find(|w| matches!(*w, "G00" | "G01" | "G02" | "G03"));
        let Some(g) = g else { continue };
        let next = [
            word(l, 'X').unwrap_or(at[0]),
            word(l, 'Y').unwrap_or(at[1]),
            word(l, 'Z').unwrap_or(at[2]),
            word(l, 'A').unwrap_or(at[3]),
        ];
        let d = [next[0] - at[0], next[1] - at[1], next[2] - at[2], next[3] - at[3]];
        if g != "G00" && d[3].abs() > 1e-9 {
            let f = word(l, 'F').unwrap_or_else(|| panic!("feed block without F: {l}"));
            // An arc is judged over its chord in the plane (XY here): at
            // most the length any reading of its F is measured along.
            let lin = if g == "G01" { (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() } else { (d[0] * d[0] + d[1] * d[1]).sqrt() };
            let (lin_rate, a) = match (reading, g) {
                (Reading::Composite, "G01") => {
                    let lcmd = (lin * lin + d[3] * d[3]).sqrt();
                    (f * lin / lcmd, f * d[3].abs() / lcmd)
                }
                _ if lin > 1e-9 => (f, f * d[3].abs() / lin),
                _ => (0.0, f),
            };
            assert!(lin_rate <= FEED_MAX * 1.0005, "linear past {FEED_MAX}: {l}");
            assert!(a <= a_rate * 1.0005, "A at {a:.1} deg/min past {a_rate}: {l}");
            turned += 1;
        }
        at = next;
    }
    turned
}

/// The 4th-axis brake words a Doosan post asks for (test-bench values).
fn brake() -> Value {
    json!({ "rotaryClamp": "M10", "rotaryUnclamp": "M11" })
}

/// Post `cps` (the Doosan one with the brake words stated).
fn post(cps: &str, mch: &Value, stream: &str) -> engine::PostOutput {
    let opts = if cps.contains("doosan") { brake() } else { json!({}) };
    engine::run_with(&read(cps), mch, stream, &opts).expect("post")
}

/// [`post`] in the program unit `units` ("mm" or "inch", #902).
fn post_units(cps: &str, mch: &Value, stream: &str, units: &str) -> engine::PostOutput {
    let mut opts = if cps.contains("doosan") { brake() } else { json!({}) };
    opts["Units"] = json!(units);
    engine::run_with(&read(cps), mch, stream, &opts).expect("post")
}

fn feeds(program: &str) -> Vec<f64> {
    program.lines().filter(|l| l.contains("G01") || l.contains("G02") || l.contains("G03")).filter_map(|l| word(l, 'F')).collect()
}

#[test]
fn fanuc_tcp_feed_holds_the_stated_a_rate() {
    let out = post("assets/posts/fanuc.cps", &trunnion(Some(A_RATE)), STREAM);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G43.4"), "{}", out.program);
    assert_eq!(assert_within(&out.program, Reading::Path, A_RATE), 5, "{}", out.program);
    // 0.1 mm of tip for 30 degrees: 1000 * 0.1 / 30 = 3.33 -> F3.3, floored.
    assert!(out.program.contains("G01 X10.1 Y0. Z0. A30. F3.3"), "{}", out.program);
    // The tip still while A turns: F is the A rate itself.
    assert!(out.program.contains("G01 X10.1 Y0. Z0. A60. F1000"), "{}", out.program);
    // A long move with a small turn is under the rate: the op's F stands.
    assert!(out.program.contains("G01 X40. Y0. Z0. A61. F2000"), "{}", out.program);
}

#[test]
fn doosan_tcp_feed_holds_the_stated_a_rate() {
    let out = post("assets/posts/doosan-dnm.cps", &trunnion(Some(A_RATE)), STREAM);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G43.4"), "{}", out.program);
    assert_eq!(assert_within(&out.program, Reading::Path, A_RATE), 5, "{}", out.program);
    assert!(out.program.contains("A30. F3.3"), "{}", out.program);
}

#[test]
fn doosan_fourth_axis_composite_feed_holds_the_stated_a_rate() {
    let rate = 600.0;
    // #950: the opening rapid writes its A0 (the post never assumes the
    // table stands at A0), so the control's A is placed and every turn
    // after it is held at the stated rate.
    let placed = post("assets/posts/doosan-dnm.cps", &plain_fourth(rate), STREAM);
    assert!(placed.warnings.is_empty(), "{:?}", placed.warnings);
    assert!(placed.program.contains("G00 X10. Y0. Z5. A0."), "{}", placed.program);
    assert_eq!(assert_within(&placed.program, Reading::Composite, rate), 5, "{}", placed.program);
    // A program whose rapid writes an A word places the table first.
    let stream = STREAM.replace("Rapid X 10000 Y 0 Z 5000 A 0 ", "Rapid X 10000 Y 0 Z 5000 A 1000 ");
    let out = post("assets/posts/doosan-dnm.cps", &plain_fourth(rate), &stream);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(!out.program.contains("G43.4"), "{}", out.program);
    // Six: the opening feed also turns A1 back to A0.
    assert_eq!(assert_within(&out.program, Reading::Composite, rate), 6, "{}", out.program);
    // Composite: lcmd = sqrt(0.1^2 + 30^2); A runs at F * 30 / lcmd <= 600.
    let lcmd = (0.1_f64 * 0.1 + 900.0).sqrt();
    let want = (rate * lcmd / 30.0 * 10.0).floor() / 10.0;
    assert!(out.program.contains(&format!("A30. F{want}")), "want F{want}: {}", out.program);
}

#[test]
fn an_unstated_rate_caps_nothing() {
    for cps in ["assets/posts/fanuc.cps", "assets/posts/doosan-dnm.cps"] {
        let out = post(cps, &trunnion(None), STREAM);
        assert!(out.warnings.is_empty(), "{cps}: {:?}", out.warnings);
        assert!(feeds(&out.program).iter().all(|&f| (f - 2000.0).abs() < 1e-9), "{cps}: {}", out.program);
    }
}

#[test]
fn a_feed_turning_from_an_unknown_angle_is_refused_by_name() {
    // The first tilted record is a feed: the post never placed A, so the
    // turn it commands - and its rate - is unknown.
    let stream = "\
PARAM ClearancePlane 5
TOOL 1 T1
PARAM SpindleSpeed 6000
SECTION
Feed X 10000 Y 0 Z 0 F 2000000 A 30000 B 0 C 0
ENDSECTION
";
    for cps in ["assets/posts/fanuc.cps", "assets/posts/doosan-dnm.cps"] {
        let out = post(cps, &trunnion(Some(A_RATE)), stream);
        assert_eq!(
            out.warnings,
            vec!["a feed that turns A from an angle the post does not know - the stated A rate cannot be held"],
            "{cps}"
        );
    }
}

/// #951 review: in an inch program (G20, F in in/min) the capped F is
/// written in inch, floored so it never passes the cap - not the mm/min
/// number, which read as in/min drives A 25.4x past its stated rate. The
/// mm and the inch program post the same physical feed, block for block.
#[test]
fn an_inch_program_posts_the_capped_feed_in_inch() {
    for (cps, want) in [
        ("assets/posts/fanuc.cps", ["A30. F0.13", "A60. F39.37", "A70. F4.377"]),
        ("assets/posts/doosan-dnm.cps", ["A30. F0.13", "A60. F39.37", "A70. F4.377"]),
    ] {
        let mch = trunnion(Some(A_RATE));
        let mm = post_units(cps, &mch, STREAM, "mm");
        let inch = post_units(cps, &mch, STREAM, "inch");
        assert!(mm.warnings.is_empty() && inch.warnings.is_empty(), "{cps}: {:?} {:?}", mm.warnings, inch.warnings);
        assert!(inch.program.contains("G20"), "{cps}: {}", inch.program);
        assert_eq!(assert_within_in(&inch.program, Reading::Path, A_RATE, 25.4), 5, "{cps}: {}", inch.program);
        for w in want {
            assert!(inch.program.contains(w), "{cps}: want {w}: {}", inch.program);
        }
        let (fm, fi) = (feeds(&mm.program), feeds(&inch.program));
        assert_eq!(fm.len(), fi.len(), "{cps}");
        for (m, i) in fm.iter().zip(&fi) {
            // Each floored under the same cap: 0.1 mm/min, 0.001 in/min,
            // and an inch move's posted length a little shorter.
            assert!((m - i * 25.4).abs() <= 0.15_f64.max(m * 0.01), "{cps}: F{m} mm/min against F{i} in/min");
        }
    }
}

/// #951 review: a feed that turns C (A held) is capped at C's stated
/// rate like one that turns A - the cap reads every rotary letter.
#[test]
fn a_feed_turning_c_alone_holds_the_stated_c_rate() {
    let c_rate = 500.0;
    let mch = json!({ "name": "TEST AC TRUNNION", "feedRateMax": FEED_MAX, "kinematics": { "static": { "rotaries": [
        { "letter": "A", "type": "rotary", "direction": [1.0, 0.0, 0.0], "travel": [-120.0, 120.0], "speeds": { "normal": A_RATE } },
        { "letter": "C", "type": "rotary", "direction": [0.0, 0.0, 1.0], "speeds": { "normal": c_rate } } ] } } });
    let stream = "\
PARAM ClearancePlane 5
TOOL 1 10MM BALL
PARAM SpindleSpeed 6000
SECTION
Rapid X 10000 Y 0 Z 5000 A 30000 B 0 C 0
Feed X 10000 Y 0 Z 0 F 2000000 A 30000 B 0 C 0
Feed X 10100 Y 0 Z 0 F 2000000 A 30000 B 0 C 40000
ENDSECTION
";
    for cps in ["assets/posts/fanuc.cps", "assets/posts/doosan-dnm.cps"] {
        let out = post(cps, &mch, stream);
        assert!(out.warnings.is_empty(), "{cps}: {:?}", out.warnings);
        // 0.1 mm of tip for 40 degrees of C: 500 * 0.1 / 40 = 1.25 -> F1.2.
        assert!(out.program.contains("C40. F1.2"), "{cps}: {}", out.program);
    }
}

/// #951 review: the "under 0.1 mm/min" refusal is judged in mm before the
/// F is converted, so an inch program posts a slow capped feed the mm one
/// posts (not refusing it as under 0.1 in/min), and refuses what the mm
/// one refuses.
#[test]
fn the_slow_feed_refusal_is_judged_in_mm_in_both_units() {
    for cps in ["assets/posts/fanuc.cps", "assets/posts/doosan-dnm.cps"] {
        // 300 deg/min over 0.1 mm of tip for 30 degrees: 1 mm/min.
        let mch = trunnion(Some(300.0));
        let mm = post_units(cps, &mch, STREAM, "mm");
        let inch = post_units(cps, &mch, STREAM, "inch");
        assert!(mm.warnings.is_empty() && inch.warnings.is_empty(), "{cps}: {:?} {:?}", mm.warnings, inch.warnings);
        assert!(mm.program.contains("A30. F1.\n") || mm.program.contains("A30. F1.\r"), "{cps}: {}", mm.program);
        // The inch move posts 0.0039 in (0.09906 mm): 0.9906 mm/min.
        assert!(inch.program.contains("A30. F0.039"), "{cps}: {}", inch.program);
        // 10 deg/min: 0.033 mm/min, refused by name in both units.
        let slow = trunnion(Some(10.0));
        for u in ["mm", "inch"] {
            let out = post_units(cps, &slow, STREAM, u);
            assert_eq!(
                out.warnings,
                vec!["a feed that turns a rotary over too short a move - the stated rotary rate leaves under 0.1 mm/min"],
                "{cps} {u}: {}",
                out.program
            );
        }
    }
}

/// The A-turning feed words of `program`, in the program's own unit.
fn a_feeds(program: &str) -> Vec<f64> {
    program
        .lines()
        .filter(|l| (l.contains("G01") || l.contains("G02") || l.contains("G03")) && word(l, 'A').is_some())
        .filter_map(|l| word(l, 'F'))
        .collect()
}

#[test]
fn a_capped_rotary_feed_is_posted_in_the_programs_unit() {
    // The review's blocker: an inch program posted the capped F in mm/min
    // (25.4x the cap). Mm and inch must post the same physical feed.
    for cps in ["assets/posts/fanuc.cps", "assets/posts/doosan-dnm.cps"] {
        let mch = trunnion(Some(A_RATE));
        let opts = if cps.contains("doosan") { let mut o = brake(); o["Units"] = json!("inch"); o } else { json!({ "Units": "inch" }) };
        let mm = post(cps, &mch, STREAM);
        let inch = engine::run_with(&read(cps), &mch, STREAM, &opts).expect("post");
        assert!(inch.warnings.is_empty(), "{cps}: {:?}", inch.warnings);
        assert!(inch.program.contains("G20"), "{cps}: {}", inch.program);
        let (m, i) = (a_feeds(&mm.program), a_feeds(&inch.program));
        assert_eq!(m.len(), i.len(), "{cps}: {}", inch.program);
        let mut capped = 0;
        for (fm, fi) in m.iter().zip(&i) {
            if (fm - 2000.0).abs() < 1e-9 {
                continue; // under the rate: the op's F, converted by ff()
            }
            capped += 1;
            let back = fi * 25.4;
            // The mm word is itself floored to 0.1, so the true cap is within 0.1 above it; the inch word floors to 0.001 in/min (0.0254 mm).
            // The cap is judged over the move's length as posted, which an inch word rounds by up to 0.00127 mm a coordinate (1 mm of Z posts
            // as 0.0394 in = 1.00076 mm, a 0.08 % longer move at the same A rate): the band allows that, never 25.4x.
            assert!(back <= (fm + 0.1) * 1.001 + 1e-6 && back >= fm * 0.99 - 0.26, "{cps}: mm F{fm} posted as inch F{fi}: {}", inch.program);
        }
        assert!(capped >= 3, "{cps}: {}", inch.program);
        assert!(inch.program.contains("A30. F0.13"), "{cps}: {}", inch.program);
        assert!(inch.program.contains("A60. F39.37"), "{cps}: {}", inch.program);
    }
}

/// #951 review: the cap judges the move between the points the post
/// writes - origin-relative, rounded to the output step - not the world
/// ones. With the work origin off the output grid (any metric origin, in
/// inch) a one-thou move posts one step while its world coordinates
/// round to none, so the cap read "tip still" and let F stand over a
/// 0.0001 in tip move.
#[test]
fn a_work_offset_off_the_output_grid_does_not_hide_a_short_move_from_the_cap() {
    let stream = "\
PARAM ClearancePlane 5
TOOL 1 10MM BALL
PARAM SpindleSpeed 6000
PARAM OriginX 1
SECTION
Rapid X 10000 Y 0 Z 5000 A 0 B 0 C 0
Feed X 10000 Y 0 Z 0 F 2000000 A 0 B 0 C 0
Feed X 10001 Y 0 Z 0 F 2000000 A 5000 B 0 C 0
ENDSECTION
";
    for cps in ["assets/posts/fanuc.cps", "assets/posts/doosan-dnm.cps"] {
        let mch = trunnion(Some(A_RATE));
        let inch = post_units(cps, &mch, stream, "inch");
        assert!(inch.warnings.is_empty(), "{cps}: {:?}", inch.warnings);
        assert!(inch.program.contains("G20"), "{cps}: {}", inch.program);
        // The origin-relative X posts as one 0.0001 in step: 5 degrees
        // over it caps F far under the 2000 mm/min the op asked.
        assert_eq!(assert_within_in(&inch.program, Reading::Path, A_RATE, 25.4), 1, "{cps}: {}", inch.program);
        let f = a_feeds(&inch.program)[0] * 25.4;
        assert!(f < 100.0, "{cps}: F {f} mm/min is no cap: {}", inch.program);
    }
}

/// #951 review: an F the cap leaves alone is still judged by the word
/// `ff()` writes, which rounds to a whole mm/min: 999.6 over 2.999 mm
/// with 3 degrees of A is exactly at the 1000 deg/min limit, and the
/// rounded F1000 would run A at 1000.33. The word posted never passes it.
#[test]
fn a_rounded_uncapped_feed_never_posts_above_the_cap() {
    let stream = "\
PARAM ClearancePlane 5
TOOL 1 10MM BALL
PARAM SpindleSpeed 6000
SECTION
Rapid X 10000 Y 0 Z 5000 A 0 B 0 C 0
Feed X 10000 Y 0 Z 0 F 999600 A 0 B 0 C 0
Feed X 12999 Y 0 Z 0 F 999600 A 3000 B 0 C 0
ENDSECTION
";
    for cps in ["assets/posts/fanuc.cps", "assets/posts/doosan-dnm.cps"] {
        for u in ["mm", "inch"] {
            let mch = trunnion(Some(A_RATE));
            let out = post_units(cps, &mch, stream, u);
            assert!(out.warnings.is_empty(), "{cps} {u}: {:?}", out.warnings);
            let mm = if u == "inch" { 25.4 } else { 1.0 };
            // The limit over the length as posted (the X words read back),
            // strict: no tolerance beyond float noise.
            let xs: Vec<f64> = out.program.lines().filter(|l| l.contains("G01")).filter_map(|l| word(l, 'X')).collect();
            let f = a_feeds(&out.program)[0] * mm;
            let lim = A_RATE * ((xs[1] - xs[0]) * mm) / 3.0;
            assert!(f <= lim + 1e-6, "{cps} {u}: F{f} over the cap {lim}: {}", out.program);
        }
    }
}
