//! #1616 (the #1614 hazard in the other mill posts): grblHAL, TinyG and
//! the Carvera state the height of the first X/Y traverse after a tool change
//! (and at the program's start) - a Z-only `G0 Z` to the section's
//! clearance plane, else its retract plane - instead of rapiding X/Y at
//! whatever height the tool change left the tip. A long tool on a high
//! work offset can stand below the part top there. A stream stating
//! neither plane is refused by name, and no X/Y motion follows it (#1682).
//! The Carvera states Z first right after its `M6` too: its firmware's
//! public source ends every M6 at the top of Z travel over the machine X/Y
//! it stood at before the change, not over the tool rack
//! (docs/decisions/1616-carvera-after-m6.md).
//! Provenance: our own tests over our own posts and stream format.
//!
//! Gate map (M4.4): covers assets/posts/grblhal.cps, assets/posts/tinyg.cps,
//! assets/posts/carvera.cps, camcraft_post::post::engine.

use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine};
use std::path::{Path, PathBuf};

/// Each post with a shipped machine it serves. The AltMill states its Z
/// `safe_retract`, so grblHAL lifts with `G53 G0 Z0` before T M6 (the
/// #1614 shape) whatever planes the stream states; the TinyG machine
/// states none, so `post` gives it one (a homed machine), for the same
/// lift - otherwise its lift needs the ClearancePlane and the
/// retract-plane-only stream would be refused for the lift, not the
/// traverse.
const POSTS: &[(&str, &str)] = &[
    ("assets/posts/grblhal.cps", "assets/machines/sienci-altmill-2x4.mch"),
    ("assets/posts/tinyg.cps", "assets/machines/tinyg-3axis.mch"),
    ("assets/posts/grbl.cps", "assets/machines/grbl-3axis.mch"),
    ("assets/posts/carvera.cps", "assets/machines/makera-carvera.mch"),
];

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn post(cps: &str, mch: &str, stream: &str) -> engine::PostOutput {
    let mut m = machine::mirror(&parse_mch(&read(mch)).unwrap_or_else(|e| panic!("{mch}: {e}")));
    if let Some(o) = m.as_object_mut() {
        o.entry("safeRetractZ").or_insert(serde_json::json!(0.0));
    }
    engine::run(&read(cps), &m, stream).unwrap_or_else(|e| panic!("{cps}: {e}"))
}

/// Two tools, each section's first move a rapid down to 2 mm at X20 Y10
/// (the #1614 shape: `G0 X20 Y10` then `G0 X20 Y10 Z2`).
fn two_tools(planes: &str) -> String {
    format!(
        "PARAM SpindleSpeed 6000
PARAM Coolant off
{planes}TOOL 1 6MM END MILL
SECTION
Rapid 20000 10000 2000
Feed 20000 10000 -1000 F500000
Feed 30000 10000 -1000 F500000
ENDSECTION
PARAM SpindleSpeed 6000
PARAM Coolant off
{planes}TOOL 2 3MM END MILL
SECTION
Rapid 20000 10000 2000
Feed 20000 10000 -1000 F500000
ENDSECTION
"
    )
}

/// A tool change: the `M6` block, or (#2481) the comment that names the
/// tool to load on Grbl 1.1, which has no M6 (the first tool is loaded
/// before the start, the next ones at an M0 stop).
fn is_tool_change(l: &str) -> bool {
    l.split_whitespace().any(|w| w == "M6") || l.starts_with("(LOAD T") || l.starts_with("(TOOL CHANGE - LOAD T")
}

fn is_motion(l: &str) -> bool {
    l.split_whitespace().any(|w| matches!(w, "G0" | "G1" | "G2" | "G3"))
}

/// The motion lines after the (first) tool change: the lifts around it
/// (`G53 G0 Z..`, machine coordinates; TinyG lifts again after its M0)
/// are the tool change's own business.
fn motions_after_tool_change(program: &str) -> Vec<&str> {
    program
        .lines()
        .skip_while(|l| !is_tool_change(l))
        .filter(|l| is_motion(l) && !l.starts_with("G53 "))
        .collect()
}

/// The first two motion lines after each tool change: the Z-only rapid
/// to `want_z`, then X20 Y10 alone.
fn assert_clearance_first(cps: &str, program: &str, want_z: &str) {
    let lines: Vec<&str> = program.lines().collect();
    let changes: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_tool_change(l))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(changes.len(), 2, "{cps}: two tool changes\n{program}");
    for i in changes {
        let mut motions = lines[i..].iter().filter(|l| is_motion(l) && !l.starts_with("G53 "));
        let got: Vec<&str> = motions.by_ref().take(2).copied().collect();
        assert_eq!(
            got,
            [want_z, "G0 X20 Y10"],
            "{cps}: the stated clearance and X/Y alone, before any descent\n{program}"
        );
    }
}

#[test]
fn the_first_traverse_after_a_tool_change_states_the_clearance_z() {
    for (cps, mch) in POSTS {
        let out = post(cps, mch, &two_tools("PARAM ClearancePlane 25.4\nPARAM RetractPlane 12.7\n"));
        assert!(out.warnings.is_empty(), "{cps}: {:?}", out.warnings);
        assert_clearance_first(cps, &out.program, "G0 Z25.4");
    }
}

#[test]
fn the_retract_plane_serves_when_no_clearance_plane_is_stated() {
    for (cps, mch) in POSTS {
        let out = post(cps, mch, &two_tools("PARAM RetractPlane 12.7\n"));
        assert!(out.warnings.is_empty(), "{cps}: {:?}", out.warnings);
        assert_clearance_first(cps, &out.program, "G0 Z12.7");
    }
}

#[test]
fn a_first_traverse_with_no_stated_plane_is_refused_by_name() {
    for (cps, mch) in POSTS {
        let out = post(cps, mch, &two_tools(""));
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains("first X/Y traverse") && w.contains("no clearance height")),
            "{cps}: {:?}\n{}",
            out.warnings,
            out.program
        );
        assert!(
            !out.program.lines().any(|l| l == "G0 X20 Y10"),
            "{cps}: no X/Y traverse at an unknown height\n{}",
            out.program
        );
        // #1682: nor any other motion carrying X or Y - the combined
        // `G0 X20 Y10 Z2` the refused rapid asked for, or a feed after it.
        let xy: Vec<&str> = out
            .program
            .lines()
            .filter(|l| is_motion(l) && l.split_whitespace().any(|w| w.starts_with('X') || w.starts_with('Y')))
            .collect();
        assert!(xy.is_empty(), "{cps}: no X/Y motion after the refusal: {xy:?}\n{}", out.program);
    }
}

/// #1682: whatever the section's first move is - a feed, an arc or a
/// canned cycle - a stream stating no plane is refused by name and none of
/// its X/Y motion reaches the program: the refused move is not written,
/// nor any later one while X/Y stay unknown.
#[test]
fn no_xy_motion_follows_the_refusal_whatever_the_first_move() {
    let firsts = [
        ("feed", "Feed 20000 10000 -1000 F500000\nFeed 30000 10000 -1000 F500000\n"),
        ("arc", "Arc CW X 8000 Y 0 Z -1000 F 500000 I 4000 J 0\nFeed 30000 10000 -1000 F500000\n"),
        ("cycle", "CYCLE DRILL X20000 Y10000 Z-5000 R2000 F200000\nFeed 30000 10000 -1000 F500000\n"),
    ];
    for (cps, mch) in POSTS {
        for (what, moves) in firsts {
            let s = format!("PARAM SpindleSpeed 6000\nPARAM Coolant off\nTOOL 1 6MM END MILL\nSECTION\n{moves}ENDSECTION\n");
            let out = post(cps, mch, &s);
            assert!(
                out.warnings.iter().any(|w| w.contains("first X/Y traverse") && w.contains("no clearance height")),
                "{cps} ({what} first): {:?}\n{}",
                out.warnings,
                out.program
            );
            let xy: Vec<&str> = out
                .program
                .lines()
                .filter(|l| l.split_whitespace().any(|w| w.starts_with('X') || w.starts_with('Y')))
                .collect();
            assert!(xy.is_empty(), "{cps} ({what} first): no X/Y word after the refusal: {xy:?}\n{}", out.program);
        }
    }
}

#[test]
fn the_traverse_is_never_below_the_moves_own_height() {
    // A first rapid to 50.8 mm, above the 25.4 mm clearance: the traverse
    // rides at the rapid's own height.
    let s = "PARAM SpindleSpeed 6000
PARAM Coolant off
PARAM ClearancePlane 25.4
TOOL 2 6MM END MILL
SECTION
Rapid 0 0 50800
Feed 10000 0 -1000 F500000
ENDSECTION
";
    for (cps, mch) in POSTS {
        let out = post(cps, mch, s);
        assert!(out.warnings.is_empty(), "{cps}: {:?}", out.warnings);
        let motions = motions_after_tool_change(&out.program);
        assert_eq!(motions.get(..2), Some(&["G0 Z50.8", "G0 X0 Y0"][..]), "{cps}\n{}", out.program);
    }
}

/// A feed as the first move: the traverse at the clearance, then a rapid
/// down to the stream's start height only when that is lower - never a
/// second `G0 Z` to the height the traverse already stated.
#[test]
fn a_feed_first_move_rapids_down_from_the_stated_clearance() {
    let stream = |start_z: f64| {
        format!(
            "PARAM SpindleSpeed 6000
PARAM Coolant off
PARAM ClearancePlane 10
TOOL 1 6MM END MILL
PARAM StartX 20
PARAM StartY 10
PARAM StartZ {start_z}
SECTION
Feed 20000 10000 -1000 F500000
ENDSECTION
"
        )
    };
    for (cps, mch) in POSTS {
        let pair = ["G0 Z10", "G0 X20 Y10"];
        // Starting at the clearance itself: no second Z rapid.
        let out = post(cps, mch, &stream(10.0));
        assert!(out.warnings.is_empty(), "{cps}: {:?}", out.warnings);
        let motions = motions_after_tool_change(&out.program);
        assert_eq!(motions.get(..2), Some(&pair[..]), "{cps}\n{}", out.program);
        assert!(
            motions.get(2).is_some_and(|l| l.starts_with("G1 ")),
            "{cps}: no second Z rapid to the same height\n{}",
            out.program
        );
        // Starting lower: the rapid down to the start, then the feed.
        let out = post(cps, mch, &stream(5.0));
        assert!(out.warnings.is_empty(), "{cps}: {:?}", out.warnings);
        let motions = motions_after_tool_change(&out.program);
        assert_eq!(motions.get(..3), Some(&[pair[0], pair[1], "G0 Z5"][..]), "{cps}\n{}", out.program);
    }
}

/// At the program's start with no tool change, every post (the Carvera
/// too) states the clearance Z before the first X/Y traverse.
#[test]
fn at_the_program_start_with_no_tool_change_the_clearance_z_comes_first() {
    let s = "PARAM SpindleSpeed 6000
PARAM Coolant off
PARAM ClearancePlane 25.4
SECTION
Rapid 20000 10000 2000
Feed 20000 10000 -1000 F500000
ENDSECTION
";
    for (cps, mch) in POSTS {
        let out = post(cps, mch, s);
        assert!(out.warnings.is_empty(), "{cps}: {:?}", out.warnings);
        assert!(!out.program.lines().any(|l| l.split_whitespace().any(|w| w == "M6")), "{cps}: no tool change\n{}", out.program);
        let motions: Vec<&str> = out.program.lines().filter(|l| is_motion(l) && !l.starts_with("G53 ")).collect();
        assert_eq!(motions.get(..2), Some(&["G0 Z25.4", "G0 X20 Y10"][..]), "{cps}\n{}", out.program);
    }
}

/// The Carvera's order right after `M6` rests on its firmware's public
/// source: the change ends with a machine-coordinate lift to the top of Z
/// travel and a return to the X/Y read when the M6 arrived. The script
/// must cite that where it chooses the order, and must not keep an X/Y-first
/// branch after the tool change. This pin fails if the X/Y-first branch
/// comes back, or if the evidence note is removed.
#[test]
fn the_carvera_script_cites_where_its_m6_leaves_the_spindle() {
    let cps = read("assets/posts/carvera.cps");
    assert!(!cps.contains("changedAt"), "carvera.cps: an X/Y-first branch after M6 is back");
    assert!(!cps.contains("OPEN HAZARD"), "carvera.cps: the #1614 note must be closed, not kept beside Z first");
    let note = cps.find("Z first everywhere, right after M6 T too (#1616)").unwrap_or_else(|| panic!("carvera.cps: the M6 evidence note is missing"));
    let body = cps.find("function xyFirst").unwrap_or_else(|| panic!("carvera.cps: xyFirst is missing"));
    assert!(note < body, "carvera.cps: the note must stand with the traverse comment, before xyFirst");
    let block = &cps[note..body];
    for want in ["ATCHandler::on_main_loop", "G53 G0 Z<coordinate.clearance_z>", "not left over the tool", "soft Z limit", "1616-carvera-after-m6.md"] {
        assert!(block.contains(want), "carvera.cps: the note must say {want:?}
{block}");
    }
}

/// The Carvera after a tool change between two sections: the first motion
/// after the second `M6` is the Z-only rapid to the clearance, and no X/Y
/// word is written between that `M6` and it.
#[test]
fn the_carvera_states_the_clearance_before_any_xy_after_its_m6() {
    let out = post("assets/posts/carvera.cps", "assets/machines/makera-carvera.mch", &two_tools("PARAM ClearancePlane 25.4
"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    let last_m6 = lines.iter().rposition(|l| l.split_whitespace().any(|w| w == "M6")).unwrap_or_else(|| panic!("no M6
{}", out.program));
    let first_motion = lines[last_m6..].iter().find(|l| is_motion(l)).copied();
    assert_eq!(first_motion, Some("G0 Z25.4"), "{}", out.program);
    let before: Vec<&&str> = lines[last_m6..].iter().take_while(|l| **l != "G0 Z25.4").collect();
    assert!(
        !before.iter().any(|l| l.split_whitespace().any(|w| w.starts_with('X') || w.starts_with('Y'))),
        "no X/Y word between the M6 and the clearance Z
{}",
        out.program
    );
}
