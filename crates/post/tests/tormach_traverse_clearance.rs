//! #1614: the Tormach mill post states the height of the first X/Y
//! traverse after a tool change (and at the program's start) - a `G0 Z`
//! to the section's clearance plane - instead of rapiding X/Y at
//! whatever height the top of Z travel leaves the tool tip. A PathPilot
//! Hub run with a 5" T4 and a high G54 Z zero had the tip 0.68" below
//! the part top after `G43 H4`, and the bare X/Y rapid crossed the part
//! there ("down, over, up"). A stream stating no clearance or retract
//! plane is refused by name.
//! Provenance: our own tests over our own post and stream format.
//!
//! Gate map (M4.4): covers assets/posts/tormach.cps, camcraft_post::post::engine.

use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine};
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn mx770() -> serde_json::Value {
    let rel = "assets/machines/tormach-770mx.mch";
    machine::mirror(&parse_mch(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")))
}

fn post(stream: &str) -> engine::PostOutput {
    engine::run(&read("assets/posts/tormach.cps"), &mx770(), stream).expect("runs")
}

/// A drilled hole after a tool change, as a generator dumps it: the
/// section states its clearance (25.4 mm) and hop.
fn drill(planes: &str) -> String {
    format!(
        "PARAM SpindleSpeed 1500
TOOL 4 6MM DRILL
{planes}SECTION
Rapid 20000 10000 2000
Plunge 20000 10000 -10000 F120000
Rapid 20000 10000 2000
; CYCLE Drill X20000 Y10000 RZ2000 BZ-10000 ENTRY0 PECK0 DWELL0 F120000 AT0
ENDSECTION
"
    )
}

#[test]
fn the_first_traverse_after_a_tool_change_states_the_clearance_z_first() {
    let out = post(&drill("PARAM ClearancePlane 25.4\nPARAM RetractPlane 12.7\n"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    let g43 = lines.iter().position(|l| l.starts_with("G43 ")).expect("a G43");
    let xy = lines
        .iter()
        .position(|l| *l == "G0 X20 Y10")
        .unwrap_or_else(|| panic!("the first X/Y traverse:\n{}", out.program));
    assert!(g43 < xy, "{}", out.program);
    assert_eq!(
        lines.get(xy - 1),
        Some(&"G0 Z25.4"),
        "the stated clearance right before the first X/Y traverse:\n{}",
        out.program
    );
    // Nothing between the G43 and that Z rapid moves X or Y.
    for l in &lines[g43..xy - 1] {
        assert!(!l.contains('X') && !l.contains('Y'), "an X/Y move before the clearance: {l}\n{}", out.program);
    }
}

#[test]
fn the_retract_plane_serves_when_no_clearance_plane_is_stated() {
    let out = post(&drill("PARAM RetractPlane 12.7\n"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    let xy = lines.iter().position(|l| *l == "G0 X20 Y10").expect("traverse");
    assert_eq!(lines.get(xy - 1), Some(&"G0 Z12.7"), "{}", out.program);
}

#[test]
fn a_first_traverse_with_no_stated_plane_is_refused_by_name() {
    let out = post(&drill(""));
    assert!(
        out.warnings.iter().any(|w| w.contains("first X/Y traverse") && w.contains("no clearance height")),
        "{:?}\n{}",
        out.warnings,
        out.program
    );
    // #1633's review, applied here: the refused move is not written
    // either - no bare `G0 X20 Y10`, no combined rapid, no feed, cycle
    // or probe with an X/Y word reaches the text at an unknown height.
    let xy_words: Vec<&str> = out
        .program
        .lines()
        .filter(|l| !l.trim_start().starts_with('('))
        .filter(|l| l.split_whitespace().any(|w| w.starts_with('X') || w.starts_with('Y')))
        .collect();
    assert!(xy_words.is_empty(), "no X/Y motion at an unknown height: {xy_words:?}\n{}", out.program);
    assert_eq!(
        out.warnings.iter().filter(|w| w.contains("first X/Y traverse")).count(),
        1,
        "refused once per program: {:?}",
        out.warnings
    );
}

#[test]
fn the_traverse_is_never_below_the_moves_own_height() {
    // A first rapid to 50.8 mm, above the 25.4 mm clearance: the traverse rides
    // at the rapid's own height.
    let s = "PARAM SpindleSpeed 6000
TOOL 2 6MM END MILL
PARAM ClearancePlane 25.4
SECTION
Rapid 0 0 50800
Feed 10000 0 -1000 F500000
ENDSECTION
";
    let out = post(s);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    let xy = lines.iter().position(|l| *l == "G0 X0 Y0").expect("traverse");
    assert_eq!(lines.get(xy - 1), Some(&"G0 Z50.8"), "{}", out.program);
}
