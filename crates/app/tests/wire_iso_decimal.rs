//! #3206: the generic ISO wire post writes each coordinate, U/V offset,
//! arc centre and taper angle with a decimal point. Many wire controls
//! read a number without one in their least input unit (0.001 mm), so
//! `X5` is 5 um there, not 5 mm. The pin reads every wire-iso golden;
//! each golden is itself locked to the posted output by its own test
//! (wire_post, wire_taper, wire_taper_angle, wire_tabs, wire_burnout,
//! wire_4axis), so a golden word is a posted word.
//!
//! Gate map (#3206): covers the wire-iso.cps script (fu, the angle word).
//! Provenance: our own test over our own goldens; the decimal-point rule
//! is the RS-274 / ISO 6983 convention for a dimension word.

const GOLDENS: &str = "tests/golden/nc/wire-iso";

/// The dimension words in one NC line (comments removed) that lack a
/// decimal point.
fn bare_words(line: &str) -> Vec<String> {
    let code = line.split('(').next().unwrap_or("");
    code.split_whitespace()
        .filter(|w| {
            let mut c = w.chars();
            let letter = c.next().unwrap_or(' ');
            let num: String = c.collect();
            "XYUVIJA".contains(letter) && !num.is_empty() && num.parse::<f64>().is_ok() && !num.contains('.')
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn every_wire_iso_dimension_word_carries_a_decimal_point() {
    let mut files: Vec<_> = std::fs::read_dir(GOLDENS)
        .expect("wire-iso goldens present")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "nc"))
        .collect();
    files.sort();
    assert!(files.len() >= 8, "expected the wire-iso goldens, got {files:?}");
    let mut bad = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).expect("read golden");
        for (n, line) in text.lines().enumerate() {
            for w in bare_words(line) {
                bad.push(format!("{}:{}: {w} in `{line}`", f.display(), n + 1));
            }
        }
    }
    assert!(bad.is_empty(), "dimension words without a decimal point:\n{}", bad.join("\n"));
}

#[test]
fn the_word_check_sees_a_bare_word_and_passes_a_pointed_one() {
    assert_eq!(bare_words("G1 X5 Y0 (X5)"), vec!["X5".to_string(), "Y0".to_string()]);
    assert!(bare_words("G1 X5. Y0. U-0.5 V.25 FX-ANGLE A3.").is_empty());
}
