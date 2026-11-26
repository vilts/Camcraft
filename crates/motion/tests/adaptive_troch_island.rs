//! Adaptive trochoid shape checks from the project tests for issues #1509 and #2990.
//! Gate map: covers camcraft_motion::machining2d (adaptive_troch, adaptive_troch_lanes, adaptive_troch_clean), camcraft_motion::quality.

#[path = "common/adaptive_troch.rs"]
mod fixtures;

use fixtures::*;
use camcraft_motion::machining2d::PocketParams;
use camcraft_motion::geo2d::Region;

#[test]
#[ignore = "heavy lane: motion heavy: adaptive_troch_island (154 s alone in debug, #3514)"]
fn pocket_with_island_is_cleared_without_crossing_or_excess_rest() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let Ok(isl) = Region::new(vec![
        poly(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]),
        poly(&[(12.0, 12.0), (18.0, 12.0), (18.0, 18.0), (12.0, 18.0)]),
    ]) else {
        panic!("island")
    };
    let Ok(isl2) = Region::new(vec![
        poly(&[(0.0, 0.0), (48.0, 0.0), (48.0, 40.0), (0.0, 40.0)]),
        poly(&[(18.0, 14.0), (30.0, 14.0), (30.0, 26.0), (18.0, 26.0)]),
    ]) else {
        panic!("island2")
    };
    check_cleared("48x40 island D10", &isl2, &PocketParams { tool_diameter: 10.0, ..p });
    check_cleared("30x30 island D6", &isl, &p);
}

/// The 30x30 island D6 case only: the same confinement, no-crossing,
/// engagement, no-slot and rest-against-HSM laws. The 48x40 D10 case
/// stays in the full test.
#[test]
#[ignore = "light lane: the 30x30 island D6 case of the full test only (release); the full test is pocket_with_island_is_cleared_without_crossing_or_excess_rest"]
fn pocket_with_island_is_cleared_without_crossing_or_excess_rest_light() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let Ok(isl) = Region::new(vec![
        poly(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]),
        poly(&[(12.0, 12.0), (18.0, 12.0), (18.0, 18.0), (12.0, 18.0)]),
    ]) else {
        panic!("island")
    };
    check_cleared("30x30 island D6", &isl, &p);
}

#[test]
fn ring_round_island_is_cleared_with_closed_lane() {
    let part = rect(0.0, 0.0, 12.0, 10.0);
    let (Ok(o), Ok(i)) = (part.offset(12.0), part.offset(2.0)) else { panic!("offsets") };
    let Ok(ring) = o.difference(&i) else { panic!("ring") };
    let p = PocketParams { bottom_z: -3.0, ..params() };
    check_cleared("ring D6", &ring, &p);
}
