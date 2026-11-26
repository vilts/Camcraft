//! Adaptive trochoid shape checks from the project tests for issues #1509 and #2990.
//! Gate map: covers camcraft_motion::machining2d (adaptive_troch, adaptive_troch_lanes, adaptive_troch_clean), camcraft_motion::quality.

#[path = "common/adaptive_troch.rs"]
mod fixtures;

use fixtures::*;
use camcraft_motion::machining2d::PocketParams;
use camcraft_motion::geo2d::Region;

#[test]
#[ignore = "heavy lane: motion heavy: adaptive_troch_neck (168 s alone in debug, #3514)"]
fn triangle_and_wide_to_narrow_neck_are_cleared() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let Ok(tri) = Region::new(vec![poly(&[(0.0, 0.0), (60.0, 0.0), (0.0, 60.0)])]) else { panic!("triangle") };
    check_cleared("triangle 60x60 D6", &tri, &p);
    let Ok(neck) = Region::new(vec![poly(&[
        (0.0, 0.0),
        (30.0, 0.0),
        (30.0, 10.0),
        (48.0, 10.0),
        (48.0, 20.0),
        (30.0, 20.0),
        (30.0, 30.0),
        (0.0, 30.0),
    ])]) else {
        panic!("neck")
    };
    check_cleared("wide-to-narrow neck D6", &neck, &p);
}

/// The wide-to-narrow neck case only: the same confinement, no-crossing,
/// engagement, no-slot and rest-against-HSM laws. The 60x60 triangle
/// stays in the full test.
#[test]
#[ignore = "light lane: the wide-to-narrow neck case of the full test only (release); the full test is triangle_and_wide_to_narrow_neck_are_cleared"]
fn triangle_and_wide_to_narrow_neck_are_cleared_light() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let Ok(neck) = Region::new(vec![poly(&[
        (0.0, 0.0),
        (30.0, 0.0),
        (30.0, 10.0),
        (48.0, 10.0),
        (48.0, 20.0),
        (30.0, 20.0),
        (30.0, 30.0),
        (0.0, 30.0),
    ])]) else {
        panic!("neck")
    };
    check_cleared("wide-to-narrow neck D6", &neck, &p);
}

#[test]
#[ignore = "heavy lane: release engagement and rest-area measurement"]
fn l_shaped_pocket_is_cleared_under_engagement_limit() {
    let p = PocketParams { tool_diameter: 10.0, bottom_z: -3.0, ..params() };
    let Ok(l) = Region::new(vec![poly(&[(0.0, 0.0), (60.0, 0.0), (60.0, 30.0), (30.0, 30.0), (30.0, 60.0), (0.0, 60.0)])]) else {
        panic!("l shape");
    };
    check_cleared("L-shape D10", &l, &p);
    check_cleared("L-shape D6", &l, &PocketParams { tool_diameter: 6.0, ..p });
}
