//! Provenance: our own Mach4 post and CL stream tests for tap pitch precision (#3701).
//! Gate map: covers assets/posts/mach4.cps, camcraft_post::post::engine.

use camcraft_post::post::engine;
use serde_json::json;
use std::path::Path;

struct Posted {
    program: String,
    warnings: Vec<String>,
}

fn post_tap(feed_um: u64, rpm: &str, units: &str) -> Result<Posted, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cps = std::fs::read_to_string(root.join("assets/posts/mach4.cps")).expect("Mach4 post");
    let machine = serde_json::from_str(
        &std::fs::read_to_string(root.join("assets/machines/3axis-vmc.mch")).expect("VMC machine"),
    )
    .expect("VMC machine JSON");
    let stream = format!(
        "PARAM WorkOffset G54\nPARAM ClearancePlane 25\nPARAM RetractPlane 5\n\
         TOOL 1 \"6MM TOOL\"\nPARAM SpindleSpeed {rpm}\nPARAM OperationName TapPrecision\nSECTION TapPrecision\n\
         Rapid X 0 Y 0 Z 5000\n\
         CYCLE TAP X 0 Y 0 R 2000 Z -10000 Q 2000 P 0 F {feed_um}\n\
         Rapid X 0 Y 0 Z 2000\nPlunge X 0 Y 0 Z -10000 F {feed_um}\n\
         Rapid X 0 Y 0 Z 2000\nENDSECTION\n"
    );
    let out = engine::run_with(&cps, &machine, &stream, &json!({ "Units": units }))
        .map_err(|e| e.to_string())?;
    Ok(Posted {
        program: out.program,
        warnings: out.warnings.iter().map(|w| w.to_string()).collect(),
    })
}

fn g84_feed(program: &str) -> Option<&str> {
    program
        .lines()
        .find(|line| line.split_whitespace().any(|word| word == "G84"))
        .and_then(|line| line.split_whitespace().find(|word| word.starts_with('F')))
}

#[test]
fn mach4_g84_keeps_two_and_three_mm_feed_decimals() {
    for (feed, word) in [(34_560, "F34.56"), (34_561, "F34.561")] {
        let out = post_tap(feed, "115", "mm").expect("tap posts");
        assert!(out.warnings.is_empty(), "{word}: {:?}", out.warnings);
        assert_eq!(g84_feed(&out.program), Some(word), "{}", out.program);
        assert!(out.program.contains("S115 M3"), "{}", out.program);
    }
}

#[test]
fn mach4_g84_keeps_four_inch_feed_decimals() {
    for (feed, word) in [(34_560, "F1.3606"), (133_200, "F5.2441")] {
        let out = post_tap(feed, "115", "inch").expect("inch tap posts");
        assert!(out.warnings.is_empty(), "{word}: {:?}", out.warnings);
        assert!(out.program.contains("G20"), "{}", out.program);
        assert_eq!(g84_feed(&out.program), Some(word), "{}", out.program);
    }
}

fn assert_tap_refused(feed_um: u64, rpm: &str, units: &str) {
    match post_tap(feed_um, rpm, units) {
        Err(error) => assert!(error.contains("G84 (tap)"), "{error}"),
        Ok(out) => {
            assert!(
                out.warnings
                    .iter()
                    .any(|w| w.contains("G84 (tap)") && w.contains("TapPrecision")),
                "tap pitch must be refused by operation name: {:?}\n{}",
                out.warnings,
                out.program
            );
            assert!(
                g84_feed(&out.program).is_none(),
                "refused tap writes no G84:\n{}",
                out.program
            );
        }
    }
}

#[test]
fn mach4_refuses_tap_feed_rounding_that_changes_pitch() {
    // F0.0787 is more than 0.01 percent below 2 / 25.4 in/min.
    assert_tap_refused(2_000, "100", "inch");
    // A positive feed below the smallest inch step must not become F0.
    assert_tap_refused(1, "100", "inch");
}

#[test]
fn mach4_refuses_tap_speed_rounding_that_changes_pitch() {
    // S115 changes the pitch of 34.56 mm/min at a requested 115.4 rpm.
    assert_tap_refused(34_560, "115.4", "mm");
}
