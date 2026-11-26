//! The posted Multi-Axis Deburr program (#1894 review). An upright 3D
//! Deburr section comes before the tilted section and one comes after
//! it, on the tilt head with its default Fanuc post. The test locks the
//! posted text in one golden (retract, turn, approach, cut, retract,
//! return) and pins the two rotary laws on that program:
//! - #3421: the tilted section states its rotary pose at a safe height
//!   before its first tilted feed.
//! - #3478: the upright section after it commands the rotaries back to
//!   zero, after a retract, before its first X/Y traverse.
//!
//! A second case posts the `Axis4` mode: only A turns, B stays zero.
//!
//! Provenance: RS-274 modal rotary words; our own design notes and the
//! analytic brick fixture; no vendor material.
//!
//! Gate map (#1894): covers app::mcp (post), app::posting (the posed
//! section through the Fanuc post), app::operation::multiaxis_deburr_op,
//! the golden tests/golden/nc/multiaxis_deburr_tilthead.nc.

mod common;

use camcraft_app::geometry::{cube, Point3};
use camcraft_app::mcp::Server;
use serde_json::{json, Value};

const BEFORE: &str = "UprightBefore";
const TILTED: &str = "TiltedDeburr";
const AFTER: &str = "UprightAfter";

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn ok(&mut self, name: &str, args: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        assert!(result["isError"] != json!(true), "{name} refused: {}", result["content"][0]["text"]);
        result["structuredContent"].clone()
    }
}

/// The 40 x 40 x 10 brick, stock from the model, the tilt head and the
/// chamfer mill. Three sections: upright, tilted, upright.
///
/// `Axis5`: the 90-degree cone, and the tilted window stops at Z 9. The
/// top edges (Z 10) are outside it, so the tilted section rides only the
/// four vertical corners: every pose leans, and the section ends leaned.
/// `Axis4`: a 60-degree cone. Its flank on a 45-degree break leans the
/// axis 15 degrees, so the top edges along X turn A; the edges along Y
/// and the corners need B and are skipped with a note.
fn program(tag: &str, mode: &str) -> String {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-ma-deburr-post-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut s = String::from("solid part\n");
    for t in cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0)).facets.iter() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid part\n");
    let stl = dir.join("part.stl");
    std::fs::write(&stl, s).expect("write stl");

    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "from_model": "part" }));
    c.ok("add_tool", json!({ "catalog": "CM-0600-90" }));
    let axis4 = mode == "axis4";
    if axis4 {
        c.ok("set_tool_cutter_params", json!({ "tool": "CM-0600-90", "params": { "tip_angle": 60.0 } }));
    }
    c.ok("load_machine", json!({ "builtin": "Tilt-head 5-axis" }));
    let window = json!({ "clearance_z": 35.0, "top_z": 10.0, "bottom_z": 1.0 });
    for (name, kind) in [(BEFORE, "deburr"), (TILTED, "multi_axis_deburr"), (AFTER, "deburr")] {
        c.ok("add_operation", json!({ "kind": kind, "tool": "CM-0600-90", "name": name, "source": "part" }));
        c.ok("edit_operation", json!({ "name": name, "params": window }));
    }
    let tilted_top = if axis4 { 10.0 } else { 9.0 };
    c.ok("edit_operation", json!({ "name": TILTED, "params": { "max_tilt_deg": 45.0, "axis_mode": mode, "top_z": tilted_top } }));
    for name in [BEFORE, TILTED, AFTER] {
        c.ok("generate", json!({ "name": name }));
    }
    let out = dir.join("out.nc");
    let posted = c.ok("post", json!({ "output": out.display().to_string() }));
    assert_eq!(posted["ops"], json!(3), "{posted}");
    let nc = std::fs::read_to_string(&out).expect("the NC file is written");
    let _ = std::fs::remove_dir_all(dir);
    nc
}

fn word(line: &str, letter: char) -> Option<f64> {
    line.split_whitespace().find_map(|w| w.strip_prefix(letter).and_then(|v| v.parse::<f64>().ok()))
}

fn is_motion(l: &str) -> bool {
    l.split_whitespace().any(|w| matches!(w, "G00" | "G0" | "G01" | "G1" | "G02" | "G03"))
}

fn turns(l: &str) -> bool {
    ['A', 'B'].iter().any(|&r| word(l, r).is_some_and(|v| v.abs() > 1e-6))
}

fn is_feed(l: &str) -> bool {
    l.split_whitespace().any(|w| w == "G01" || w == "G1")
}

/// The modal rotary pose after each line: a feed does not repeat the A
/// and B words, so the pose of a line is the last stated one.
fn modal_poses(lines: &[&str]) -> Vec<(f64, f64)> {
    let (mut a, mut b) = (0.0, 0.0);
    lines
        .iter()
        .map(|l| {
            a = word(l, 'A').unwrap_or(a);
            b = word(l, 'B').unwrap_or(b);
            (a, b)
        })
        .collect()
}

fn leaned((a, b): (f64, f64)) -> bool {
    a.abs() > 1e-6 || b.abs() > 1e-6
}

/// The lines of one section: from the line that names `op` to the line
/// that names `next` (or the end).
fn section<'a>(lines: &'a [&'a str], op: &str, next: Option<&str>) -> &'a [&'a str] {
    let start = lines.iter().position(|l| l.contains(op)).unwrap_or_else(|| panic!("the program names {op}"));
    let end = next.and_then(|n| lines.iter().position(|l| l.contains(n))).unwrap_or(lines.len());
    assert!(start < end, "{op} comes before {next:?}");
    &lines[start..end]
}

#[test]
fn the_tilted_section_states_its_rotary_pose_before_its_first_tilted_feed() {
    let nc = program("pose", "axis5");
    let lines: Vec<&str> = nc.lines().collect();
    let before = section(&lines, BEFORE, Some(TILTED));
    assert!(!before.iter().any(|l| turns(l)), "the upright section before turns no rotary:\n{nc}");
    let start = lines.iter().position(|l| l.contains(TILTED)).expect("the tilted section");
    let poses = modal_poses(&lines);
    let tilted = section(&lines, TILTED, Some(AFTER));
    let feed = (0..tilted.len())
        .position(|i| is_feed(tilted[i]) && leaned(poses[start + i]))
        .unwrap_or_else(|| panic!("the tilted section cuts with a lean:\n{nc}"));
    // The pose: a rapid that states the leaned A and B of that feed,
    // before it and after the last earlier feed.
    let pose = tilted[..feed]
        .iter()
        .rposition(|l| word(l, 'A').is_some() || word(l, 'B').is_some())
        .unwrap_or_else(|| panic!("the tilted section states its pose before its first tilted feed:\n{nc}"));
    assert!(tilted[pose].starts_with("G00"), "the pose rides a rapid: {}\n{nc}", tilted[pose]);
    assert!(word(tilted[pose], 'A').is_some() && word(tilted[pose], 'B').is_some(), "both rotaries are stated: {}", tilted[pose]);
    // The rapid states the pose that the first tilted feed runs under,
    // and no feed turns a rotary: the lean is set before the tool moves in.
    assert_eq!(poses[start + pose], poses[start + feed], "the rapid states the lean of the first tilted feed:\n{nc}");
    assert!(
        !tilted[pose + 1..=feed].iter().any(|l| word(l, 'A').is_some() || word(l, 'B').is_some()),
        "no feed turns a rotary on the way in:\n{nc}"
    );
    // The retract: the tool goes up (Z only, or a machine home) before the turn.
    assert!(
        tilted[..pose].iter().chain(before.iter()).any(|l| {
            (l.starts_with("G00") && word(l, 'Z').is_some() && word(l, 'X').is_none() && word(l, 'Y').is_none())
                || l.contains("G28")
                || l.contains("G53")
        }),
        "the tool retracts before the rotaries turn:\n{nc}"
    );
}

#[test]
fn the_upright_section_after_the_tilted_section_returns_the_rotaries_first() {
    let nc = program("return", "axis5");
    let lines: Vec<&str> = nc.lines().collect();
    let start = lines.iter().position(|l| l.contains(AFTER)).expect("the upright section");
    assert!(leaned(modal_poses(&lines)[start]), "the tilted section ends leaned, so a return is owed:\n{nc}");
    let after = section(&lines, AFTER, None);
    let cut = after
        .iter()
        .position(|l| is_feed(l))
        .unwrap_or_else(|| panic!("the upright section cuts:\n{nc}"));
    let traverse = after[..cut]
        .iter()
        .position(|l| is_motion(l) && (word(l, 'X').is_some() || word(l, 'Y').is_some()))
        .unwrap_or_else(|| panic!("the upright section traverses X/Y:\n{nc}"));
    for letter in ['A', 'B'] {
        let zero = after[..traverse]
            .iter()
            .position(|l| word(l, letter).is_some_and(|v| v.abs() < 1e-9))
            .unwrap_or_else(|| panic!("the upright section commands {letter}0 before its first X/Y traverse:\n{nc}"));
        assert!(
            after[..zero].iter().any(|l| l.contains("G28") || l.contains("G53")),
            "the tool retracts to a safe height before {letter} turns back:\n{nc}"
        );
    }
    // No leaned block after the return.
    assert!(!after.iter().any(|l| turns(l)), "the upright section cuts with no lean:\n{nc}");
}

#[test]
fn the_three_section_tilthead_program_matches_its_golden() {
    let nc = program("golden", "axis5");
    common::write_golden("nc/multiaxis_deburr_tilthead.nc", &nc);
}

#[test]
fn axis4_posts_with_the_a_axis_only() {
    let nc = program("axis4", "axis4");
    let lines: Vec<&str> = nc.lines().collect();
    let tilted = section(&lines, TILTED, Some(AFTER));
    assert!(
        tilted.iter().any(|l| word(l, 'A').is_some_and(|v| v.abs() > 1e-6)),
        "the 4-axis ride turns A:\n{nc}"
    );
    assert!(
        !lines.iter().any(|l| word(l, 'B').is_some_and(|v| v.abs() > 1e-6)),
        "the 4-axis ride keeps B at zero:\n{nc}"
    );
}
