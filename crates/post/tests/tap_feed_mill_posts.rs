//! Provenance: our own mill posts and CL stream tests for tap pitch precision (#3699).
//! The tap feed is pitch x rpm, kept at 0.001 mm/min by the stream; the
//! control cuts the pitch F / S, so a rounded F word cuts a wrong pitch.
//! Gate map: covers assets/posts/fanuc.cps, assets/posts/doosan-dnm.cps,
//! assets/posts/mazak.cps, assets/posts/sinumerik.cps, assets/posts/tormach.cps,
//! assets/posts/linuxcnc.cps, camcraft_post::post::engine.
//! Lane shape: all tests here run in milliseconds.

use camcraft_post::post::engine;
use serde_json::json;
use std::path::Path;

struct Post {
    stem: &'static str,
    machine: &'static str,
    /// The stream cycle kind that posts as G84 on this control.
    cycle: &'static str,
    /// A Fanuc-family control writes a whole-number F with its point (#1836).
    point: bool,
}

const FANUC: Post = Post { stem: "fanuc", machine: "3axis-vmc", cycle: "TAP", point: true };
const DOOSAN: Post = Post { stem: "doosan-dnm", machine: "doosan-dnm-3axis", cycle: "TAP", point: true };
const MAZAK: Post = Post { stem: "mazak", machine: "3axis-vmc", cycle: "TAP", point: true };
const SINUMERIK: Post = Post { stem: "sinumerik", machine: "3axis-vmc", cycle: "TAP", point: false };
const TORMACH: Post = Post { stem: "tormach", machine: "tormach-1100mx", cycle: "TAP", point: false };
// LinuxCNC posts a rigid tap as G33.1; its G84 is the floating-holder tap.
const LINUXCNC: Post = Post { stem: "linuxcnc", machine: "linuxcnc-3axis", cycle: "FLOATINGTAP", point: false };

struct Posted {
    program: String,
    warnings: Vec<String>,
}

fn post_tap(post: &Post, feed_um: u64, rpm: &str, units: &str) -> Result<Posted, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cps = std::fs::read_to_string(root.join(format!("assets/posts/{}.cps", post.stem))).expect("post script");
    let machine = serde_json::from_str(
        &std::fs::read_to_string(root.join(format!("assets/machines/{}.mch", post.machine))).expect("machine file"),
    )
    .expect("machine JSON");
    let cycle = post.cycle;
    let stream = format!(
        "PARAM WorkOffset G54\nPARAM ClearancePlane 25\nPARAM RetractPlane 5\n\
         TOOL 1 \"6MM TAP\"\nPARAM SpindleSpeed {rpm}\nPARAM OperationName TapPrecision\nSECTION TapPrecision\n\
         Rapid X 0 Y 0 Z 5000\n\
         CYCLE {cycle} X 0 Y 0 R 2000 Z -10000 Q 2000 P 0 F {feed_um}\n\
         Rapid X 0 Y 0 Z 2000\nPlunge X 0 Y 0 Z -10000 F {feed_um}\n\
         Rapid X 0 Y 0 Z 2000\nENDSECTION\n"
    );
    // The LinuxCNC machine file states no safe Z; its G28 retract keeps the test on the tap.
    let options = json!({ "Units": units, "retractMode": "g28" });
    let out = engine::run_with(&cps, &machine, &stream, &options)
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

fn assert_feed(post: &Post, feed_um: u64, rpm: &str, units: &str, word: &str) {
    let out = post_tap(post, feed_um, rpm, units)
        .unwrap_or_else(|e| panic!("{} {word}: tap posts: {e}", post.stem));
    assert!(out.warnings.is_empty(), "{} {word}: {:?}\n{}", post.stem, out.warnings, out.program);
    assert_eq!(g84_feed(&out.program), Some(word), "{}:\n{}", post.stem, out.program);
}

fn assert_tap_refused(post: &Post, feed_um: u64, rpm: &str, units: &str) {
    match post_tap(post, feed_um, rpm, units) {
        Err(error) => assert!(error.contains("G84 (tap)"), "{}: {error}", post.stem),
        Ok(out) => {
            assert!(
                out.warnings
                    .iter()
                    .any(|w| w.contains("G84 (tap)") && w.contains("TapPrecision")),
                "{}: tap pitch must be refused by operation name: {:?}\n{}",
                post.stem,
                out.warnings,
                out.program
            );
            assert!(g84_feed(&out.program).is_none(), "{}: refused tap writes no G84:\n{}", post.stem, out.program);
        }
    }
}

/// The issue's two feeds (M1.4 x 0.3 at S115, M2 x 0.4 at S333), a feed
/// that needs three decimals, a whole feed, both units, and the refusals.
fn check_post(post: &Post) {
    assert_feed(post, 34_500, "115", "mm", "F34.5");
    assert_feed(post, 133_200, "333", "mm", "F133.2");
    assert_feed(post, 34_561, "115", "mm", "F34.561");
    assert_feed(post, 600_000, "1000", "mm", if post.point { "F600." } else { "F600" });
    assert_feed(post, 34_500, "115", "inch", "F1.3583");
    assert_feed(post, 133_200, "333", "inch", "F5.2441");
    // S115 changes the pitch of 34.5 mm/min at a requested 115.4 rpm.
    assert_tap_refused(post, 34_500, "115.4", "mm");
    // A positive feed below the smallest inch step must not become F0.
    assert_tap_refused(post, 1, "100", "inch");
}

#[test]
fn fanuc_g84_keeps_exact_tap_feed() {
    check_post(&FANUC);
}

#[test]
fn doosan_dnm_g84_keeps_exact_tap_feed() {
    check_post(&DOOSAN);
}

#[test]
fn mazak_g84_keeps_exact_tap_feed() {
    check_post(&MAZAK);
}

#[test]
fn sinumerik_g84_keeps_exact_tap_feed() {
    check_post(&SINUMERIK);
}

#[test]
fn tormach_g84_keeps_exact_tap_feed() {
    check_post(&TORMACH);
}

#[test]
fn linuxcnc_g84_keeps_exact_tap_feed() {
    check_post(&LINUXCNC);
}
