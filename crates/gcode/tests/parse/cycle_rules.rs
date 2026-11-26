//! Canned cycle rules: G17 only (#3657 review 4), sticky state over repeated codes (review 6), and the G85/G89 retract rates (review 7).
//! Provenance: LinuxCNC G-code reference (common canned cycle info, G85, G89) and RS-274/NGC (NISTIR 6556 §3.5.16).

use camcraft_cl::cl::{Move, MoveKind};
use camcraft_gcode::{GcodeError, ParseOptions, ParsedProgram};

use crate::{parse_in_time, refusal};

fn parse_ok(nc: &str) -> ParsedProgram {
    parse_in_time(nc, &ParseOptions::default()).expect(nc)
}

/// (kind, end Z) of the moves after the hole's positioning, for hole `n`.
fn hole_moves(p: &ParsedProgram, n: usize) -> Vec<(MoveKind, f64)> {
    let tp = &p.sections[0].toolpath;
    let c = tp.cycles[n];
    tp.moves[c.first_move..]
        .iter()
        .take_while(|m: &&Move| m.to.x == c.xy.x && m.to.y == c.xy.y)
        .map(|m| (m.kind, m.to.z))
        .collect()
}

#[test]
fn cycles_in_g18_and_g19_refuse_before_expansion() {
    let e = refusal("G21 G18 G90\nG0 Y10. Z10.\nG81 X0 Y-5. Z-5. R2. F100\n");
    assert!(matches!(e, GcodeError::UnsupportedCode { line: 3, .. }), "{e:?}");
    let e = refusal("G21 G19 G90\nG0 Z10.\nG83 X0 Y0 Z-5. R2. Q1. F100\n");
    assert!(matches!(e, GcodeError::UnsupportedCode { line: 3, .. }), "{e:?}");
}

#[test]
fn plane_change_while_a_cycle_is_active_refuses() {
    let base = "G21 G90\nG0 Z10.\nG81 X0 Y0 Z-5. R2. F100\n";
    for change in ["G18\n", "G19 X5.\n", "G18 G81 X5.\n"] {
        let e = refusal(&format!("{base}{change}"));
        assert!(matches!(e, GcodeError::UnsupportedCode { line: 4, .. }), "{change}: {e:?}");
    }
    for ok in ["G80\nG18\n", "G0 G18 X5.\n", "G17 X5.\n"] {
        parse_ok(&format!("{base}{ok}"));
    }
}

/// The review input: `G81 X10` repeats the code, and R and Z stay sticky.
#[test]
fn repeated_cycle_code_keeps_sticky_r_and_z() {
    let p = parse_ok("G21 G90\nG0 Z10.\nG81 X0 Z-5. R2. F100\nG81 X10.\nX20.\nG82 X30. P0.5\n");
    let c = &p.sections[0].toolpath.cycles;
    assert_eq!(c.len(), 4);
    for cy in c {
        assert_eq!((cy.r_z, cy.bottom_z), (2.0, -5.0));
    }
    assert_eq!([c[0].xy.x, c[1].xy.x, c[2].xy.x, c[3].xy.x], [0.0, 10.0, 20.0, 30.0]);
}

/// G98 returns to the level before the series, also after a G99 hole and a
/// repeated code with all words.
#[test]
fn g98_initial_level_survives_g99_and_repeated_codes() {
    let p = parse_ok(
        "G21 G90\nG0 Z10.\nG99 G81 X0 Z-5. R2. F100\nG98 G81 X10. Z-5. R2.\nG99 X20.\nG98 X30.\n",
    );
    let clear: Vec<f64> = (0..4).map(|n| hole_moves(&p, n).last().map_or(f64::NAN, |m| m.1)).collect();
    assert_eq!(clear, vec![2.0, 10.0, 2.0, 10.0]);
}

/// G91: R from the initial level, Z from R; bare blocks repeat the increments.
#[test]
fn incremental_sticky_depths_repeat_from_the_same_levels() {
    let p = parse_ok("G21 G90\nG0 X0 Y0 Z10.\nG91 G81 X10. R-8. Z-7. F100\nX10.\nG81 X10.\n");
    let c = &p.sections[0].toolpath.cycles;
    assert_eq!(c.len(), 3);
    for (n, cy) in c.iter().enumerate() {
        assert_eq!((cy.xy.x, cy.r_z, cy.bottom_z), (10.0 * (n as f64 + 1.0), 2.0, -5.0));
    }
}

/// The review input for G89, and G85 beside it, in both return modes.
#[test]
fn g89_feeds_to_the_clear_level_and_g85_feeds_to_r() {
    let run = |mode: &str, code: &str| {
        let extra = if code == "G89" { " P1." } else { "" };
        let p = parse_ok(&format!("G21 G90 {mode}\nG0 Z10.\n{code} X1. Z-5. R2.{extra} F100\n"));
        hole_moves(&p, 0)
    };
    use MoveKind::{Feed, Plunge, Rapid};
    assert_eq!(run("G98", "G89"), vec![(Plunge, -5.0), (Feed, 10.0)]);
    assert_eq!(run("G99", "G89"), vec![(Plunge, -5.0), (Feed, 2.0)]);
    assert_eq!(run("G98", "G85"), vec![(Plunge, -5.0), (Feed, 2.0), (Rapid, 10.0)]);
    assert_eq!(run("G99", "G85"), vec![(Plunge, -5.0), (Feed, 2.0)]);
}
