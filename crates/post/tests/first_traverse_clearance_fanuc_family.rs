//! #1633 (#1616's second half; the #1614 hazard in the Fanuc-family
//! mill posts): fanuc, haas, mazak, doosan-dnm, sinumerik and heidenhain
//! state the height of the first X/Y traverse after a tool change (and at
//! the program's start) - a Z-only rapid to the section's clearance
//! plane, else its retract plane, never below the move's own Z, then X/Y
//! alone at it - instead of one combined rapid (`G00 X20 Y10 Z-3`) from
//! wherever the tool change left the tip. A long tool on a high work
//! offset can stand below the part top there. A stream stating neither
//! plane is refused by name. Under G43.4 the height is stated after the
//! arm (the tip's work-frame Z); on Doosan's indexing path, with the
//! table still clamped.
//! Provenance: our own tests over our own posts and stream format.
//!
//! Gate map (M4.4): covers assets/posts/fanuc.cps, assets/posts/haas.cps,
//! assets/posts/mazak.cps, assets/posts/doosan-dnm.cps,
//! assets/posts/sinumerik.cps, assets/posts/heidenhain.cps,
//! camcraft_post::post::engine.

use camcraft_post::cl::{CycleKind, MoveKind};
use camcraft_post::machine::parse_mch;
use camcraft_post::post::cnc::{self, CycleRec, MoveRec, Record};
use camcraft_post::post::{engine, machine};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const POSTS: &[&str] = &["fanuc", "haas", "mazak", "doosan-dnm", "sinumerik", "heidenhain"];

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn cps(stem: &str) -> String {
    read(&format!("assets/posts/{stem}.cps"))
}

fn post(stem: &str, stream: &str) -> engine::PostOutput {
    engine::run(&cps(stem), &Value::Null, stream).unwrap_or_else(|e| panic!("{stem}: {e}"))
}

/// One block in a neutral spelling: no block number or `*` end
/// (840D, TNC), no `+` sign on a coordinate (TNC), the rapid and feed
/// words two-digit, and the TNC's G40 G90 (which ride the section's
/// first motion block) left out.
fn norm(line: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for (i, w) in line.trim().trim_end_matches('*').split_whitespace().enumerate() {
        if i == 0 && w.len() > 1 && w.starts_with('N') && w[1..].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let w = match w {
            "G0" => "G00".to_string(),
            "G1" => "G01".to_string(),
            "G2" => "G02".to_string(),
            "G3" => "G03".to_string(),
            _ => match w.char_indices().nth(1) {
                Some((1, '+')) if w.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) => {
                    format!("{}{}", &w[..1], &w[2..])
                }
                _ => w.to_string(),
            },
        };
        // #1836: the Fanuc family writes a whole length with its point
        // (`Z25.`); the neutral spelling drops it, as the 840D writes it.
        let w = match w.strip_suffix('.') {
            Some(s) if s.len() > 1 && s[1..].trim_start_matches('-').chars().all(|c| c.is_ascii_digit()) => s.to_string(),
            _ => w,
        };
        words.push(w);
    }
    let motion = words.iter().any(|w| matches!(w.as_str(), "G00" | "G01" | "G02" | "G03"));
    if motion {
        words.retain(|w| w != "G40" && w != "G90");
    }
    words.join(" ")
}

fn is_motion(l: &str) -> bool {
    let w: Vec<&str> = l.split_whitespace().collect();
    w.iter().any(|w| matches!(*w, "G00" | "G01" | "G02" | "G03")) && !w.iter().any(|w| matches!(*w, "G53" | "G28" | "G91"))
}

/// A tool change: `T.. M6` (the Fanuc family, 840D) or the TNC's tool
/// call `T1 G17 S..`.
fn is_tool_change(l: &str) -> bool {
    let w: Vec<&str> = l.split_whitespace().collect();
    w.contains(&"M6")
        || (w.first().is_some_and(|t| t.len() > 1 && t.starts_with('T') && t[1..].chars().all(|c| c.is_ascii_digit()))
            && w.contains(&"G17"))
}

/// Every normalized line of the program.
fn lines(program: &str) -> Vec<String> {
    program.lines().map(norm).collect()
}

/// The first `n` motion lines after each tool change.
fn motions_after_changes(program: &str, n: usize) -> Vec<Vec<String>> {
    let ls = lines(program);
    let at: Vec<usize> = ls.iter().enumerate().filter(|(_, l)| is_tool_change(l)).map(|(i, _)| i).collect();
    at.iter().map(|&i| ls[i..].iter().filter(|l| is_motion(l)).take(n).cloned().collect()).collect()
}

/// Two tools, each section's first move a rapid down to 2 mm at X20 Y10
/// (the #1614 shape: one `G00 X20 Y10 Z2` from wherever the change left
/// the tip).
fn two_tools(planes: &str) -> String {
    format!(
        "PARAM SpindleSpeed 6000
{planes}TOOL 1 6MM END MILL
SECTION
Rapid 20000 10000 2000
Feed 20000 10000 -1000 F500000
Feed 30000 10000 -1000 F500000
ENDSECTION
PARAM SpindleSpeed 6000
{planes}TOOL 2 3MM END MILL
SECTION
Rapid 20000 10000 2000
Feed 20000 10000 -1000 F500000
ENDSECTION
"
    )
}

fn assert_clearance_first(stem: &str, program: &str, want_z: &str) {
    let firsts = motions_after_changes(program, 3);
    assert_eq!(firsts.len(), 2, "{stem}: two tool changes\n{program}");
    for m in firsts {
        assert_eq!(
            m,
            [want_z, "G00 X20 Y10", "G00 X20 Y10 Z2"],
            "{stem}: the stated clearance, then X/Y alone, then the move\n{program}"
        );
    }
}

#[test]
fn the_first_traverse_after_a_tool_change_states_the_clearance_z_first() {
    for stem in POSTS {
        let out = post(stem, &two_tools("PARAM ClearancePlane 25.4\nPARAM RetractPlane 12.7\n"));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        assert_clearance_first(stem, &out.program, "G00 Z25.4");
    }
}

#[test]
fn the_retract_plane_serves_when_no_clearance_plane_is_stated() {
    for stem in POSTS {
        let out = post(stem, &two_tools("PARAM RetractPlane 12.7\n"));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        assert_clearance_first(stem, &out.program, "G00 Z12.7");
    }
}

#[test]
fn a_first_traverse_with_no_stated_plane_is_refused_by_name() {
    for stem in POSTS {
        let out = post(stem, &two_tools(""));
        let named: Vec<&String> = out
            .warnings
            .iter()
            .filter(|w| w.contains("first X/Y traverse") && w.contains("no clearance height stated"))
            .collect();
        assert_eq!(named.len(), 1, "{stem}: refused by name, once: {:?}\n{}", out.warnings, out.program);
        // #1633 review: the refused move is not written either - no
        // combined `G00 X20 Y10 Z2` (nor any X/Y motion of the refused
        // sections) reaches the text, whatever the caller does with the
        // warning.
        let xy_motions: Vec<String> = lines(&out.program)
            .into_iter()
            .filter(|l| is_motion(l) && l.split_whitespace().any(|w| w.starts_with('X') || w.starts_with('Y')))
            .collect();
        assert!(xy_motions.is_empty(), "{stem}: no X/Y motion at an unknown height: {xy_motions:?}\n{}", out.program);
        assert!(
            !lines(&out.program).iter().any(|l| l.contains("X20 Y10")),
            "{stem}: neither the bare traverse nor the combined rapid\n{}",
            out.program
        );
    }
}

#[test]
fn the_traverse_is_never_below_the_moves_own_height() {
    // A first rapid to 50.8 mm, above the 25.4 mm clearance: the traverse
    // rides at the rapid's own height, and the rapid is the traverse.
    let s = "PARAM SpindleSpeed 6000
PARAM ClearancePlane 25.4
TOOL 2 6MM END MILL
SECTION
Rapid 0 0 50800
Feed 10000 0 -1000 F500000
ENDSECTION
";
    for stem in POSTS {
        let out = post(stem, s);
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let m = &motions_after_changes(&out.program, 2)[0];
        assert_eq!(m, &["G00 Z50.8", "G00 X0 Y0 Z50.8"], "{stem}\n{}", out.program);
    }
}

/// A feed as the first move: the traverse at the clearance to where the
/// stream says the feed starts, then a rapid down to that start only
/// when it is lower - never a second `G00 Z` to the height the traverse
/// already stated.
#[test]
fn a_feed_first_move_rapids_down_from_the_stated_clearance() {
    let stream = |start_z: f64| {
        format!(
            "PARAM SpindleSpeed 6000
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
    for stem in POSTS {
        let out = post(stem, &stream(10.0));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let m = &motions_after_changes(&out.program, 3)[0];
        assert_eq!(m.get(..2), Some(&["G00 Z10".to_string(), "G00 X20 Y10".to_string()][..]), "{stem}\n{}", out.program);
        assert!(m.get(2).is_some_and(|l| l.starts_with("G01 ")), "{stem}: no second Z rapid\n{}", out.program);
        let out = post(stem, &stream(5.0));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let m = &motions_after_changes(&out.program, 4)[0];
        assert_eq!(m.get(..3), Some(&["G00 Z10".to_string(), "G00 X20 Y10".to_string(), "G00 Z5".to_string()][..]), "{stem}\n{}", out.program);
        assert!(m.get(3).is_some_and(|l| l.starts_with("G01 ")), "{stem}\n{}", out.program);
    }
}

fn rapid(x: f64, y: f64, z: f64) -> Record {
    Record::Move(MoveRec { kind: MoveKind::Rapid, x, y, z, feed: None, center: None, ccw: false, plane: Default::default(), abc: None })
}

/// A canned cycle as the section's first motion: the cycle would position
/// over its hole from the height the tool change left it at. The
/// traverse's height is stated first, over the hole at it, then the cycle.
#[test]
fn a_cycle_first_is_positioned_over_its_hole_at_the_stated_clearance() {
    let stream = cnc::serialize(&[
        Record::Param { name: "SpindleSpeed".into(), value: json!(3000) },
        Record::Param { name: "ClearancePlane".into(), value: json!(25.4) },
        Record::Tool { number: 3, name: "6MM DRILL".into() },
        Record::Section { begin: true },
        Record::Cycle(CycleRec {
            kind: CycleKind::Drill,
            x: 20.0,
            y: 10.0,
            r_z: 2.0,
            bottom_z: -3.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 0.0,
            first_move: 0,
            feed: Some(100.0),
        }),
        rapid(20.0, 10.0, 2.0),
        Record::Move(MoveRec {
            kind: MoveKind::Plunge,
            x: 20.0,
            y: 10.0,
            z: -3.0,
            feed: Some(100.0),
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: None,
        }),
        rapid(20.0, 10.0, 2.0),
        rapid(20.0, 10.0, 25.4),
        Record::Section { begin: false },
    ]);
    for stem in POSTS {
        let out = post(stem, &stream);
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let ls = lines(&out.program);
        let change = ls.iter().position(|l| is_tool_change(l)).unwrap_or_else(|| panic!("{stem}\n{}", out.program));
        let z = ls[change..].iter().position(|l| l == "G00 Z25.4").map(|i| change + i);
        let z = z.unwrap_or_else(|| panic!("{stem}: the stated clearance\n{}", out.program));
        assert!(!ls[change..z].iter().any(|l| is_motion(l) || l.contains("G8")), "{stem}: the height first\n{}", out.program);
        assert_eq!(ls.get(z + 1).map(String::as_str), Some("G00 X20 Y10"), "{stem}: over the hole at it\n{}", out.program);
    }
}

/// The TNC states the height in the manual's own positioning form: the
/// section's first motion block, with G40 G90 (`G00 G40 G90 Z+250*`).
#[test]
fn the_tnc_states_the_height_in_its_positioning_block_form() {
    let out = post("heidenhain", &two_tools("PARAM ClearancePlane 25.4\n"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let bodies: Vec<&str> = out.program.lines().map(|l| l.split_once(' ').map_or(l, |(_, b)| b)).collect();
    let call = bodies.iter().position(|b| b.starts_with("T1 G17")).unwrap_or_else(|| panic!("{}", out.program));
    let first = bodies[call..].iter().find(|b| b.starts_with("G00")).copied();
    assert_eq!(first, Some("G00 G40 G90 Z+25.4*"), "{}", out.program);
}

fn tilthead() -> Value {
    machine::mirror(&parse_mch(&read("assets/machines/tilthead.mch")).unwrap_or_else(|e| panic!("tilthead: {e}")))
}

/// The frame under G43.4: the post arms TCPM for the section's first
/// (tilted) record, then states the height - the tip's Z in the work
/// offset, the frame the stream's planes are in - and the X/Y traverse,
/// and only the record itself turns the rotaries, at its target. The
/// rotaries never swing during the traverse.
#[test]
fn under_g43_4_the_height_is_stated_after_the_arm_and_before_any_turn() {
    let stream = "PARAM SpindleSpeed 6000
PARAM ClearancePlane 25
TOOL 1 6MM BALL
SECTION
Rapid X 10000 Y 0 Z 5000 A 30000 B 0 C 0
Feed X 10000 Y 0 Z 0 F 300000 A 30000 B 0 C 0
ENDSECTION
";
    for stem in ["fanuc", "doosan-dnm"] {
        let out = engine::run_with(&cps(stem), &tilthead(), stream, &json!({ "rotaryBrake": true }))
            .unwrap_or_else(|e| panic!("{stem}: {e}"));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let ls = lines(&out.program);
        let arm = ls.iter().position(|l| l == "G43.4 H01").unwrap_or_else(|| panic!("{stem}: armed\n{}", out.program));
        let after: Vec<&str> = ls[arm + 1..].iter().filter(|l| is_motion(l)).take(3).map(String::as_str).collect();
        assert_eq!(after, ["G00 Z25", "G00 X10 Y0", "G00 X10 Y0 Z5 A30 B0"], "{stem}\n{}", out.program);
        let first_turn = ls.iter().position(|l| l.contains("A30")).unwrap_or(0);
        assert!(arm < first_turn, "{stem}: no rotary word before the arm\n{}", out.program);
    }
}

/// Doosan's indexing path (a 4th axis with no TCP): the height and the
/// traverse are made with the table still clamped, in the frame of the
/// record's own words; the index turns on the record (M11, the move,
/// M10), as before.
#[test]
fn on_the_doosan_index_path_the_height_is_stated_with_the_table_clamped() {
    let fourth = json!({ "name": "TEST 4TH", "kinematics": { "static": { "rotaries": [ { "letter": "A", "type": "rotary" } ] } } });
    let stream = "PARAM SpindleSpeed 6000
PARAM ClearancePlane 25
TOOL 1 6MM END MILL
SECTION
Rapid X 10000 Y 0 Z 5000 A 90000
Feed X 30000 Y 0 Z -1000 F 300000 A 90000
ENDSECTION
";
    let out = engine::run_with(&cps("doosan-dnm"), &fourth, stream, &json!({ "rotaryBrake": true }))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let ls = lines(&out.program);
    let seq: Vec<&str> =
        ls.iter().filter(|l| is_motion(l) || *l == "M10" || *l == "M11").take(5).map(String::as_str).collect();
    assert_eq!(seq, ["G00 Z25", "G00 X10 Y0", "M11", "G00 X10 Y0 Z5 A90", "M10"], "{}", out.program);
}
