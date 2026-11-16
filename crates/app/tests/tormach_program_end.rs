//! #1594: how a Tormach PathPilot mill program ends. Z goes straight up
//! in machine coordinates (`G53 G0 Z0`, or the machine's stated safe Z,
//! #495) and the program ends there with M30: no `G30` after the final
//! retract (a bare G30 moves every axis to the stored G30 position, and a
//! low stored G30 drove the tool back down in the user's PathPilot Hub
//! run) and no rotary return. The `G30` before each tool change stays.
//! Provenance: our own tests over our own post and machine files; the
//! G30 behaviour from the public LinuxCNC G-code reference.
//!
//! Gate map (M4.4): covers post::post (tormach.cps onClose / retract),
//! app::posting (MACHINES registry).

use camcraft_app::machine::{Kind, kind, parse_mch};
use camcraft_app::post::{engine, machine};
use camcraft_app::posting::MACHINES;
use serde_json::{Value, json};
use std::path::Path;

/// Two tools, so the program has a tool change between them.
const STREAM: &str = "PARAM SpindleSpeed 2000\nPARAM ClearancePlane 10\nTOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n\
                      PARAM SpindleSpeed 2000\nPARAM ClearancePlane 10\nTOOL 2 T2\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n";

/// One operation indexed to A 90 (the part turned +90 about X, so the
/// indexed frame's (x, y, z) is the world's (x, -z, y)): the tool stands
/// 10 mm above the face, then cuts 10 mm along X, 1 mm deep.
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

/// The lines after the last `G53` (the final retract) up to the end.
fn after_final_retract(nc: &str) -> Vec<&str> {
    let lines: Vec<&str> = nc.lines().collect();
    let last = lines.iter().rposition(|l| l.starts_with("G53 ")).unwrap_or_else(|| panic!("no G53\n{nc}"));
    lines[last + 1..].to_vec()
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

/// Every shipped Tormach mill ends `G53 G0 Z0` then M30 - nothing between
/// them, G30 least of all - while each tool change keeps its Z-up + G30.
#[test]
fn every_tormach_mill_program_ends_z_up_then_m30_with_no_g30() {
    for (name, text) in tormach_mills() {
        let nc = post(text, STREAM);
        let lines: Vec<&str> = nc.lines().collect();
        let n = lines.len();
        assert_eq!(lines.get(n.saturating_sub(3)..), Some(&["M5", "G53 G0 Z0", "M30"][..]), "{name}\n{nc}");
        assert_eq!(after_final_retract(&nc), ["M30"], "{name}: no G30 after the final retract\n{nc}");
        // The tool change is out of #1594's scope: still Z up, G30, then M6.
        let changes: Vec<usize> = (0..n).filter(|&i| lines[i].ends_with(" M6")).collect();
        assert_eq!(changes.len(), 2, "{name}\n{nc}");
        for i in changes {
            assert_eq!(lines.get(i.saturating_sub(2)..i), Some(&["G53 G0 Z0", "G30"][..]), "{name}: before {}\n{nc}", lines[i]);
        }
    }
}

/// A Tormach file stating its own non-zero safe Z (#495) ends at that Z,
/// with no G30 anywhere (off the Z0 convention the G30 never rides).
#[test]
fn a_stated_safe_z_ends_at_that_z_with_no_g30() {
    let (name, text) = tormach_mills()[0];
    let stated = text.replacen("\"safe_retract\": 0.0", "\"safe_retract\": 2.0", 1);
    assert_ne!(stated, text, "{name} states safe_retract 0.0");
    let nc = post(&stated, STREAM);
    let lines: Vec<&str> = nc.lines().collect();
    let n = lines.len();
    assert_eq!(lines.get(n.saturating_sub(2)..), Some(&["G53 G0 Z2", "M30"][..]), "{name}\n{nc}");
    assert!(!lines.contains(&"G30"), "{name}\n{nc}");
}

/// On the 4th-axis machine an indexed job ends Z up and M30: the rotary
/// stays where the last operation left it - no `G0 A0` (the user's Hub
/// run: not wanted) and no G30 that would swing it.
#[test]
fn a_fourth_axis_program_ends_without_returning_a() {
    let (name, text) = MACHINES
        .iter()
        .copied()
        .find(|(n, _)| *n == "Tormach 770MX + microARC 4")
        .unwrap_or_else(|| panic!("the microARC 4 machine ships"));
    let nc = post(text, INDEXED);
    assert!(nc.lines().any(|l| l == "G0 A90"), "{name}: the job indexes\n{nc}");
    assert_eq!(after_final_retract(&nc), ["M30"], "{name}: no G0 A / G30 at the end\n{nc}");
}
