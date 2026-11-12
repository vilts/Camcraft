//! #574: a touch probe section states its spindle stopped. After the
//! probe's tool change and length offset, and before its first move,
//! every built-in mill post writes its dialect's spindle-stop word (M5;
//! MOTOFF on Mazak) - so a control whose tool change leaves the spindle
//! turning (a manual change, a same-tool continuation) still brings the
//! probe to the part stopped. A cutting section posts no such word in
//! its preamble.
//!
//! Provenance: touch-probe practice (a probe is never rotated under
//! power); RS-274 / ISO 6983 M05 spindle stop; our own synthetic program.
//!
//! Gate map (M4.4): covers post::dump, post::engine and every mill post
//! script's preamble (assets/posts/*.cps startPreamble, lathe excluded;
//! doosan-dnm since #1035).

use camcraft_app::cl::{MoveKind, PathBuilder, Toolpath};
use camcraft_app::geometry::Point3;
use camcraft_app::post::{cnc, dump, engine, Dump};
use std::path::Path;

/// Every built-in mill post and its spindle-stop word (the lathe post
/// probes nothing).
const MILL_POSTS: &[(&str, &str)] = &[
    ("fanuc", "M5"),
    ("haas", "M5"),
    ("tormach", "M5"),
    ("sinumerik", "M5"),
    ("heidenhain", "M5"),
    ("mazak", "MOTOFF"),
    // #1035: added after #574, and missed it.
    ("doosan-dnm", "M5"),
];

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

/// A plain approach and one feed: enough for a preamble to open.
fn program() -> Toolpath {
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 50.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 50.0), None);
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 25.0), None);
    b.push(MoveKind::Feed, Point3::new(10.0, 10.0, 15.0), Some(300.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 50.0), None);
    b.finish()
}

fn post(stem: &str, rpm: f64) -> String {
    let d = Dump {
        op_name: "Probe1",
        tool_number: 99,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm,
        clearance_z: 50.0,
        retract_z: 50.0,
        ..Dump::default()
    };
    let stream = cnc::serialize(&dump(&program(), &d));
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let cps = read(&format!("assets/posts/{stem}.cps"));
    engine::run(&cps, &mch, &stream).unwrap_or_else(|e| panic!("{stem}: must post: {e}")).program
}

/// A block's words, without comments, a Heidenhain `*` end or N-number.
fn words(line: &str) -> Vec<String> {
    if line.trim_start().starts_with('(') || line.trim_start().starts_with(';') {
        return Vec::new();
    }
    line.split_whitespace()
        .map(|w| w.trim_end_matches('*').to_string())
        .filter(|w| !(w.starts_with('N') && w.len() > 1 && w[1..].chars().all(|c| c.is_ascii_digit())))
        .collect()
}

/// A block that moves an axis: an X / Y / Z word with a number.
fn is_motion(ws: &[String]) -> bool {
    ws.iter().any(|w| {
        let mut c = w.chars();
        matches!(c.next(), Some('X' | 'Y' | 'Z'))
            && c.next().is_some_and(|d| d.is_ascii_digit() || matches!(d, '-' | '+' | '.'))
    })
}

/// (index of the last tool-change / length-offset block, index of the
/// first motion block after it) in the program's lines.
fn preamble_span(stem: &str, lines: &[Vec<String>]) -> (usize, usize) {
    let anchor = lines
        .iter()
        .rposition(|ws| ws.iter().any(|w| w == "T99" || w == "G43" || w == "H99" || w == "D99"))
        .unwrap_or_else(|| panic!("{stem}: no tool change / length offset block"));
    let first = (anchor + 1..lines.len())
        .find(|&i| is_motion(&lines[i]))
        .unwrap_or_else(|| panic!("{stem}: no motion after the tool change"));
    (anchor, first)
}

#[test]
fn a_probe_section_stops_its_spindle_after_the_tool_change_on_every_mill_post() {
    for &(stem, stop) in MILL_POSTS {
        let out = post(stem, 0.0);
        let lines: Vec<Vec<String>> = out.lines().map(words).collect();
        let (anchor, first) = preamble_span(stem, &lines);
        let at = (anchor + 1..first).find(|&i| lines[i].iter().any(|w| w == stop));
        assert!(at.is_some(), "{stem}: no {stop} between the tool change and the first move:\n{out}");
        // Nothing starts the spindle again after the stop.
        if let Some(at) = at {
            let restart = (at + 1..first)
                .any(|i| lines[i].iter().any(|w| matches!(w.as_str(), "M3" | "M03" | "MOTON")));
            assert!(!restart, "{stem}: the spindle restarts after {stop}:\n{out}");
        }
    }
}

#[test]
fn a_cutting_section_states_no_spindle_stop_before_its_first_move() {
    for &(stem, stop) in MILL_POSTS {
        let out = post(stem, 8000.0);
        let lines: Vec<Vec<String>> = out.lines().map(words).collect();
        let (anchor, first) = preamble_span(stem, &lines);
        let stray = (anchor + 1..first).any(|i| lines[i].iter().any(|w| w == stop));
        assert!(!stray, "{stem}: a cutting section posts {stop} before its first move:\n{out}");
    }
}
