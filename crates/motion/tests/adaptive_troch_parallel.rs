//! Adaptive trochoid shape checks from the project tests for issues #1509 and #2990.
//! Gate map: covers camcraft_motion::machining2d (adaptive_troch, adaptive_troch_lanes, adaptive_troch_clean), camcraft_motion::quality.

#[path = "common/adaptive_troch.rs"]
mod fixtures;

use fixtures::*;
use camcraft_motion::machining2d::PocketParams;

// #3141: 64-145 s in debug on a loaded runner, so the full case and its wall-time budget run in release only.
#[test]
#[ignore = "heavy lane: release --ignored (the 30x24 D6 and 44x36 D10 pockets and their 90 s budget; 36-145 s debug)"]
fn wide_rectangular_pocket_is_cleared_by_parallel_lanes() {
    let started = std::time::Instant::now();
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let r = rect(0.0, 0.0, 30.0, 24.0);
    check_cleared("wide 30x24 D6", &r, &p);
    // A D10 tool needs a short side above 32.36 mm to keep parallel lanes.
    check_cleared("wide 44x36 D10", &rect(0.0, 0.0, 44.0, 36.0), &PocketParams { tool_diameter: 10.0, ..p });
    assert!(started.elapsed().as_secs_f64() <= 90.0, "wide pocket release fixture exceeds 90 seconds: {:?}", started.elapsed());
}

#[test]
#[ignore = "light lane: the 30x24 D6 pocket of the full test only (release)"]
fn wide_rectangular_pocket_is_cleared_by_parallel_lanes_light() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    check_cleared("wide 30x24 D6", &rect(0.0, 0.0, 30.0, 24.0), &p);
}

#[test]
fn corner_cleanup_reduces_34x16_rest_to_at_most_hsm_rest() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    check_cleared("face 34x16 D6", &face(), &p);
}
