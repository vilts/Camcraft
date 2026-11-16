//! #1357: a post refuses, by name, a canned cycle the stream gives no
//! feed (LinuxCNC alarms "Cannot feed with zero feed rate"; a control
//! with an earlier F drills at whatever feed was last modal), and the
//! Tormach mill post refuses a cut with the spindle stopped. Found in a
//! PathPilot Hub run of the old Tormach drill_peck golden: a G73 with
//! no F, and T0 with the spindle never started.
//! Provenance: our own tests over our own posts and stream format.
//!
//! Gate map (M4.4): covers post::post (fanuc.cps, haas.cps, sinumerik.cps,
//! mazak.cps, doosan-dnm.cps, grblhal.cps, heidenhain.cps, tormach.cps,
//! fanuc-lathe.cps).

mod common;

use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use common::legacy_program::{as_program, with_cycle_feeds};
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn vmc() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json")
}

fn mx770() -> serde_json::Value {
    let rel = "assets/machines/tormach-770mx.mch";
    machine::mirror(&parse_mch(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")))
}

/// The mill posts that write canned cycles, with the words their
/// no-feed refusal is named by.
const CYCLE_POSTS: &[(&str, &str)] = &[
    ("fanuc", "has no feed rate"),
    ("haas", "has no feed rate"),
    ("sinumerik", "has no feed rate"),
    ("mazak", "has no feed rate"),
    ("doosan-dnm", "has no feed rate"),
    ("grblhal", "has no feed rate"),
    ("tormach", "has no feed rate"),
    ("heidenhain", "positive cutting feed"),
];

/// A drilled hole whose cycle and chain state no feed at all: a tool,
/// a spindle and a stated R, nothing else missing.
const NO_FEED: &str = "PARAM SpindleSpeed 1500
PARAM ClearancePlane 10
TOOL 4 6MM DRILL
START 20000 10000 6000
Rapid 20000 10000 2000
Plunge 20000 10000 -10000
Rapid 20000 10000 2000
Rapid 20000 10000 6000
; CYCLE Drill X20000 Y10000 RZ2000 BZ-10000 ENTRY0 PECK0 DWELL0 AT1
";

#[test]
fn a_canned_cycle_with_no_feed_is_refused_by_name() {
    for (stem, why) in CYCLE_POSTS {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        let out = engine::run(&cps, &vmc(), NO_FEED).unwrap_or_else(|e| panic!("{stem}: {e}"));
        assert!(
            out.warnings.iter().any(|w| w.contains(why)),
            "{stem}: a cycle with no feed must be refused by name ({why}), got {:?}\n{}",
            out.warnings,
            out.program
        );
    }
    // The same hole with its feed stated posts clean (heidenhain reads
    // the chain's; the rest the cycle's own F).
    let fed = NO_FEED.replace("Plunge 20000 10000 -10000", "Plunge 20000 10000 -10000 F120000");
    let fed = with_cycle_feeds(&fed);
    assert!(fed.contains("DWELL0 F120000 AT1"), "{fed}");
    for (stem, _) in CYCLE_POSTS {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        let out = engine::run(&cps, &vmc(), &fed).unwrap_or_else(|e| panic!("{stem}: {e}"));
        let refused: Vec<_> = out.warnings.iter().filter(|w| w.contains("feed")).collect();
        assert!(refused.is_empty(), "{stem}: a fed cycle posts: {refused:?}");
    }
}

#[test]
fn the_lathe_drill_cycle_with_no_feed_is_refused_by_name() {
    // On centre (X0 Y0), so only the feed is wrong.
    let stream = "PARAM SpindleSpeed 800
PARAM ClearancePlane 10
TOOL 1 10MM DRILL
START 0 0 6000
Rapid 0 0 2000
Plunge 0 0 -10000
Rapid 0 0 2000
; CYCLE Drill X0 Y0 RZ2000 BZ-10000 ENTRY0 PECK0 DWELL0 AT1
";
    let mch: serde_json::Value = serde_json::from_str(&read("assets/machines/lathe.mch")).expect("lathe json");
    let cps = read("assets/posts/fanuc-lathe.cps");
    let out = engine::run(&cps, &mch, stream).expect("runs");
    assert!(
        out.warnings.iter().any(|w| w.contains("has no feed rate")),
        "fanuc-lathe: {:?}\n{}",
        out.warnings,
        out.program
    );
}

#[test]
fn a_tormach_cut_before_any_spindle_start_is_refused_by_name() {
    let cps = read("assets/posts/tormach.cps");
    // A bare contour: no tool, no spindle speed - G1/G2/G3 with the
    // spindle never started.
    let bare = read("crates/app/tests/golden/contour_plate_with_hole.cl");
    let out = engine::run(&cps, &mx770(), &bare).expect("runs");
    assert!(
        out.warnings.iter().any(|w| w.contains("feed move G1") && w.contains("with the spindle stopped")),
        "a cut with no spindle start must be refused by name, got {:?}",
        out.warnings
    );
    // A bare drill whose cycle is fed: the cycle itself is refused.
    let drill = with_cycle_feeds(&read("crates/app/tests/golden/drill_peck.cl"));
    let out = engine::run(&cps, &mx770(), &drill).expect("runs");
    assert!(
        out.warnings.iter().any(|w| w.contains("canned cycle G73") && w.contains("with the spindle stopped")),
        "a cycle with no spindle start must be refused by name, got {:?}",
        out.warnings
    );
    // The same paths as real programs post clean.
    for g in ["contour_plate_with_hole", "drill_peck"] {
        let s = as_program(g, &read(&format!("crates/app/tests/golden/{g}.cl")));
        let out = engine::run(&cps, &mx770(), &s).expect("runs");
        assert!(out.warnings.is_empty(), "{g}: {:?}", out.warnings);
    }
}

#[test]
fn a_tormach_section_after_m5_with_no_spindle_speed_is_refused() {
    // The first section starts the spindle; every section end writes M5.
    // A second section that states no speed would cut with it stopped.
    let cps = read("assets/posts/tormach.cps");
    let stream = "PARAM SpindleSpeed 6000
PARAM ClearancePlane 10
TOOL 2 6MM END MILL
SECTION
Rapid 0 0 5000
Feed 10000 0 -1000 F500000
ENDSECTION
SECTION
Rapid 0 10000 5000
Feed 10000 10000 -1000 F500000
ENDSECTION
";
    let out = engine::run(&cps, &mx770(), stream).expect("runs");
    assert!(
        out.warnings.iter().any(|w| w.contains("with the spindle stopped")),
        "{:?}\n{}",
        out.warnings,
        out.program
    );
    // Restated, it posts.
    let restated = stream.replacen("SECTION\nRapid 0 10000", "PARAM SpindleSpeed 6000\nPARAM ClearancePlane 10\nSECTION\nRapid 0 10000", 1);
    let out = engine::run(&cps, &mx770(), &restated).expect("runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
}

#[test]
fn the_tormach_drill_golden_is_a_runnable_program() {
    let nc = read("crates/app/tests/golden/nc/tormach/drill_peck.nc");
    let lines: Vec<&str> = nc.lines().collect();
    let at = |pred: &dyn Fn(&str) -> bool| lines.iter().position(|l| pred(l));
    let g73 = at(&|l| l.starts_with("G73 ")).expect("a G73 hole");
    assert!(
        lines[g73].split_whitespace().any(|w| w.starts_with('F') && w[1..].parse::<f64>().is_ok_and(|f| f > 0.0)),
        "the G73 states a positive feed: {}",
        lines[g73]
    );
    let m6 = at(&|l| l.starts_with('T') && l.ends_with(" M6")).expect("a tool change");
    let m3 = at(&|l| l.starts_with('S') && l.ends_with(" M3")).expect("a spindle start");
    assert!(m6 < m3 && m3 < g73, "T.. M6, then S.. M3, then the G73:\n{nc}");
    assert_eq!(lines.first(), Some(&"(Tormach 770MX)"), "posted for the 770MX");
    assert!(nc.contains("(OPERATION: "), "the operation is named:\n{nc}");
}

/// A one-section program whose tool and spindle speed the caller states,
/// with a plain feed move: what a 0-rpm section must not post as a cut.
fn zero_rpm_section(kind: &str) -> String {
    format!(
        "PARAM SpindleSpeed 0
PARAM ClearancePlane 10
PARAM ToolKind {kind}
TOOL 2 6MM END MILL
SECTION
Rapid 10000 10000 50000
Rapid 10000 10000 25000
Feed 10000 10000 15000 F300000
Rapid 10000 10000 50000
ENDSECTION
"
    )
}

#[test]
fn a_tormach_cutter_section_stating_zero_rpm_is_refused_by_name() {
    // The M3 review (#1367): the probe exemption read "0 rpm" alone, so a
    // cutting tool at 0 rpm posted M5 and then G1 F.. with no refusal. A
    // section is a probe's only when its tool is a touch probe.
    let cps = read("assets/posts/tormach.cps");
    for kind in ["end mill", "drill", "ball nose"] {
        let out = engine::run(&cps, &mx770(), &zero_rpm_section(kind)).expect("runs");
        assert!(
            out.warnings.iter().any(|w| w.contains("feed move G1") && w.contains("with the spindle stopped")),
            "{kind} at 0 rpm cutting must be refused by name, got {:?}\n{}",
            out.warnings,
            out.program
        );
        assert!(!out.program.lines().any(|l| l.starts_with("G1 ")), "{kind}: no G1 is posted:\n{}", out.program);
    }
    // A 0-rpm section that states no tool kind at all is no probe either.
    let bare = zero_rpm_section("x").replace("PARAM ToolKind x\n", "");
    let out = engine::run(&cps, &mx770(), &bare).expect("runs");
    assert!(
        out.warnings.iter().any(|w| w.contains("with the spindle stopped")),
        "0 rpm with no tool kind must be refused, got {:?}",
        out.warnings
    );
}

#[test]
fn a_tormach_touch_probe_section_still_posts() {
    // The same section on a touch probe: stopped on purpose, and posted.
    let cps = read("assets/posts/tormach.cps");
    let out = engine::run(&cps, &mx770(), &zero_rpm_section("touch probe")).expect("runs");
    assert!(out.warnings.is_empty(), "a probe section posts clean: {:?}\n{}", out.warnings, out.program);
    let lines: Vec<&str> = out.program.lines().collect();
    let m5 = lines.iter().position(|l| *l == "M5").expect("the probe states M5");
    let g1 = lines.iter().position(|l| l.starts_with("G1 ")).expect("the probe's feed posts");
    assert!(m5 < g1, "M5 before the stylus feeds:\n{}", out.program);
    assert!(!lines.iter().any(|l| l.ends_with(" M3")), "no spindle start on a probe:\n{}", out.program);
}

/// A hole drilled after an earlier fed move, whose own cycle and chain
/// state no feed: a post that falls back to the modal feed would drill
/// it at F300 from the facing pass. Refused by name instead.
const EARLIER_F_THEN_NO_FEED: &str = "PARAM SpindleSpeed 1500
PARAM ClearancePlane 10
TOOL 4 6MM DRILL
START 20000 10000 6000
Feed 30000 10000 6000 F300000
Rapid 20000 10000 2000
Plunge 20000 10000 -10000
Rapid 20000 10000 2000
Rapid 20000 10000 6000
; CYCLE Drill X20000 Y10000 RZ2000 BZ-10000 ENTRY0 PECK0 DWELL0 AT2
";

#[test]
fn a_cycle_with_no_feed_after_an_earlier_f_is_refused_by_name() {
    // The M3 review (#1367): restoring Heidenhain's modal-feed fallback
    // passed every no-feed pin, whose streams had no earlier F.
    for (stem, why) in [("heidenhain", "positive cutting feed"), ("fanuc", "has no feed rate")] {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        let out = engine::run(&cps, &vmc(), EARLIER_F_THEN_NO_FEED).unwrap_or_else(|e| panic!("{stem}: {e}"));
        assert!(
            out.warnings.iter().any(|w| w.contains(why)),
            "{stem}: an unfed cycle after an earlier F must be refused by name ({why}), got {:?}\n{}",
            out.warnings,
            out.program
        );
    }
}
