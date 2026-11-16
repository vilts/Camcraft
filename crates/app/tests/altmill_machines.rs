//! #728: Sienci's AltMill routers (2x4, 4x4, 4x8) - each machine file
//! states its figures, and the grblHAL post speaks them: the homed lift
//! `G53 G0 Z0` before every `T<n> M6` (the sender or the ATC takes the
//! change from there), mm programs, and a spindle speed past the
//! machine's maximum refused.
//! Provenance: our own tests over our own machine files and post. The
//! figures are Sienci's published spec pages ("4x4 & 2x4 Specifications",
//! "AltMill 4x8 Specifications", resources.sienci.com, read 2026-11-10),
//! and for the 4x4 the soft-limit travel, axis rates, homing and spindle
//! range read from an AltMill 4x4's own SLB-EXT (`$$`, 2026-11-10).
//! See docs/decisions/728-altmill.md.
//!
//! Gate map (#728): covers app::posting (MACHINES registry),
//! post::machine (safeRetractZ, units), camcraft_post::post (grblhal.cps).

use camcraft_app::machine::{Machine, parse_mch};
use camcraft_app::post::{engine, machine};
use camcraft_app::posting::MACHINES;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Two tools, a cut and a drill cycle: enough to show the lift, the
/// change and the program's shape.
const STREAM: &str = "PARAM ClearancePlane 15\nPARAM StockZTop 0\nPARAM SpindleSpeed 18000\nTOOL 3 EM6\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 10000 Y 0 Z -1000 F 1500000\nENDSECTION\n\
                      PARAM ClearancePlane 15\nPARAM SpindleSpeed 12000\nTOOL 4 DRILL5\nSECTION\nCYCLE DRILL X 20000 Y 5000 Z -3000 R 2000 P 1500 F 300000\nENDSECTION\n";

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn shipped(name: &str) -> Machine {
    let text = MACHINES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
        .unwrap_or_else(|| panic!("{name} is not a shipped machine"));
    parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn mirrored(name: &str) -> Value {
    machine::mirror(&shipped(name))
}

fn post_with(m: &Value, stream: &str, options: &Value) -> engine::PostOutput {
    engine::run_with(&read("assets/posts/grblhal.cps"), m, stream, options).unwrap_or_else(|e| panic!("posts: {e}"))
}

fn post(m: &Value, stream: &str) -> engine::PostOutput {
    engine::run(&read("assets/posts/grblhal.cps"), m, stream).unwrap_or_else(|e| panic!("posts: {e}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// One machine's figures, mm and mm/min.
struct Spec {
    name: &'static str,
    travel: [f64; 3],
    speed: [f64; 3],
}

const SPECS: [Spec; 3] = [
    // Sienci's spec page: cutting area 1265 x 635 x 174 mm; 15000 mm/min
    // X and Y, 6000 Z.
    Spec { name: "Sienci AltMill 2x4", travel: [1265.0, 635.0, 174.0], speed: [15000.0, 15000.0, 6000.0] },
    // The controller's soft limits ($130-$132: 1260, 1248, 170), inside
    // the page's 1265 x 1251 x 174 cutting area; $110-$112 match the page.
    Spec { name: "Sienci AltMill 4x4", travel: [1260.0, 1248.0, 170.0], speed: [15000.0, 15000.0, 6000.0] },
    // Sienci's 4x8 page: 1265 x 2460 x 210 mm; Y 25000 mm/min (rack and
    // pinion), X 590 IPM = 15000 mm/min (the page's "16,000" disagrees
    // with its own IPM; the lower is taken), Z 6000.
    Spec { name: "Sienci AltMill 4x8", travel: [1265.0, 2460.0, 210.0], speed: [15000.0, 25000.0, 6000.0] },
];

#[test]
fn every_altmill_states_its_figures() {
    for s in &SPECS {
        let n = s.name;
        let m = shipped(n);
        assert_eq!(m.model, n);
        assert_eq!(m.units, None, "{n}: mm (the parser records only inch) - the SLB-EXT runs G21");
        assert_eq!(m.axes.len(), 3, "{n}: X, Y, Z - the Vortex rotary waits on #714");
        for (a, want) in m.axes.iter().zip(s.travel) {
            // The full stroke either way from the work zero, as every
            // shipped machine writes it.
            let (lo, hi) = a.travel.unwrap_or_else(|| panic!("{n} {} travel", a.name));
            assert!(close(hi, want) && close(lo, -want), "{n} {}: {lo}..{hi}", a.name);
        }
        for (a, want) in m.axes.iter().zip(s.speed) {
            assert!(a.feed.is_some_and(|f| close(f, want)), "{n} {} speed {:?}", a.name, a.feed);
        }
        assert_eq!((m.spindle_min, m.spindle_max), (Some(1200.0), Some(24000.0)), "{n} rpm");
        assert_eq!(m.spindle_power, None, "{n}: the spindle is an add-on, its power unstated");
        assert_eq!(m.safe_retract_z, Some(0.0), "{n} homes: machine Z0 is the homed top");
        let codes: Vec<(&str, &str)> = m.coolants.iter().map(|c| (c.name.as_str(), c.m_code.as_str())).collect();
        assert_eq!(codes, [("Flood", "M8"), ("Mist", "M7")], "{n} coolant");
    }
}

#[test]
fn every_tool_change_lifts_to_the_homed_top_then_stands_alone() {
    for s in &SPECS {
        let out = post(&mirrored(s.name), STREAM);
        assert!(out.warnings.is_empty(), "{}: {:?}", s.name, out.warnings);
        let nc = out.program;
        let lines: Vec<&str> = nc.lines().collect();
        assert_eq!(lines.first().copied(), Some(format!("({})", s.name).as_str()), "{nc}");
        assert!(lines.get(1).is_some_and(|l| l.starts_with("G21 ")), "mm\n{nc}");
        for call in ["T3 M6", "T4 M6"] {
            let at = lines.iter().position(|l| *l == call).unwrap_or_else(|| panic!("no {call:?}\n{nc}"));
            assert_eq!(lines[at - 1], "G53 G0 Z0", "{}: up to the homed top first\n{nc}", s.name);
        }
        assert_eq!(nc.matches("G53 G0 Z0\n").count(), 3, "two changes and the end\n{nc}");
        for w in nc.split_whitespace() {
            assert!(!["G43", "G43.1", "G49"].contains(&w), "the controller's measured length is kept\n{nc}");
        }
    }
}

#[test]
fn a_spindle_speed_past_the_maximum_is_refused() {
    let stream = STREAM.replacen("PARAM SpindleSpeed 18000", "PARAM SpindleSpeed 30000", 1);
    let out = post(&mirrored("Sienci AltMill 4x4"), &stream);
    assert!(out.warnings.iter().any(|w| w.contains("spindle rpm over machine maximum")), "{:?}", out.warnings);
}

/// The two-tool program on the 4x4, locked byte-exact in mm and in inch
/// (the document's `Units: inch`, #902), so a later post change shows up as a diff.
#[test]
fn the_4x4_two_tool_program_is_locked() {
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    for (m, file) in [(json!({}), "altmill_4x4_two_tool"), (json!({"Units": "inch"}), "altmill_4x4_two_tool_inch")] {
        let out = post_with(&mirrored("Sienci AltMill 4x4"), STREAM, &m);
        assert!(out.warnings.is_empty(), "{file}: {:?}", out.warnings);
        let rel = format!("crates/app/tests/golden/nc/grblhal/{file}.nc");
        if write {
            std::fs::write(repo(&rel), &out.program).expect("golden written");
        }
        assert_eq!(out.program, read(&rel), "{rel}: run with WRITE_GOLDEN=1 and review the diff");
    }
}
