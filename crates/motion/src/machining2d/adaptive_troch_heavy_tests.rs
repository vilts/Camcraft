//! The slow tests of 2D Adaptive's trochoidal method (#1509), each with
//! its light twin (#3514; Gate map: covers motion::machining2d::adaptive_troch,
//! trochoid_slot::plan_trochoidal_slot). A full test runs in the heavy
//! lane at the cut; its twin runs the same law on a reduced case in the
//! light lane. A twin stays next to its full test (`cargo xtask gate light`).

use super::*;

#[test]
#[ignore = "heavy lane: motion heavy: lib (76 s in debug, #3514)"]
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
    assert!(crate::verify::crossed_pairs(&t, super::super::super::ARC_FIT_TOL).is_empty(), "no feed crosses another");
    eprintln!("trochoidal 34x16 D6: {:.1} deg, {} moves, {:?}", q.max_deg, t.moves.len(), t.notes);
}

/// The same face cut to one level (bottom -3, one 3 mm step) in place
/// of two: the same confinement, engagement, no-slot, clean-up, rest and
/// no-crossing laws on half the path.
#[test]
#[ignore = "light lane: one level of the 34x16 face in place of two (release); the full test is the_pocket_face_is_cleared_inside_its_region_under_the_engagement_limit"]
fn the_pocket_face_is_cleared_inside_its_region_under_the_engagement_limit_light() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let t = cut(&face(), &p);
    assert!(t.has_arcs() && t.all_finite());
    let worst = cut_points(&t).iter().map(|q| (q.x.abs() - 14.0).max(q.y.abs() - 5.0)).fold(f64::MIN, f64::max);
    assert!(worst <= 2e-3, "the cutter centre leaves the region by {worst:.4} mm");
    assert!(cut_points(&t).iter().all(|q| q.z >= -3.0 - 1e-9), "never below the bottom");
    let spec = QualitySpec::new(D);
    let Ok(mut dex) = stock_for(&face(), 0.0, -3.0, D, &spec) else { panic!("stock") };
    let Ok(q) = measure(&t, &mut dex, &spec) else { panic!("measure") };
    assert!(q.max_deg <= DEFAULT_MAX_ENGAGEMENT, "engagement {:.1} deg at {:?}", q.max_deg, q.max_at);
    assert!(q.slot_len < 1e-6, "no slotting: {:.3} mm", q.slot_len);
    assert!(t.notes.iter().any(|n| n.starts_with(WHO) && n.contains("loops of radius")), "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("clean-up pass(es) emitted")), "{:?}", t.notes);
    let Ok(r_after) = rest_area(&dex, &face(), -3.0) else { panic!("rest") };
    assert!(r_after < 20.0, "corners cleared, rest {r_after:.2} mm2");
    assert!(crate::verify::crossed_pairs(&t, super::super::super::ARC_FIT_TOL).is_empty(), "no feed crosses another");
}

/// A band wider than the tool: the plain bore leaves a post at the
/// lane's start; the opened bore clears it, and so does the stand-alone
/// slot, which opens its bore on such a band (#1589).
#[test]
#[ignore = "heavy lane: motion heavy: lib (140 s in debug, #3514)"]
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
    let slot = match super::super::super::plan_trochoidal_slot(&chain, &tp) {
        Ok(pl) => pl,
        Err(e) => panic!("slot: {e}"),
    };
    assert!(rest(&slot.path) < 0.5, "the stand-alone slot leaves {:.2} mm2 standing", rest(&slot.path));
    assert!(slot.max_engagement <= tp.max_engagement, "engagement {:.1} deg", slot.max_engagement);
}

/// The production lane only: its opened bore leaves no post. The plain
/// bore and the stand-alone slot stay in the full test.
#[test]
#[ignore = "light lane: the production lane of the full test only (release); the full test is the_opened_bore_leaves_no_post_at_the_lane_start"]
fn the_opened_bore_leaves_no_post_at_the_lane_start_light() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let Ok(center) = face().offset(-0.5 * D) else { panic!("center") };
    let Ok((chain, b)) = lane(&face(), &center, &p) else { panic!("lane") };
    assert!(b > 4.5 && b < 5.0 && chain.len() == 2, "band {b}, {} points", chain.len());
    let Ok(core) = Region::sweep(&[vec![chain[0]]], b - 0.5 * D - 0.5) else { panic!("core") };
    let t = match cut_lane(&center, &chain, b, &p, &AdaptiveTrochParams::default()) {
        Ok(t) => t,
        Err(e) => panic!("cut: {e}"),
    };
    let spec = QualitySpec::new(D);
    let Ok(mut dex) = stock_for(&face(), 0.0, -3.0, D, &spec) else { panic!("stock") };
    let Ok(_) = measure(&t, &mut dex, &spec) else { panic!("measure") };
    let Ok(post) = rest_area(&dex, &core, -3.0) else { panic!("rest") };
    assert!(post < 0.5, "the opened bore leaves {post:.2} mm2 standing");
}

#[test]
#[ignore = "heavy lane: motion heavy: lib (73 s in debug, #3514)"]
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

/// The cut without lift room only; the default-helix cross-check stays
/// in the full test.
#[test]
#[ignore = "light lane: the cut without lift room of the full test only (release); the full test is without_lift_room_the_returns_retract_and_a_plunge_entry_is_said_unread"]
fn without_lift_room_the_returns_retract_and_a_plunge_entry_is_said_unread_light() {
    let p = PocketParams { clearance_z: 0.1, retract_z: None, bottom_z: -3.0, entry: Entry3D::Plunge, ..params() };
    let t = cut(&face(), &p);
    let rapids = t.moves.iter().filter(|m| m.kind == MoveKind::Rapid).count();
    assert!(rapids > 40, "retract returns: {rapids} rapids");
    assert!(cut_points(&t).iter().all(|q| q.z <= 0.1 + 1e-9), "no feed above the clearance plane");
    assert!(t.notes.iter().any(|n| n.contains("the Entry setting is not read")), "{:?}", t.notes);
}

#[test]
#[ignore = "heavy lane: motion heavy: lib (334 s in debug, #3514)"]
fn explicit_trochoidal_parameters_control_engagement_return_feed_and_loop_width() {
    let face = || rect(-15.0, -8.0, 15.0, 8.0);
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

/// The three refusals by name and the custom lift with its return feed,
/// on a 16x12 face in place of 30x16 (the 10 deg refusal searches the
/// whole face for its least engagement: 16 s on 30x16, 3.6 s on 16x12 in
/// release). The explicit Retract cut stays in the full test; the twin of
/// `without_lift_room_..._light` holds the retract returns.
#[test]
#[ignore = "light lane: a 16x12 face in place of 30x16, without the Retract cut (release); the full test is explicit_trochoidal_parameters_control_engagement_return_feed_and_loop_width"]
fn explicit_trochoidal_parameters_control_engagement_return_feed_and_loop_width_light() {
    let face = || rect(-8.0, -6.0, 8.0, 6.0);
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let with = |t: &AdaptiveTrochParams| adaptive_trochoidal_with(&face(), &p, t);
    let refused = |t: &AdaptiveTrochParams| match with(t) {
        Err(e) => e.to_string(),
        Ok(path) => panic!("not refused: {} moves", path.moves.len()),
    };
    let impossible = refused(&AdaptiveTrochParams { max_engagement: 10.0, ..AdaptiveTrochParams::default() });
    assert!(
        impossible.contains(WHO) && impossible.contains("10.0 deg engagement limit") && impossible.contains("the least"),
        "{impossible}"
    );
    let custom = AdaptiveTrochParams {
        ret: Some(TrochoidReturn::Microlift { height: 0.4 }),
        return_feed: Some(1800.0),
        loop_width: Some(4.0),
        ..AdaptiveTrochParams::default()
    };
    let Ok(fast_lift) = with(&custom) else { panic!("custom lift, return_feed and narrower loop_width") };
    assert!(fast_lift.moves.iter().any(|m| m.feed == Some(1800.0)), "return feed is emitted");
    assert!(fast_lift.moves.iter().any(|m| m.feed == Some(p.feed)), "cutting feed stays p.feed");
    let wide = refused(&AdaptiveTrochParams { loop_width: Some(50.0), ..AdaptiveTrochParams::default() });
    assert!(wide.contains(WHO) && wide.contains("loop"), "{wide}");
    let bad_eng = refused(&AdaptiveTrochParams { max_engagement: 200.0, ..AdaptiveTrochParams::default() });
    assert!(bad_eng.contains(WHO) && bad_eng.contains("max engagement"), "{bad_eng}");
}

#[test]
#[ignore = "heavy lane: motion heavy: lib (332 s in debug, #3514)"]
fn custom_parameters_reach_parallel_and_closed_lane_layouts() {
    use super::super::super::adaptive_troch_lanes::plan_all_lanes;
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let t =
        AdaptiveTrochParams { ret: Some(TrochoidReturn::Retract), return_feed: Some(1800.0), loop_width: Some(4.0), max_engagement: 100.0 };
    let outer = rect(-6.0, -5.0, 6.0, 5.0);
    let ring = outer.offset(12.0).unwrap().difference(&outer.offset(2.0).unwrap()).unwrap();
    for region in [rect(0.0, 0.0, 30.0, 24.0), ring] {
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

/// The parallel layout of the 30x24 pocket only; the closed-lane ring
/// stays in the full test.
#[test]
#[ignore = "light lane: the 30x24 parallel layout of the full test only (release); the full test is custom_parameters_reach_parallel_and_closed_lane_layouts"]
fn custom_parameters_reach_parallel_and_closed_lane_layouts_light() {
    use super::super::super::adaptive_troch_lanes::plan_all_lanes;
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let t =
        AdaptiveTrochParams { ret: Some(TrochoidReturn::Retract), return_feed: Some(1800.0), loop_width: Some(4.0), max_engagement: 100.0 };
    let region = rect(0.0, 0.0, 30.0, 24.0);
    let Ok(center) = region.offset(-0.5 * p.tool_diameter) else { panic!("center") };
    let Ok(lanes) = plan_all_lanes(&region, &center, &p, &t) else { panic!("custom lane planning") };
    assert!(!lanes.layouts.is_empty());
    for layout in lanes.layouts {
        assert_eq!(layout.tp.ret, TrochoidReturn::Retract);
        assert_eq!(layout.tp.return_feed, 1800.0);
        assert!(layout.b <= 2.0 + 1e-9, "custom loop width is a cap");
        assert!(layout.tp.max_engagement <= t.max_engagement);
    }
}

#[test]
#[ignore = "heavy lane: motion heavy: lib (260 s in debug, #3514)"]
fn custom_return_feed_is_emitted_by_production_parallel_lanes_and_cleanup() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let t = AdaptiveTrochParams {
        ret: Some(TrochoidReturn::Microlift { height: 0.4 }),
        return_feed: Some(1800.0),
        loop_width: Some(4.0),
        ..AdaptiveTrochParams::default()
    };
    let region = rect(0.0, 0.0, 30.0, 24.0);
    let path = adaptive_trochoidal_with(&region, &p, &t).expect("custom production multi-lane cut");
    assert!(path.notes.iter().any(|n| n.contains("lanes (")), "{:?}", path.notes);
    assert!(path.moves.iter().any(|m| m.feed == Some(1800.0)));
    assert!(path.moves.iter().any(|m| m.feed == Some(p.feed)));
    assert!(confined(&path, &region.offset(-0.5 * p.tool_diameter).unwrap()).unwrap().is_none());
}

/// A smaller multi-lane pocket, 24x20 in place of 30x24: the same custom
/// return feed on parallel lanes and clean-up, inside the region.
#[test]
#[ignore = "light lane: a 24x20 pocket in place of 30x24 (release); the full test is custom_return_feed_is_emitted_by_production_parallel_lanes_and_cleanup"]
fn custom_return_feed_is_emitted_by_production_parallel_lanes_and_cleanup_light() {
    let p = PocketParams { bottom_z: -3.0, ..params() };
    let t = AdaptiveTrochParams {
        ret: Some(TrochoidReturn::Microlift { height: 0.4 }),
        return_feed: Some(1800.0),
        loop_width: Some(4.0),
        ..AdaptiveTrochParams::default()
    };
    let region = rect(0.0, 0.0, 24.0, 20.0);
    let Ok(path) = adaptive_trochoidal_with(&region, &p, &t) else { panic!("custom production multi-lane cut") };
    assert!(path.notes.iter().any(|n| n.contains("lanes (")), "{:?}", path.notes);
    assert!(path.moves.iter().any(|m| m.feed == Some(1800.0)));
    assert!(path.moves.iter().any(|m| m.feed == Some(p.feed)));
    let Ok(center) = region.offset(-0.5 * p.tool_diameter) else { panic!("center") };
    assert!(matches!(confined(&path, &center), Ok(None)), "the path stays in the region");
}
