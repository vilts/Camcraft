//! #611 / #619: the Tormach 8L, 15L Slant-PRO and 15LX Slant-PRO, shipped as lathe machines so
//! turning programs can be judged against a real Tormach lathe envelope
//! (and simulated in PathPilot Hub's 8L / 15L). Every figure is from
//! Tormach's operator manuals (UM10753, UM10225, UM10860; sources in
//! docs/decisions/611-tormach-lathes.md); travel is stated as the full
//! stroke either way of the work zero, the shipped mills' convention.
//!
//! Gate map (#611): covers app::posting (MACHINES), post::machine
//! (parse_mch of the two files).
//! Provenance: Tormach's published 8L and 15L Slant-PRO specifications;
//! our own fixtures.

use camcraft_app::machine::{kind, parse_mch, Kind, Machine};

const IN: f64 = 25.4;

fn shipped(name: &str) -> Machine {
    let text = camcraft_app::posting::builtin_machine_text(name)
        .unwrap_or_else(|why| panic!("{name} is not a shipped machine: {why}"));
    parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// One lathe's spec-sheet row.
struct Spec {
    name: &'static str,
    /// X (cross-slide) and Z (carriage) stroke, inches.
    travel_in: [f64; 2],
    /// (min, max); a min of NaN = the manual states none (unstated limit).
    rpm: (f64, f64),
    ranges: &'static [(f64, f64)],
    ipm: f64,
    kw: f64,
    tools: usize,
}

const SPECS: [Spec; 3] = [
    // 8L: X 4.5 in, Z 10 in; 180-2500 / 350-5000 rpm (two belt
    // ranges, 5C nose); 150 IPM; gang / quick-change post (no turret
    // - PathPilot's 99-entry tool table bounds the numbers).
    Spec { name: "Tormach 8L", travel_in: [4.5, 10.0], rpm: (180.0, 5000.0), ranges: &[(180.0, 2500.0), (350.0, 5000.0)], ipm: 150.0, kw: 1.1, tools: 99 },
    // 15L Slant-PRO: X 10 in, Z 12 in; 180-2500 rpm with its 6 in
    // chuck (the 5C collet's 3500 is NOT the default - see the
    // decision); 60 IPM; 8-station turret.
    Spec { name: "Tormach 15L Slant-PRO", travel_in: [10.0, 12.0], rpm: (180.0, 2500.0), ranges: &[], ipm: 60.0, kw: 2.24, tools: 8 },
    // 15LX Slant-PRO (UM10860 §3.1, #619): X 10 in, Z 14 in; max
    // 3500 rpm, no minimum stated (servo spindle); 6 hp continuous
    // (4.5 kW); 360 IPM; gang tooling standard (turret is a kit).
    Spec { name: "Tormach 15LX Slant-PRO", travel_in: [10.0, 14.0], rpm: (f64::NAN, 3500.0), ranges: &[], ipm: 360.0, kw: 4.5, tools: 99 },
];

#[test]
fn every_tormach_lathe_states_its_spec_sheet_figures() {
    for s in &SPECS {
        let n = s.name;
        let m = shipped(n);
        assert_eq!(m.model, n);
        assert_eq!(kind(Some(&m)), Kind::Lathe, "{n} is a lathe");
        assert_eq!(m.units.as_deref(), Some("inch"), "{n} is programmed in inches");
        for (axis, want) in ["X", "Z"].iter().zip(s.travel_in) {
            let a = m.axes.iter().find(|a| a.name == *axis).unwrap_or_else(|| panic!("{n} has no {axis}"));
            let (lo, hi) = a.travel.unwrap_or_else(|| panic!("{n} {axis} travel"));
            assert!(close(hi, want * IN) && close(lo, -want * IN), "{n} {axis}: {lo}..{hi}");
            assert!(a.feed.is_some_and(|f| close(f, s.ipm * IN)), "{n} {axis} feed {:?}", a.feed);
        }
        assert!(!m.axes.iter().any(|a| a.name == "Y"), "{n}: a 2-axis lathe states no Y");
        let want_min = (!s.rpm.0.is_nan()).then_some(s.rpm.0);
        assert_eq!((m.spindle_min, m.spindle_max), (want_min, Some(s.rpm.1)), "{n} rpm");
        assert_eq!(m.spindle_ranges, s.ranges.to_vec(), "{n} ranges");
        assert_eq!(m.tool_count, Some(s.tools), "{n} tool count");
        assert!(m.spindle_power.is_some_and(|p| close(p, s.kw)), "{n} power {:?}", m.spindle_power);
    }
}

#[test]
fn the_lathes_are_offered_with_the_lathe_posts_not_the_mills() {
    for s in &SPECS {
        let m = shipped(s.name);
        assert_ne!(kind(Some(&m)), Kind::Mill, "{}: never offered as a mill", s.name);
    }
}
