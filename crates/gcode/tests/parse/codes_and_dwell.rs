//! Decimal code values keep their place value (#3657 review 8), and the Haas dwell reads the written form of `P` (review 9).
//! Provenance: RS-274/NGC decimal G-codes (G90.1, G91.1); the Haas G04 reference (P without a decimal point is ms, with one is s).

use camcraft_gcode::{ArcCenterMode, Dialect, DistanceMode, GcodeError, ParseOptions};

use crate::{parse_in_time, refusal};

/// The allow-list of exact decimal code values.
#[test]
fn decimal_codes_keep_their_place_value() {
    let ok: &[(&str, ArcCenterMode, DistanceMode)] = &[
        ("G90.1", ArcCenterMode::Absolute, DistanceMode::Absolute),
        ("G90.10", ArcCenterMode::Absolute, DistanceMode::Absolute),
        ("G91.1", ArcCenterMode::Incremental, DistanceMode::Absolute),
        ("G91.100", ArcCenterMode::Incremental, DistanceMode::Absolute),
        ("G90.0", ArcCenterMode::Incremental, DistanceMode::Absolute),
        ("G91.", ArcCenterMode::Incremental, DistanceMode::Incremental),
    ];
    for (code, arc, dist) in ok {
        let p = parse_in_time(&format!("G21\n{code}\n"), &ParseOptions::default()).expect(code);
        assert_eq!((p.final_modal.arc_center, p.final_modal.distance), (*arc, *dist), "{code}");
    }
    for code in ["G90.01", "G91.01", "G90.11", "G17.01", "G43.04"] {
        let e = refusal(&format!("G21\n{code}\n"));
        assert!(
            matches!(&e, GcodeError::UnsupportedCode { line: 2, code: c, .. } if c == code),
            "{code}: {e:?}"
        );
    }
}

fn dwells(dialect: Dialect, nc: &str) -> Vec<f64> {
    let opts = ParseOptions { dialect, ..Default::default() };
    parse_in_time(nc, &opts).expect(nc).dwells.iter().map(|d| d.seconds).collect()
}

#[test]
fn haas_g04_p_without_a_point_is_ms_and_with_a_point_is_s() {
    let nc = "G21\nG04 P10\nG04 P10.\nG4 P2.5\nG4 P1500\n";
    assert_eq!(dwells(Dialect::haas(), nc), vec![0.01, 10.0, 2.5, 1.5]);
    // Fanuc stays in ms for P; RS-274 stays in seconds.
    assert_eq!(dwells(Dialect::fanuc(), "G21\nG04 P10\nG04 P10.\n"), vec![0.01, 0.01]);
    assert_eq!(dwells(Dialect::rs274(), "G21\nG04 P10\nG04 P10.\n"), vec![10.0, 10.0]);
}

#[test]
fn haas_cycle_dwell_uses_the_same_rule() {
    let opts = ParseOptions { dialect: Dialect::haas(), ..Default::default() };
    let nc = "G21 G90\nG0 Z10.\nG82 X0 Y0 Z-5. R2. P500 F100\nG82 X10. P0.5\n";
    let p = parse_in_time(nc, &opts).expect("haas G82");
    let d: Vec<f64> = p.sections[0].toolpath.cycles.iter().map(|c| c.dwell).collect();
    assert_eq!(d, vec![0.5, 0.5]);
}
