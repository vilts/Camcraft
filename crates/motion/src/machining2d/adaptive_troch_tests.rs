//! 2D Adaptive's trochoidal method, pinned (issue #1509; Gate map:
//! covers motion::machining2d::adaptive_troch, trochoid_bore,
//! trochoid_slot::plan_trochoidal_slot): the path
//! stays in the region and under the engagement limit, the opened bore
//! leaves no post, and every refusal is by name.

use super::super::trochoid_slot::{DEFAULT_MICROLIFT, TrochoidParams, TrochoidReturn};
use super::*;
use crate::entry::Entry3D;
use crate::geo2d::Loop;
use crate::links::Links;
use crate::load::load_step;
use crate::quality::{QualitySpec, measure, rest_area, stock_for};

const D: f64 = 6.0;

fn poly(pts: &[(f64, f64)]) -> Loop {
    let Ok(l) = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()) else { panic!("loop") };
    l
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let Ok(r) = Region::new(vec![poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])]) else { panic!("region") };
    r
}

/// A pocket face, 34 x 16 about the origin: a slim twin of the
/// maintainer's 50 x 25 (#1503; that one is the app suite's), its band
/// still wider than the tool.
fn face() -> Region {
    rect(-17.0, -8.0, 17.0, 8.0)
}

fn params() -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: D,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        retract_z: Some(2.0),
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: Links::PLANAR,
    }
}

fn refusal(region: &Region, p: &PocketParams) -> String {
    match adaptive_trochoidal(region, p) {
        Err(e) => e.to_string(),
        Ok(t) => panic!("not refused: {} moves, {:?}", t.moves.len(), t.notes),
    }
}

fn cut(region: &Region, p: &PocketParams) -> Toolpath {
    match adaptive_trochoidal(region, p) {
        Ok(t) => t,
        Err(e) => panic!("refused: {e}"),
    }
}

/// Every cutting move's cutter centre, sampled.
fn cut_points(t: &Toolpath) -> Vec<Point3> {
    t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).flat_map(|m| m.segments3(0.05)).flat_map(|(a, b)| [a, b]).collect()
}

/// #1589: a stand-alone slot whose band is at most the tool radius keeps
/// the plain bore it always posted, move for move.
#[test]
fn a_slot_band_within_the_tool_radius_keeps_the_plain_bore() {
    let chain = [Point2::new(0.0, 0.0), Point2::new(12.0, 0.0)];
    for width in [10.0, 12.0] {
        let tp = TrochoidParams {
            tool_diameter: D,
            width,
            loop_width: None,
            step: 1.5,
            max_engagement: 90.0,
            ret: TrochoidReturn::Microlift { height: 0.3 },
            smoothing: 0.5,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: Some(1.0),
            top_z: -2.0,
            bottom_z: -2.0,
            stepdown: 2.0,
            feed: 800.0,
            return_feed: 2000.0,
            plunge_feed: 200.0,
            climb: true,
            stock_top: None,
        };
        let Ok(plain) = plan_lane(&chain, &tp, false) else { panic!("plain {width}") };
        let Ok(slot) = super::super::plan_trochoidal_slot(&chain, &tp) else { panic!("slot {width}") };
        assert!(slot.path == plain.path, "a {width} mm slot (band {:.1} <= r 3) moved", 0.5 * (width - D));
    }
}

#[test]
fn an_empty_region_is_nothing_to_machine() {
    assert_eq!(refusal(&Region::empty(), &params()), "nothing to machine: empty pocket region");
}

#[test]
fn a_region_narrower_than_the_tool_is_refused_by_name() {
    let why = refusal(&rect(0.0, 0.0, 40.0, 5.0), &params());
    assert!(why.contains(WHO) && why.contains("narrower than the 6.000 mm tool"), "{why}");
}

#[test]
fn a_region_too_narrow_to_loop_in_is_refused_by_name_with_the_numbers() {
    let why = refusal(&rect(0.0, 0.0, 40.0, 6.1), &params());
    assert!(why.contains(WHO) && why.contains("too narrow for a 6.000 mm tool to loop in") && why.contains("6.260"), "{why}");
}

#[test]
fn a_zero_or_oversized_load_is_refused_by_name_never_looped_forever() {
    for load in [0.0, 0.6, f64::NAN] {
        let why = refusal(&face(), &PocketParams { optimal_load: load, ..params() });
        assert!(why.contains("optimal load") || why.contains("optimal-load") || why.contains("finite"), "{load}: {why}");
    }
    let why = refusal(&face(), &PocketParams { stepdown: 0.0, ..params() });
    assert!(why.contains("stepdown"), "{why}");
}

/// The law behind the construction: a band wider than the room is
/// stopped by the check on the finished path, with its ghost.
#[test]
fn a_loop_outside_the_region_is_refused_by_the_check_on_the_path() {
    let Ok(center) = face().offset(-0.5 * D) else { panic!("center") };
    let chain = [Point2::new(-9.0, 0.0), Point2::new(9.0, 0.0)];
    let err = match cut_lane(&center, &chain, 6.5, &PocketParams { bottom_z: -3.0, ..params() }, &AdaptiveTrochParams::default()) {
        Err(e) => e,
        Ok(_) => panic!("a band 1.5 mm past the walls must not pass"),
    };
    assert!(err.to_string().contains("would leave the region - nothing is cut"), "{err}");
    assert!(err.location().is_some() && err.ghost().is_some(), "the refusal names its place and shows the path");
}

#[test]
fn a_drop_past_the_helix_turn_cap_is_refused_by_name() {
    let why = refusal(&face(), &PocketParams { bottom_z: -4000.0, stepdown: 4000.0, ..params() });
    assert!(why.contains(WHO) && why.contains("bore helix") && why.contains("half the tool radius"), "{why}");
}

/// PR #1788: valid loads below 4% must not be raised to a 4% cleanup step.
#[test]
fn low_load_bounds_cleanup_spacing_and_complete_path() {
    let center = rect(-3.0, -3.0, 3.0, 3.0);
    let cleared = rect(-5.0, -5.0, 5.0, 5.0);
    let reach = rect(-6.0, -6.0, 6.0, 6.0);
    let used = rect(-1.98, -1.98, 1.98, 1.98);
    let Ok((chains, _)) =
        super::super::adaptive_troch_clean::plan_cleanup(&center, &reach, &used, &cleared, D, 0.01, true, &AdaptiveTrochParams::default())
    else {
        panic!("cleanup")
    };
    assert!(!chains.is_empty(), "the spacing check must exercise cleanup");
    // The initial safe-air front is at 5 - (3 + 0.02) = 1.98 mm.
    // Its first peel must advance by at most the requested 0.06 mm.
    let first_extent = chains[0].pts.iter().map(|q| q.x.abs().max(q.y.abs())).fold(0.0, f64::max);
    assert!(first_extent > 1.98, "cleanup advances into stock");
    assert!(first_extent <= 2.04 + 2e-3, "1% cleanup front advanced too far: {first_extent}");

    // The complete generator refuses unsupported low loads by parameter name
    // before planning any lane or emitting cleanup at an enlarged spacing.
    for load in [0.01, 0.03, 0.039999] {
        let p = PocketParams { optimal_load: load, bottom_z: -1.0, stepdown: 1.0, ..params() };
        let why = refusal(&face(), &p);
        assert!(why.contains("optimal load below 4%") && why.contains("unsupported by cleanup"), "{why}");
    }
}

#[test]
fn merged_production_checks_refuse_outside_and_self_crossing_paths() {
    let center = rect(-10.0, -10.0, 10.0, 10.0);
    let mut outside = PathBuilder::new(Point3::new(0.0, 0.0, -1.0));
    outside.push(MoveKind::Feed, Point3::new(11.0, 0.0, -1.0), Some(800.0));
    let err = check_path(&outside.finish(), &center).expect_err("outside cleanup/lane is refused");
    assert!(err.to_string().contains("would leave the region"), "{err}");
    assert!(err.location().is_some() && err.ghost().is_some());

    let mut crossed = PathBuilder::new(Point3::new(-4.0, -4.0, -1.0));
    for (x, y) in [(4.0, 4.0), (-4.0, 4.0), (4.0, -4.0)] {
        crossed.push(MoveKind::Feed, Point3::new(x, y, -1.0), Some(800.0));
    }
    let err = check_path(&crossed.finish(), &center).expect_err("crossing cleanup/lane is refused");
    assert!(err.to_string().contains("would cross themselves"), "{err}");
    assert!(err.location().is_some() && err.ghost().is_some());
}

#[test]
fn cleanup_front_respects_custom_engagement_limit() {
    let center = rect(-3.0, -3.0, 3.0, 3.0);
    let cleared = rect(-5.0, -5.0, 5.0, 5.0);
    let reach = rect(-6.0, -6.0, 6.0, 6.0);
    let used = rect(-1.98, -1.98, 1.98, 1.98);
    let t = AdaptiveTrochParams { max_engagement: 60.0, ..AdaptiveTrochParams::default() };
    let (chains, _) = super::super::adaptive_troch_clean::plan_cleanup(&center, &reach, &used, &cleared, D, 0.1, true, &t)
        .expect("cleanup with a custom engagement limit");
    assert!(!chains.is_empty(), "exercise a real cleanup peel");
    let first_extent = chains[0].pts.iter().map(|q| q.x.abs().max(q.y.abs())).fold(0.0, f64::max);
    let angle_step = 0.18 * 0.5 * D * (1.0 - t.max_engagement.to_radians().cos());
    assert!(first_extent > 1.98, "cleanup advances into stock");
    assert!(first_extent <= 1.98 + angle_step + 2e-3, "custom angle cleanup advanced too far: {first_extent}");
}

// The slow tests of this module and their light twins (#3514).
#[path = "adaptive_troch_heavy_tests.rs"]
mod heavy;
