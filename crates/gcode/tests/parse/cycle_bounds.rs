//! Bounded canned-cycle expansion (#3657 review 1): G73/G83 pecks that make no f64 progress, and the move budget over pecks and `L` repeats.
//! Provenance: AGENTS.md Prime Directive (never hang); IEEE 754 binary64 spacing near 1e6 and 1e20.

use camcraft_gcode::GcodeError;

use crate::refusal;

fn is_cycle_refusal(e: &GcodeError, line: usize) -> bool {
    matches!(e, GcodeError::InvalidCycle { line: l, .. } | GcodeError::OutOfRange { line: l, .. } if *l == line)
}

/// The review input: Z and R near 1e20, where `t - 10.0 == t`.
#[test]
fn peck_cycles_near_1e20_refuse_and_never_spin() {
    for code in ["G73", "G83"] {
        let nc = format!(
            "G21 G90\n{code} X0 Y0 Z99999999999999980000 R100000000000000000000 Q10 F100\n"
        );
        let e = refusal(&nc);
        assert!(is_cycle_refusal(&e, 2), "{code}: {e:?}");
    }
}

/// Inside the coordinate domain: at 1e6 mm one ULP is about 1.2e-10 mm, so a Q of
/// 1e-11 mm makes no progress, although the peck count (about 4000) is legal.
#[test]
fn peck_cycles_without_f64_progress_refuse_by_line() {
    for code in ["G73", "G83"] {
        let nc = format!(
            "G21 G90\nG0 X0 Y0 Z1000000.\n{code} X0 Y0 Z999999.99999996 R1000000. Q0.00000000001 F100\n"
        );
        let e = refusal(&nc);
        assert!(
            matches!(&e, GcodeError::InvalidCycle { line: 3, message } if message.contains("progress")),
            "{code}: {e:?}"
        );
    }
}

#[test]
fn peck_count_over_4096_per_hole_refuses() {
    for code in ["G73", "G83"] {
        let nc = format!("G21 G90\nG0 X0 Y0 Z5.\n{code} X0 Y0 Z-40. R1. Q0.01 F100\n");
        assert!(is_cycle_refusal(&refusal(&nc), 3), "{code}");
    }
}

/// 410 pecks per hole times 200 repeats is more than 160 000 moves from one short block.
#[test]
fn expansion_over_pecks_and_repeats_is_bounded() {
    for code in ["G73", "G83"] {
        let nc = format!("G21 G91\nG0 Z5.\n{code} X1. Y0 Z-41. R-4. Q0.1 L200 F100\n");
        let e = refusal(&nc);
        assert!(
            matches!(&e, GcodeError::InvalidCycle { line: 3, message } if message.contains("limit")),
            "{code}: {e:?}"
        );
    }
    let ok = "G21 G91\nG0 Z5.\nG83 X1. Y0 Z-41. R-4. Q0.1 L10 F100\n";
    let parsed = crate::parse_in_time(ok, &Default::default()).expect("ten deep holes are in budget");
    assert_eq!(parsed.sections[0].toolpath.cycles.len(), 10);
}
