//! #1596: after every Tormach PathPilot mill tool change (`G53 G0 Z0`,
//! `G30`, `T.. M6`, `G43 H..`) Z goes straight up again in machine
//! coordinates (`G53 G0 Z0`, or the machine's stated safe Z, #495) before
//! the section's first X/Y or A move. A bare G30 leaves Z at the stored
//! G30 height; one stored below the part top (the user's PathPilot Hub
//! run, machine Z -1.5611) would otherwise carry the tool sideways at that
//! height into the part or the vise.
//! Provenance: our own tests over our own post and machine files; the
//! G30 behaviour from the public LinuxCNC G-code reference.
//!
//! Gate map (M4.4): covers post::post (tormach.cps startPreamble / liftZ),
//! app::posting (MACHINES registry).

use camcraft_app::machine::{Kind, kind, parse_mch};
use camcraft_app::post::{engine, machine};
use camcraft_app::posting::MACHINES;
use serde_json::{Value, json};
use std::path::Path;

/// Two tools, so the program has its first tool change and a second one.
const STREAM: &str = "PARAM SpindleSpeed 2000\nPARAM ClearancePlane 10\nTOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n\
                      PARAM SpindleSpeed 2000\nPARAM ClearancePlane 10\nTOOL 2 T2\nSECTION\nRapid X 20000 Y 0 Z 5000\nFeed X 21000 Y 0 Z -1000 F 500000\nENDSECTION\n";

/// One operation indexed to A 90 on the 4th-axis machine.
const INDEXED: &str = "PARAM SpindleSpeed 2000\nPARAM ClearancePlane 10\nTOOL 1 T1\nSECTION\n\
                       Rapid X 0 Y -10000 Z 0 A 90000 B 0 C 0\n\
                       Feed X 0 Y 1000 Z 0 F 500000 A 90000 B 0 C 0\n\
                       Feed X 10000 Y 1000 Z 0 F 500000 A 90000 B 0 C 0\n\
                       Rapid X 10000 Y -10000 Z 0 A 90000 B 0 C 0\nENDSECTION\n";

fn cps() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts/tormach.cps");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn doc_units(m: &Value) -> Value {
    let inch = m.get("units").and_then(Value::as_str).is_some_and(|u| u.starts_with("inch"));
    json!({ "Units": if inch { "inch" } else { "mm" } })
}

fn post(text: &str, stream: &str) -> String {
    let m = machine::mirror(&parse_mch(text).unwrap_or_else(|e| panic!("parses: {e}")));
    let out = engine::run_with(&cps(), &m, stream, &doc_units(&m)).unwrap_or_else(|e| panic!("posts: {e}"));
    assert!(out.warnings.is_empty(), "{:?}\n{}", out.warnings, out.program);
    out.program
}

fn tormach_mills() -> Vec<(&'static str, &'static str)> {
    let mills: Vec<_> = MACHINES
        .iter()
        .copied()
        .filter(|(name, text)| {
            name.starts_with("Tormach")
                && parse_mch(text).is_ok_and(|m| matches!(kind(Some(&m)), Kind::Mill | Kind::MillTurn))
        })
        .collect();
    assert!(mills.len() >= 2, "the shipped Tormach mills: {mills:?}");
    mills
}

/// A line that moves the machine in X, Y or A (a G53 lift moves Z only).
fn moves_sideways(l: &str) -> bool {
    let w: Vec<&str> = l.split_whitespace().collect();
    w.first().is_some_and(|g| ["G0", "G1", "G2", "G3"].contains(g))
        && w.iter().any(|x| x.starts_with('X') || x.starts_with('Y') || x.starts_with('A'))
}

/// For each `T.. M6`: the line after its `G43 H..` is the lift `want`,
/// and nothing between the tool change and that lift moved X, Y or A.
/// Returns the index of each lift.
fn lifts_after_tool_changes(nc: &str, want: &str, what: &str) -> Vec<usize> {
    let lines: Vec<&str> = nc.lines().collect();
    let changes: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].ends_with(" M6")).collect();
    assert!(!changes.is_empty(), "{what}: no tool change\n{nc}");
    changes
        .into_iter()
        .map(|i| {
            assert!(
                lines.get(i + 1).is_some_and(|l| l.starts_with("G43 H")),
                "{what}: G43 after {}\n{nc}",
                lines[i]
            );
            assert_eq!(lines.get(i + 2).copied(), Some(want), "{what}: Z up after the G43 of {}\n{nc}", lines[i]);
            i + 2
        })
        .collect()
}

/// Every shipped Tormach mill lifts Z to `G53 G0 Z0` right after each
/// tool change's `G43` - the program's first one and a later one - and
/// before any X/Y move of the section.
#[test]
fn every_tormach_mill_tool_change_lifts_z_before_moving_xy() {
    for (name, text) in tormach_mills() {
        let nc = post(text, STREAM);
        let lines: Vec<&str> = nc.lines().collect();
        let lifts = lifts_after_tool_changes(&nc, "G53 G0 Z0", name);
        assert_eq!(lifts.len(), 2, "{name}\n{nc}");
        for at in lifts {
            // The G30 still precedes the tool change (PathPilot's convention).
            assert!(lines[..at].iter().rev().take(6).any(|l| *l == "G30"), "{name}: G30 kept\n{nc}");
            // The section's first sideways move comes after the lift.
            let next = lines[at..].iter().position(|l| moves_sideways(l));
            assert!(next.is_some(), "{name}: the section moves\n{nc}");
        }
        // Between each G30 and the next sideways move there is a lift.
        for (g, _) in lines.iter().enumerate().filter(|(_, l)| **l == "G30") {
            let end = lines[g..].iter().position(|l| moves_sideways(l)).map_or(lines.len(), |p| g + p);
            assert!(
                lines[g + 1..end].iter().any(|l| l.starts_with("G53 G0 Z")),
                "{name}: no Z lift between the G30 at line {} and the next X/Y move\n{nc}",
                g + 1
            );
        }
    }
}

/// A Tormach file stating its own non-zero safe Z (#495) lifts to that
/// Z after the tool change.
#[test]
fn a_stated_safe_z_is_the_lift_after_the_tool_change() {
    let (name, text) = tormach_mills()[0];
    let stated = text.replacen("\"safe_retract\": 0.0", "\"safe_retract\": 2.0", 1);
    assert_ne!(stated, text, "{name} states safe_retract 0.0");
    let nc = post(&stated, STREAM);
    assert_eq!(lifts_after_tool_changes(&nc, "G53 G0 Z2", name).len(), 2, "{name}\n{nc}");
}

/// On the 4th-axis machine the lift after the tool change comes before
/// the section's A index as well as before its X/Y.
#[test]
fn a_fourth_axis_section_lifts_z_before_the_a_index() {
    let (name, text) = MACHINES
        .iter()
        .copied()
        .find(|(n, _)| *n == "Tormach 770MX + microARC 4")
        .unwrap_or_else(|| panic!("the microARC 4 machine ships"));
    let nc = post(text, INDEXED);
    let lines: Vec<&str> = nc.lines().collect();
    let a = lines.iter().position(|l| *l == "G0 A90").unwrap_or_else(|| panic!("{name}: the job indexes\n{nc}"));
    let lifts = lifts_after_tool_changes(&nc, "G53 G0 Z0", name);
    assert_eq!(lifts.len(), 1, "{name}\n{nc}");
    assert!(lifts[0] < a, "{name}: the lift after G43 comes before the A index\n{nc}");
    let first_sideways = lines.iter().position(|l| moves_sideways(l)).unwrap_or(lines.len());
    assert_eq!(first_sideways, a, "{name}: A is the first sideways move\n{nc}");
}
