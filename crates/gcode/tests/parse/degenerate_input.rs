//! Degenerate-input tests for `camcraft-gcode`: malformed programs give a value or a refusal, never a panic or a hang.
//! Provenance: AGENTS.md Prime Directive (degenerate input gives a clean error); inputs follow RS-274/NGC (NISTIR 6556) word syntax.

use camcraft_gcode::ParseOptions;

use crate::parse_in_time;

#[test]
fn degenerate_programs_refuse_or_parse_without_panic_or_hang() {
    let cases: &[&'static str] = &[
        "",
        "\n\n\n",
        "%\n%\n",
        "G21 G90\r\nG0 X1.\rG1 Y2. F100\r\n",
        "G21 G90\nX\n",
        "G21 G90\nG0 X-\n",
        "G21 G90\nG0 X1e999\n",
        "G21 G90\nG0 X1.2.3\n",
        "G21 G90\nG0 X99999999999999999999999999999999999999.\n",
        "G21 G90\nG1 X10. F0\n",
        "G21 G90\nG1 X10. F-5\n",
        "G21 G90\nG0 X0 Y0\nG2 X0 Y0 I0 J0 F100\n",
        "G21 G90\nG0 X0 Y0\nG2 X10. Y0 R0 F100\n",
        "G21 G90\nG0 X0 Y0\nG2 X100. Y0 R1. F100\n",
        "G21 G90\nG0 X0 Y0 Z5.\nG83 X0 Y0 Z-10. R2. Q0 F100\n",
        "G21 G90\nG0 X0 Y0 Z5.\nG83 X0 Y0 Z-10. R2. Q-1. F100\n",
        "G21 G90\nG0 X0 Y0 Z5.\nG73 X0 Y0 Z-10. R2. Q0.000001 F100\n",
        "G21 G90\nG0 X0 Y0 Z5.\nG81 X0 Y0 Z-10. R2. L999999999999 F100\n",
        "G21 G90\nG0 X0 Y0 Z5.\nG81 X0 Y0 Z-10. R2. L-3 F100\n",
        "G21 G90\nG0 X0 Y0 Z5.\nG81 X0 Y0 Z10. R2. F100\n",
        "G21 G90\nG4 P-1.\n",
        "G21 G90\nT99999999999 M6\n",
        "G21 G90\nS-100 M3\n",
        "G21 G90\nG0 X\u{00e9}1.\n",
        "G21 G90\n(unclosed comment\nG0 X1.\n",
        "G21 G90\nG0 X1. ; trailing\nN99999999999999999999 G0 X2.\n",
        "G21 G90\nG0 G1 G2 G3 X1.\n",
        "G21 G20 G0 X1.\n",
    ];
    for nc in cases {
        let _ = parse_in_time(nc, &ParseOptions::default());
    }
}
