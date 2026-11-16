//! #1359: every spindle mill dialect refuses, by name, a feed, arc or
//! canned cycle with the spindle stopped (no spindle start since the last
//! stop) - the rule #1357 gave the Tormach post, ported to the rest. A
//! touch-probe section (a stated 0 rpm on a touch-probe tool, #574) is
//! exempt; a cutter whose speed reads 0 is not.
//! Provenance: our own tests over our own posts and stream format.
//!
//! Gate map (M4.4): covers post::post (fanuc.cps, haas.cps, sinumerik.cps,
//! mazak.cps, heidenhain.cps, doosan-dnm.cps, grblhal.cps, tinyg.cps,
//! carvera.cps).

use camcraft_app::post::engine;
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

/// The spindle mill dialects (Tormach is pinned in
/// `post_cycle_feed_spindle.rs`) and whether each writes canned cycles
/// (tinyg and carvera drill with plain feeds, so a hole's refusal is its
/// feed move's).
const POSTS: &[(&str, bool)] = &[
    ("fanuc", true),
    ("haas", true),
    ("sinumerik", true),
    ("mazak", true),
    ("heidenhain", true),
    ("doosan-dnm", true),
    ("grblhal", true),
    ("tinyg", false),
    ("carvera", false),
];

const STOPPED: &str = "with the spindle stopped";

fn post(stem: &str, stream: &str) -> engine::PostOutput {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    engine::run(&cps, &vmc(), stream).unwrap_or_else(|e| panic!("{stem}: {e}"))
}

fn refused(stem: &str, out: &engine::PostOutput, what: &str) {
    assert!(
        out.warnings.iter().any(|w| w.contains(what) && w.contains(STOPPED)),
        "{stem}: must refuse by name ({what} ... {STOPPED}), got {:?}\n{}",
        out.warnings,
        out.program
    );
}

/// A section with the caller's spindle and tool-kind records, a plunge
/// and an arc. #1616: it states its clearance, as `post::dump` does.
fn section(head: &str) -> String {
    format!(
        "{head}PARAM ClearancePlane 10
TOOL 2 6MM END MILL
SECTION
Rapid 10000 0 5000
Feed 10000 0 -1000 F300000
Arc 0 10000 -1000 F500000 I0 J0 S1
Rapid 0 10000 5000
ENDSECTION
"
    )
}

#[test]
fn a_feed_with_no_spindle_start_is_refused_by_name_in_every_dialect() {
    for (stem, _) in POSTS {
        let out = post(stem, &section(""));
        refused(stem, &out, "feed move G");
        // Once per program, and the cut is not posted.
        assert_eq!(out.warnings.iter().filter(|w| w.contains(STOPPED)).count(), 1, "{stem}: {:?}", out.warnings);
        // The same section with its spindle stated posts clean.
        let out = post(stem, &section("PARAM SpindleSpeed 6000\n"));
        assert!(out.warnings.is_empty(), "{stem}: a running spindle posts: {:?}\n{}", out.warnings, out.program);
    }
}

#[test]
fn an_arc_with_no_spindle_start_is_refused_by_name_in_every_dialect() {
    let arc_first = section("").replace("Feed 10000 0 -1000 F300000\n", "");
    for (stem, _) in POSTS {
        refused(stem, &post(stem, &arc_first), "arc move G");
    }
}

#[test]
fn a_cutter_stating_zero_rpm_is_refused_and_a_touch_probe_is_not() {
    for (stem, _) in POSTS {
        // A cutter at 0 rpm is a stopped spindle, not a probe; so is a
        // 0-rpm section that states no tool kind.
        for head in ["PARAM SpindleSpeed 0\nPARAM ToolKind Flat end mill\n", "PARAM SpindleSpeed 0\n"] {
            refused(stem, &post(stem, &section(head)), "feed move G");
        }
        // The same section on a touch probe feeds its stylus stopped.
        let probe = section("PARAM SpindleSpeed 0\nPARAM ToolKind touch probe\n");
        let out = post(stem, &probe);
        assert!(
            !out.warnings.iter().any(|w| w.contains(STOPPED)),
            "{stem}: a touch probe section is exempt, got {:?}\n{}",
            out.warnings,
            out.program
        );
        // The probe's kind is its own section's: a 0-rpm section after it
        // that states no kind is a stopped spindle again.
        let after = format!("{probe}{}", section("PARAM SpindleSpeed 0\n"));
        refused(stem, &post(stem, &after), "feed move G");
    }
}

#[test]
fn a_section_after_the_stop_that_states_no_speed_is_refused() {
    // Every section ends with the spindle stopped; a later section that
    // states no speed would cut with it stopped.
    let one = section("PARAM SpindleSpeed 6000\n");
    let two = section("");
    for (stem, _) in POSTS {
        refused(stem, &post(stem, &format!("{one}{two}")), "feed move G");
        let restated = format!("{one}{}", section("PARAM SpindleSpeed 6000\n"));
        let out = post(stem, &restated);
        assert!(out.warnings.is_empty(), "{stem}: restated, it posts: {:?}", out.warnings);
    }
}

/// A fed hole with no spindle: a cycle whose own feed is stated.
/// #1616: it states its clearance, as `post::dump` does.
const HOLE: &str = "PARAM ClearancePlane 10
TOOL 4 6MM DRILL
START 20000 10000 6000
Rapid 20000 10000 2000
Plunge 20000 10000 -10000 F120000
Rapid 20000 10000 2000
Rapid 20000 10000 6000
; CYCLE Drill X20000 Y10000 RZ2000 BZ-10000 ENTRY0 PECK0 DWELL0 F120000 AT1
";

#[test]
fn a_hole_with_no_spindle_start_is_refused_by_name_in_every_dialect() {
    for (stem, cycles) in POSTS {
        let out = post(stem, HOLE);
        let what = if !cycles {
            "feed move G"
        } else if *stem == "heidenhain" {
            "drilling cycle"
        } else {
            "canned cycle"
        };
        refused(stem, &out, what);
        let out = post(stem, &format!("PARAM SpindleSpeed 1500\n{HOLE}"));
        assert!(out.warnings.is_empty(), "{stem}: a spun drill posts: {:?}\n{}", out.warnings, out.program);
    }
}
