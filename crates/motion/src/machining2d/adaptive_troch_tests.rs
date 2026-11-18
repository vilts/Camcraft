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

#[test]
fn the_pocket_face_is_cleared_inside_its_region_under_the_engagement_limit() {
    let t = cut(&face(), &params());
    assert!(t.has_arcs() && t.all_finite());
    // The cutter centre never leaves the face eroded by the tool radius
    // (read on the rectangle's own numbers, not through `confined`).
    let worst = cut_points(&t).iter().map(|q| (q.x.abs() - 14.0).max(q.y.abs() - 5.0)).fold(f64::MIN, f64::max);
    assert!(worst <= 2e-3, "the cutter centre leaves the region by {worst:.4} mm");
    assert!(cut_points(&t).iter().all(|q| q.z >= -6.0 - 1e-9), "never below the bottom");
    let spec = QualitySpec::new(D);
    let Ok(mut dex) = stock_for(&face(), 0.0, -6.0, D, &spec) else { panic!("stock") };
    let Ok(q) = measure(&t, &mut dex, &spec) else { panic!("measure") };
    assert!(q.max_deg <= DEFAULT_MAX_ENGAGEMENT, "engagement {:.1} deg at {:?}", q.max_deg, q.max_at);
    assert!(q.slot_len < 1e-6, "no slotting: {:.3} mm", q.slot_len);
    assert!(t.notes.iter().any(|n| n.starts_with(WHO) && n.contains("loops of radius")), "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("clean-up pass(es) emitted")), "{:?}", t.notes);
    let Ok(r_after) = rest_area(&dex, &face(), -6.0) else { panic!("rest") };
    assert!(r_after < 20.0, "corners cleared, rest {r_after:.2} mm2");
    assert!(crate::verify::crossed_pairs(&t, super::super::ARC_FIT_TOL).is_empty(), "no feed crosses another");
    eprintln!("trochoidal 34x16 D6: {:.1} deg, {} moves, {:?}", q.max_deg, t.moves.len(), t.notes);
}

/// A band wider than the tool: the plain bore leaves a post at the
/// lane's start; the opened bore clears it, and so does the stand-alone
/// slot, which opens its bore on such a band (#1589).
#[test]
fn the_opened_bore_leaves_no_post_at_the_lane_start() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let Ok(center) = face().offset(-0.5 * D) else { panic!("center") };
    let Ok((chain, b)) = lane(&face(), &center, &p) else { panic!("lane") };
    assert!(b > 4.5 && b < 5.0 && chain.len() == 2, "band {b}, {} points", chain.len());
    // Where the post would stand: a disc well inside the bore.
    let Ok(core) = Region::sweep(&[vec![chain[0]]], b - 0.5 * D - 0.5) else { panic!("core") };
    let spec = QualitySpec::new(D);
    let rest = |t: &Toolpath| {
        let Ok(mut dex) = stock_for(&face(), 0.0, -3.0, D, &spec) else { panic!("stock") };
        let Ok(_) = measure(t, &mut dex, &spec) else { panic!("measure") };
        let Ok(a) = rest_area(&dex, &core, -3.0) else { panic!("rest") };
        a
    };
    let t = match cut_lane(&center, &chain, b, &p, &AdaptiveTrochParams::default()) {
        Ok(t) => t,
        Err(e) => panic!("cut: {e}"),
    };
    assert!(rest(&t) < 0.5, "the opened bore leaves {:.2} mm2 standing", rest(&t));
    let tp = TrochoidParams {
        tool_diameter: D,
        width: 2.0 * (b + 0.5 * D),
        loop_width: None,
        step: load_step(D, p.optimal_load),
        max_engagement: DEFAULT_MAX_ENGAGEMENT,
        ret: TrochoidReturn::Microlift { height: DEFAULT_MICROLIFT },
        smoothing: 1.5,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: Some(2.0),
        top_z: 0.0,
        bottom_z: -3.0,
        stepdown: 3.0,
        feed: 800.0,
        return_feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        stock_top: None,
    };
    let Ok(plain) = plan_lane(&chain, &tp, false) else { panic!("plain") };
    assert!(rest(&plain.path) > 4.0, "the plain bore's post: {:.2} mm2", rest(&plain.path));
    // #1589: the stand-alone slot on this band (b > r) opens its bore.
    let slot = match super::super::plan_trochoidal_slot(&chain, &tp) {
        Ok(pl) => pl,
        Err(e) => panic!("slot: {e}"),
    };
    assert!(rest(&slot.path) < 0.5, "the stand-alone slot leaves {:.2} mm2 standing", rest(&slot.path));
    assert!(slot.max_engagement <= tp.max_engagement, "engagement {:.1} deg", slot.max_engagement);
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

#[test]
fn without_lift_room_the_returns_retract_and_a_plunge_entry_is_said_unread() {
    let p = PocketParams { clearance_z: 0.1, retract_z: None, bottom_z: -3.0, entry: Entry3D::Plunge, ..params() };
    let t = cut(&face(), &p);
    let rapids = t.moves.iter().filter(|m| m.kind == MoveKind::Rapid).count();
    assert!(rapids > 40, "retract returns: {rapids} rapids");
    assert!(cut_points(&t).iter().all(|q| q.z <= 0.1 + 1e-9), "no feed above the clearance plane");
    assert!(t.notes.iter().any(|n| n.contains("the Entry setting is not read")), "{:?}", t.notes);
    let helix = cut(&face(), &PocketParams { bottom_z: -3.0, ..params() });
    assert!(!helix.notes.iter().any(|n| n.contains("Entry setting")), "{:?}", helix.notes);
}

#[test]
fn explicit_trochoidal_parameters_control_engagement_return_feed_and_loop_width() {
    let p = PocketParams { bottom_z: -3.0, ..params() };

    let impossible = adaptive_trochoidal_with(&face(), &p, &AdaptiveTrochParams { max_engagement: 10.0, ..AdaptiveTrochParams::default() })
        .expect_err("10 deg engagement must be refused")
        .to_string();
    assert!(
        impossible.contains(WHO) && impossible.contains("10.0 deg engagement limit") && impossible.contains("the least"),
        "{impossible}"
    );

    let retracted = adaptive_trochoidal_with(
        &face(),
        &p,
        &AdaptiveTrochParams { ret: Some(TrochoidReturn::Retract), ..AdaptiveTrochParams::default() },
    )
    .expect("retract generates");
    let rapids = retracted.moves.iter().filter(|m| m.kind == MoveKind::Rapid).count();
    assert!(rapids > 40, "explicit Retract forces rapid hops even with room: {rapids}");

    let fast_lift = adaptive_trochoidal_with(
        &face(),
        &p,
        &AdaptiveTrochParams {
            ret: Some(TrochoidReturn::Microlift { height: 0.4 }),
            return_feed: Some(1800.0),
            loop_width: Some(4.0),
            ..AdaptiveTrochParams::default()
        },
    )
    .expect("custom lift, return_feed and narrower loop_width");
    assert!(fast_lift.moves.iter().any(|m| m.feed == Some(1800.0)), "return feed is emitted");
    assert!(fast_lift.moves.iter().any(|m| m.feed == Some(p.feed)), "cutting feed stays p.feed");

    let wide = adaptive_trochoidal_with(&face(), &p, &AdaptiveTrochParams { loop_width: Some(50.0), ..AdaptiveTrochParams::default() })
        .expect_err("loop width wider than the room is refused")
        .to_string();
    assert!(wide.contains(WHO) && wide.contains("loop"), "{wide}");

    let bad_eng = adaptive_trochoidal_with(&face(), &p, &AdaptiveTrochParams { max_engagement: 200.0, ..AdaptiveTrochParams::default() })
        .expect_err("max_engagement > 180 is refused")
        .to_string();
    assert!(bad_eng.contains(WHO) && bad_eng.contains("max engagement"), "{bad_eng}");
}

fn check_cleared(name: &str, region: &Region, p: &PocketParams) {
    let reach = if p.stock_to_leave > 0.0 {
        region.offset(-p.stock_to_leave).unwrap_or_else(|_| panic!("{name}: reach offset"))
    } else {
        region.clone()
    };
    let center = region.offset(-(0.5 * p.tool_diameter + p.stock_to_leave)).unwrap_or_else(|_| panic!("{name}: center offset"));
    let guard = center.offset(2e-3).unwrap_or_else(|_| panic!("{name}: guard offset"));
    let t = cut(region, p);
    assert!(t.all_finite(), "{name}: non-finite move");
    assert!(t.has_arcs(), "{name}: expected arc moves");
    for q in cut_points(&t) {
        assert!(guard.contains(Point2::new(q.x, q.y)), "{name}: point ({:.3}, {:.3}) outside center", q.x, q.y);
        assert!(q.z >= p.bottom_z - 1e-9 && q.z <= p.clearance_z + 1e-9, "{name}: z={:.3} out of range", q.z);
    }
    let crossed = crate::verify::crossed_pairs(&t, super::super::ARC_FIT_TOL);
    assert!(crossed.is_empty(), "{name}: {} crossings: {:?}", crossed.len(), &crossed[..crossed.len().min(3)]);

    let spec = QualitySpec::new(p.tool_diameter);
    let Ok(mut dex) = stock_for(region, p.top_z, p.bottom_z, p.tool_diameter, &spec) else { panic!("{name}: stock") };
    let Ok(q) = measure(&t, &mut dex, &spec) else { panic!("{name}: measure") };
    assert!(q.max_deg <= DEFAULT_MAX_ENGAGEMENT + 0.5, "{name}: engagement {:.1} deg at {:?}", q.max_deg, q.max_at);
    assert!(q.slot_len < 1e-6, "{name}: slotting {:.3} mm", q.slot_len);
    let Ok(troch_rest) = rest_area(&dex, &reach, p.bottom_z) else { panic!("{name}: troch rest") };

    let Ok(hsm) = super::super::pocket::pocket(region, p) else { panic!("{name}: hsm ref") };
    let Ok(mut hdex) = stock_for(region, p.top_z, p.bottom_z, p.tool_diameter, &spec) else { panic!("{name}: hsm stock") };
    let Ok(_) = measure(&hsm, &mut hdex, &spec) else { panic!("{name}: hsm measure") };
    let Ok(hsm_rest) = rest_area(&hdex, &reach, p.bottom_z) else { panic!("{name}: hsm rest") };
    let tol = reach.area() * 0.005 + 2.0;
    assert!(troch_rest <= hsm_rest + tol, "{name}: troch rest {troch_rest:.2} mm2 exceeds hsm rest {hsm_rest:.2} mm2 (+{tol:.2})");
}

#[test]
fn wide_rectangular_pocket_is_cleared_by_parallel_lanes() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let r = rect(0.0, 0.0, 60.0, 40.0);
    check_cleared("wide 60x40 D6", &r, &p);
    check_cleared("wide 60x40 D10", &r, &PocketParams { tool_diameter: 10.0, ..p });
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

#[test]
fn pocket_with_island_is_cleared_without_crossing_or_excess_rest() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let Ok(isl) = Region::new(vec![
        poly(&[(0.0, 0.0), (66.0, 0.0), (66.0, 46.0), (0.0, 46.0)]),
        poly(&[(27.0, 17.0), (39.0, 17.0), (39.0, 29.0), (27.0, 29.0)]),
    ]) else {
        panic!("island")
    };
    let Ok(isl2) = Region::new(vec![
        poly(&[(0.0, 0.0), (60.0, 0.0), (60.0, 40.0), (0.0, 40.0)]),
        poly(&[(24.0, 14.0), (36.0, 14.0), (36.0, 26.0), (24.0, 26.0)]),
    ]) else {
        panic!("island2")
    };
    check_cleared("60x40 island D10", &isl2, &PocketParams { tool_diameter: 10.0, ..p });
    check_cleared("66x46 island D6", &isl, &p);
}

#[test]
fn ring_round_island_is_cleared_with_closed_lane() {
    let part = rect(0.0, 0.0, 60.0, 50.0);
    let (Ok(o), Ok(i)) = (part.offset(12.0), part.offset(2.0)) else { panic!("offsets") };
    let Ok(ring) = o.difference(&i) else { panic!("ring") };
    let p = PocketParams { bottom_z: -3.0, ..params() };
    check_cleared("ring D6", &ring, &p);
}

#[test]
fn triangle_and_wide_to_narrow_neck_are_cleared() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let Ok(tri) = Region::new(vec![poly(&[(0.0, 0.0), (60.0, 0.0), (0.0, 60.0)])]) else { panic!("triangle") };
    check_cleared("triangle 60x60 D6", &tri, &p);
    let Ok(neck) = Region::new(vec![poly(&[
        (0.0, 0.0),
        (30.0, 0.0),
        (30.0, 10.0),
        (60.0, 10.0),
        (60.0, 20.0),
        (30.0, 20.0),
        (30.0, 30.0),
        (0.0, 30.0),
    ])]) else {
        panic!("neck")
    };
    check_cleared("wide-to-narrow neck D6", &neck, &p);
}

#[test]
fn corner_cleanup_reduces_34x16_rest_to_at_most_hsm_rest() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    check_cleared("face 34x16 D6", &face(), &p);
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
fn custom_parameters_reach_parallel_and_closed_lane_layouts() {
    use super::super::adaptive_troch_lanes::plan_all_lanes;
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let t =
        AdaptiveTrochParams { ret: Some(TrochoidReturn::Retract), return_feed: Some(1800.0), loop_width: Some(4.0), max_engagement: 100.0 };
    let outer = rect(-30.0, -25.0, 30.0, 25.0);
    let ring = outer.offset(12.0).unwrap().difference(&outer.offset(2.0).unwrap()).unwrap();
    for region in [rect(0.0, 0.0, 60.0, 40.0), ring] {
        let center = region.offset(-0.5 * p.tool_diameter).unwrap();
        let lanes = plan_all_lanes(&region, &center, &p, &t).expect("custom lane planning");
        assert!(!lanes.layouts.is_empty());
        for layout in lanes.layouts {
            assert_eq!(layout.tp.ret, TrochoidReturn::Retract);
            assert_eq!(layout.tp.return_feed, 1800.0);
            assert!(layout.b <= 2.0 + 1e-9, "custom loop width is a cap");
            assert!(layout.tp.max_engagement <= t.max_engagement);
        }
    }
}

#[test]
fn custom_return_feed_is_emitted_by_production_parallel_lanes_and_cleanup() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let t = AdaptiveTrochParams {
        ret: Some(TrochoidReturn::Microlift { height: 0.4 }),
        return_feed: Some(1800.0),
        loop_width: Some(4.0),
        ..AdaptiveTrochParams::default()
    };
    let region = rect(0.0, 0.0, 60.0, 40.0);
    let path = adaptive_trochoidal_with(&region, &p, &t).expect("custom production multi-lane cut");
    assert!(path.notes.iter().any(|n| n.contains("lanes (")), "{:?}", path.notes);
    assert!(path.moves.iter().any(|m| m.feed == Some(1800.0)));
    assert!(path.moves.iter().any(|m| m.feed == Some(p.feed)));
    assert!(confined(&path, &region.offset(-0.5 * p.tool_diameter).unwrap()).unwrap().is_none());
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
