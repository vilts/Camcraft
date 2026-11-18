//! #707: Makera's Carvera desktop mills - the Carvera (C1) and the
//! Carvera Air - each state their maker's published figures: work area
//! (as the full stroke either way in program coordinates), spindle
//! power and top speed, the axes' top travel speed, the C1's six-tool
//! changer and the air blast (M7) both machines offer instead of coolant.
//! Provenance: our own tests over our own machine files; the figures are
//! the "Specs" tables on makera.com/products/carvera and
//! makera.com/pages/carvera-air (read 2026-11-10), and M7 / M9 "airflow"
//! from the Carvera firmware's supported-codes list.
//! Gate map (M4.4): covers app::posting (MACHINES registry) and the
//! machine parser over the two files.

use camcraft_app::machine::{Machine, parse_mch};

fn shipped(name: &str) -> Machine {
    let text = camcraft_app::posting::builtin_machine_text(name)
        .unwrap_or_else(|why| panic!("{name} is not a shipped machine: {why}"));
    parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// One row of Makera's spec table.
struct Spec {
    name: &'static str,
    /// Work area X, Y, Z (mm).
    area: [f64; 3],
    rpm_max: f64,
    kw: f64,
    /// "Max Travel Speed" (mm/min), one figure for every axis.
    travel_speed: f64,
    /// "Tool library size": the C1's changer; the Air's quick changer
    /// states none.
    tools: Option<usize>,
}

const SPECS: [Spec; 2] = [
    Spec { name: "Makera Carvera", area: [360.0, 240.0, 140.0], rpm_max: 15000.0, kw: 0.2, travel_speed: 6000.0, tools: Some(6) },
    Spec { name: "Makera Carvera Air", area: [300.0, 200.0, 130.0], rpm_max: 13000.0, kw: 0.2, travel_speed: 4000.0, tools: None },
];

#[test]
fn every_carvera_states_makeras_figures() {
    for s in &SPECS {
        let m = shipped(s.name);
        let n = s.name;
        assert_eq!(m.model, n);
        assert_eq!(m.kind.as_deref(), Some("mill"), "{n} is a mill");
        // The parser records only an inch file; mm is the default (None).
        // The Carvera firmware halts on G20, so mm is the only choice.
        assert_eq!(m.units, None, "{n} is programmed in mm");
        assert_eq!(m.axes.len(), 3, "{n}: X, Y, Z (the 4th axis module is optional, not part of the base machine)");
        for (a, want) in m.axes.iter().zip(s.area) {
            // Travel is program coordinates from the work zero: the full
            // stroke either way, so any zero on the bed is reachable.
            let (lo, hi) = a.travel.unwrap_or_else(|| panic!("{n} {} travel", a.name));
            assert!(close(hi, want) && close(lo, -want), "{n} {}: {lo}..{hi}", a.name);
            assert!(a.feed.is_some_and(|f| close(f, s.travel_speed)), "{n} {} feed {:?}", a.name, a.feed);
            assert!(a.rapid.is_some_and(|f| close(f, s.travel_speed)), "{n} {} rapid {:?}", a.name, a.rapid);
        }
        // "0 - 15000 RPM": no lowest working speed is published, so none
        // is invented.
        assert_eq!((m.spindle_min, m.spindle_max), (None, Some(s.rpm_max)), "{n} rpm");
        assert_eq!(m.spindle_power, Some(s.kw), "{n} kW");
        assert_eq!(m.tool_count, s.tools, "{n} tool changer");
        let codes: Vec<(&str, &str)> = m.coolants.iter().map(|c| (c.name.as_str(), c.m_code.as_str())).collect();
        assert_eq!(codes, [("Air", "M7")], "{n}: air blast, no flood");
    }
}
