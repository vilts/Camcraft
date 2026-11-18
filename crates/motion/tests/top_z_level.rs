//! #2004: Top Z is the top of the material a depth-stepping operation
//! cuts, so it is never a cut level: a plane there removes nothing and
//! doubles the cycle time of a one-level cut. The shared level law starts
//! one stepdown under Top Z and ends at Bottom Z exactly; a stepdown that
//! spans the whole depth is one level at Bottom Z.
//!
//! Provenance: our own regression (the maintainer's 2D Adaptive report of
//! 2026-11-15); the depth-of-cut ladder is general machining knowledge.
//!
//! Gate map: covers camcraft_motion::machining2d (levels / cut_levels,
//! pocket, adaptive_troch, contour, slots, trochoid_slot, bore, circular, face) and
//! camcraft_motion::machining3d (entry, ladder).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{
    adaptive_trochoidal, bore, circular, contour, cut_levels, cut_levels_under, face, pocket, slot, trace, BoreHole, BoreParams,
    CircularParams, ContourParams, FaceParams, Lead, PocketParams, PocketStrategy, Side, SlotParams, TraceParams,
    trochoidal_slot, TrochoidParams, TrochoidReturn, DEFAULT_MICROLIFT,
};
use camcraft_motion::machining3d::{pocket3d, Pocket3DParams};

/// The issue's one-level case: Top Z = stock top + 1, a stepdown past
/// the whole depth.
const TOP: f64 = 13.0;
const BOTTOM: f64 = 0.0;
const STEP: f64 = 50.0;

fn square(x0: f64, y0: f64, s: f64) -> Region {
    let l = Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x0 + s, y0),
        Point2::new(x0 + s, y0 + s),
        Point2::new(x0, y0 + s),
    ]);
    Region::new(vec![l.expect("loop")]).expect("region")
}

/// The planes the path cuts at: the Z of every level feed move (a feed
/// that keeps its Z), highest first, without repeats.
fn cut_planes(t: &Toolpath) -> Vec<f64> {
    let mut zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && (m.from.z - m.to.z).abs() < 1e-9)
        .map(|m| m.to.z)
        .collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    zs
}

fn one_level(what: &str, t: &Toolpath) {
    assert_eq!(cut_planes(t), vec![BOTTOM], "{what}: one level at Bottom Z, none at Top Z");
}

#[test]
fn the_level_law_starts_one_stepdown_under_top_z() {
    assert_eq!(cut_levels(20.0, 0.0, 5.0).ok(), Some(vec![15.0, 10.0, 5.0, 0.0]));
    assert_eq!(cut_levels(TOP, BOTTOM, STEP).ok(), Some(vec![0.0]));
    assert_eq!(cut_levels(13.0, 0.0, 5.0).ok(), Some(vec![8.0, 3.0, 0.0]));
    // The last step keeps the remainder (the follow-up decides equal steps).
    assert_eq!(cut_levels(20.0, 2.0, 5.0).ok(), Some(vec![15.0, 10.0, 5.0, 2.0]));
    // A stepdown of exactly the depth: one level.
    assert_eq!(cut_levels(10.0, 0.0, 10.0).ok(), Some(vec![0.0]));
    // A span of zero (or less) stays the one bottom plane.
    assert_eq!(cut_levels(1.0, 1.0, 5.0).ok(), Some(vec![1.0]));
    assert_eq!(cut_levels(0.0, 1.0, 5.0).ok(), Some(vec![1.0]));
    // A stepdown of zero stays refused.
    assert!(cut_levels(10.0, 0.0, 0.0).is_err());
    assert!(cut_levels(f64::NAN, 0.0, 1.0).is_err());
}

fn pp(strategy: PocketStrategy) -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: 6.0,
        stepover: 2.4,
        stock_to_leave: 0.0,
        clearance_z: 20.0,
        retract_z: Some(15.0),
        top_z: TOP,
        bottom_z: BOTTOM,
        stepdown: STEP,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: Links::PLANAR,
    }
}

#[test]
fn a_pocket_whose_stepdown_spans_the_depth_cuts_one_level() {
    let reg = square(0.0, 0.0, 40.0);
    for s in [PocketStrategy::OffsetRings, PocketStrategy::Adaptive] {
        let t = pocket(&reg, &pp(s)).unwrap_or_else(|e| panic!("{s:?}: {e}"));
        one_level(&format!("2D pocket {s:?}"), &t);
    }
    let t = adaptive_trochoidal(&reg, &pp(PocketStrategy::Adaptive)).expect("trochoidal");
    // The trochoidal returns between loops ride the Auto microlift, 0.3 mm
    // over the level (a feed that keeps its Z, but no cut): one cut level.
    assert_eq!(cut_planes(&t), vec![BOTTOM + 0.3, BOTTOM], "2D Adaptive (Trochoidal): one level at Bottom Z, none at Top Z");
}

#[test]
fn a_pocket_of_four_steps_cuts_the_four_planes_under_top_z() {
    let reg = square(0.0, 0.0, 40.0);
    let p = PocketParams {
        top_z: 20.0,
        stepdown: 5.0,
        clearance_z: 30.0,
        retract_z: Some(25.0),
        ..pp(PocketStrategy::OffsetRings)
    };
    let t = pocket(&reg, &p).expect("pocket");
    assert_eq!(cut_planes(&t), vec![15.0, 10.0, 5.0, 0.0]);
}

#[test]
fn a_contour_whose_stepdown_spans_the_depth_cuts_one_level() {
    let p = ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 20.0,
        retract_z: 15.0,
        top_z: TOP,
        bottom_z: BOTTOM,
        stepdown: STEP,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: Links::PLANAR,
    };
    one_level("2D contour", &contour(&square(0.0, 0.0, 30.0), &p).expect("contour"));
}

#[test]
fn a_slot_and_a_trace_whose_stepdown_spans_the_depth_cut_one_level() {
    let p = SlotParams {
        links: Links::PLANAR,
        tool_diameter: 10.0,
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 20.0,
        retract_z: None,
        top_z: TOP,
        bottom_z: BOTTOM,
        stepdown: STEP,
        stepover: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
    };
    one_level("slot", &slot(Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), &p).expect("slot"));
    let tp = TraceParams {
        clearance_z: 20.0,
        retract_z: None,
        top_z: TOP,
        bottom_z: BOTTOM,
        stepdown: STEP,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        stock_top: None,
    };
    one_level("trace", &trace(&square(0.0, 0.0, 30.0), &tp).expect("trace"));
}

#[test]
fn a_bore_whose_stepdown_spans_the_depth_cuts_one_level() {
    let p = BoreParams {
        holes: vec![BoreHole { xy: Point2::new(0.0, 0.0), radius: 8.0, top_z: TOP, floor_z: BOTTOM, through: false, stepped: false }],
        tool_diameter: 6.0,
        corner_radius: 0.0,
        stock_to_leave: 0.0,
        stepdown: STEP,
        through_allowance: 0.0,
        clearance_z: 20.0,
        retract_z: None,
        feed: 500.0,
        plunge_feed: 250.0,
        climb: true,
        tolerance: 0.01,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        links: Links::PLANAR,
    };
    one_level("bore", &bore(&p).expect("bore"));
}

fn cp(vault: bool) -> CircularParams {
    CircularParams {
        links: Links::PLANAR,
        center: Point2::new(0.0, 0.0),
        radius: 10.0,
        vault,
        bottom_z: BOTTOM,
        top_z: TOP,
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        stepover: 2.0,
        stepdown: STEP,
        clearance_z: 20.0,
        retract_z: None,
        feed: 500.0,
        plunge_feed: 250.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
    }
}

#[test]
fn a_circular_whose_stepdown_spans_the_depth_cuts_one_level() {
    one_level("circular", &circular(&square(-12.0, -12.0, 24.0), &cp(false)).expect("circular"));
}

#[test]
fn a_vault_cuts_no_ring_at_top_z_but_keeps_each_inner_ring_opening_plane() {
    let t = circular(&square(-12.0, -12.0, 24.0), &cp(true)).expect("vault");
    let planes = cut_planes(&t);
    assert!(planes.iter().all(|&z| z < TOP - 1e-6), "no level at Top Z: {planes:?}");
    // Rings at rho 7, 5, 3, 1; each ring's floor is the bowl at rho + r,
    // and each inner ring opens at the floor of the ring before it.
    let bowl = |rho: f64| BOTTOM + (TOP - BOTTOM) * (rho / 10.0f64).min(1.0).powi(2);
    // The rim ring (rho 7) has its floor at Top Z, so it cuts nothing and
    // is not walked; rho 5 opens under it at Top Z, which is not a level.
    for rho in [5.0, 3.0, 1.0] {
        let opening = bowl(rho + 2.0 + 3.0);
        let want = if opening < TOP - 1e-6 { vec![bowl(rho + 3.0), opening] } else { vec![bowl(rho + 3.0)] };
        for z in want {
            assert!(planes.iter().any(|&q| (q - z).abs() < 1e-3), "rho {rho}: plane {z} missing in {planes:?}");
        }
    }
}

#[test]
fn a_staged_face_cuts_no_level_at_top_z_and_a_plain_face_stays_one_pass() {
    let p = FaceParams {
        holes: Default::default(),
        tool_diameter: 6.0,
        stepover: 4.0,
        clearance_z: 20.0,
        cut_z: BOTTOM,
        top_z: Some(TOP),
        stepdown: Some(STEP),
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
        stock_top: None,
    };
    one_level("staged face", &face(&square(0.0, 0.0, 30.0), &p).expect("face"));
    let plain = FaceParams { top_z: None, stepdown: None, ..p };
    one_level("plain face", &face(&square(0.0, 0.0, 30.0), &plain).expect("face"));
}

/// A flat plate under the pocket, its top at Z -1: no part material
/// stands at or above any plane of the ladder.
fn plate() -> Vec<Triangle> {
    let (lo, hi) = ([-30.0, -30.0, -5.0], [70.0, 70.0, -1.0]);
    let v = |i: usize| Point3::new([lo[0], hi[0]][i & 1], [lo[1], hi[1]][(i >> 1) & 1], [lo[2], hi[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)]
        .into_iter()
        .flatten()
        .collect()
}

#[test]
fn a_3d_pocket_whose_stepdown_spans_the_depth_cuts_one_level() {
    let p = Pocket3DParams {
        links: Links::ROUGH3D,
        tool_diameter: 6.0,
        stepover: 2.4,
        stock_to_leave: 0.0,
        clearance_z: 20.0,
        top_z: TOP,
        bottom_z: BOTTOM,
        stepdown: STEP,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::OffsetRings,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.5,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
    };
    let t = pocket3d(&plate(), &square(0.0, 0.0, 40.0), &p).expect("3D pocket");
    one_level("3D pocket", &t);
}

#[test]
fn stock_over_top_z_keeps_the_level_at_top_z() {
    // #2004 round 3: material above Top Z - the first bite stays one
    // stepdown, so the level at Top Z stays (the old ladder, same floats).
    assert_eq!(cut_levels_under(10.0, Some(13.0), 0.0, 5.0).expect("law"), vec![10.0, 5.0, 0.0]);
    // Material at or under Top Z: no air level.
    assert_eq!(cut_levels_under(10.0, Some(10.0), 0.0, 5.0).expect("law"), vec![5.0, 0.0]);
    assert_eq!(cut_levels_under(13.0, Some(12.0), 0.0, 5.0).expect("law"), vec![8.0, 3.0, 0.0]);
    for (t, b, st) in [(20.0, 0.0, 5.0), (13.0, 0.0, 50.0), (20.0, 2.0, 4.6)] {
        assert_eq!(cut_levels_under(t, None, b, st).expect("law"), cut_levels(t, b, st).expect("law"));
    }
}

/// #2004 round 5: a trace (and the engrave twin, which calls it) reads the
/// stock top. Stock over Top Z keeps the level at Top Z, so the first bite
/// is at most one stepdown under the material top; without it Top Z is the
/// material top and no air level is cut.
#[test]
fn a_trace_under_stock_over_top_z_keeps_the_level_at_top_z() {
    let tp = TraceParams {
        clearance_z: 20.0,
        retract_z: None,
        top_z: 10.0,
        bottom_z: BOTTOM,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        stock_top: Some(13.0),
    };
    let reg = square(0.0, 0.0, 30.0);
    assert_eq!(cut_planes(&trace(&reg, &tp).expect("trace")), vec![10.0, 5.0, 0.0], "stock over Top Z");
    let flush = TraceParams { stock_top: Some(10.0), ..tp };
    assert_eq!(cut_planes(&trace(&reg, &flush).expect("trace")), vec![5.0, 0.0], "stock at Top Z");
    let none = TraceParams { stock_top: None, ..tp };
    assert_eq!(cut_planes(&trace(&reg, &none).expect("trace")), vec![5.0, 0.0], "no stock stated");
}

/// #2004 round 5: the trochoidal slot reads the stock top the same way.
/// Its microlift returns ride 0.3 mm over each level, so the test reads
/// the levels on the grid of the stepdown only.
#[test]
fn a_trochoidal_slot_under_stock_over_top_z_keeps_the_level_at_top_z() {
    let tp = TrochoidParams {
        tool_diameter: 10.0,
        width: 16.0,
        loop_width: None,
        step: 1.0,
        max_engagement: 90.0,
        ret: TrochoidReturn::Microlift { height: DEFAULT_MICROLIFT },
        smoothing: 1.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: Some(2.0),
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        feed: 1200.0,
        return_feed: 3000.0,
        plunge_feed: 300.0,
        climb: true,
        stock_top: Some(3.0),
    };
    let chain = [Point2::new(0.0, 0.0), Point2::new(40.0, 0.0)];
    let levels = |p: &TrochoidParams| -> Vec<f64> {
        let t = trochoidal_slot(&chain, p).unwrap_or_else(|e| panic!("trochoidal slot: {e}"));
        cut_planes(&t).into_iter().filter(|z| (z / 3.0 - (z / 3.0).round()).abs() < 1e-6).collect()
    };
    assert_eq!(levels(&tp), vec![0.0, -3.0, -6.0], "stock over Top Z");
    assert_eq!(levels(&TrochoidParams { stock_top: Some(0.0), ..tp }), vec![-3.0, -6.0], "stock at Top Z");
    assert_eq!(levels(&TrochoidParams { stock_top: None, ..tp }), vec![-3.0, -6.0], "no stock stated");
}
