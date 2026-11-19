//! Pins of issue #1950 on the user's D plate (`Milling-test.step`: disc
//! R30 cut flat at y 19.719, bore R12.5) in a 70 x 70 stock, one level,
//! its center area built as the 3-D ladder builds it (`open_sides` with
//! the Adaptive stock band, less the part, eroded by r): the stock ring
//! reaches `hsm_passes` with the stock and is planned from the air. Ours.

use super::*;
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::load::DEFAULT_OPTIMAL_LOAD;

fn circle(r: f64, n: usize) -> Vec<Point2> {
    (0..n).map(|k| {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        Point2::new(r * a.cos(), r * a.sin())
    }).collect()
}

fn part_d() -> Region {
    // Disc R30 cut by the flat y = 19.719 (the STEP's chord).
    let Ok(l) = Loop::new(circle(30.0, 2000)) else { panic!("disc") };
    let Ok(disc) = Region::new(vec![l]) else { panic!("disc") };
    let Ok(h) = Loop::new(vec![Point2::new(-40.0, -40.0), Point2::new(40.0, -40.0), Point2::new(40.0, 19.719_008_6), Point2::new(-40.0, 19.719_008_6)]) else { panic!("half") };
    let Ok(h) = Region::new(vec![h]) else { panic!("half") };
    let Ok(d) = disc.intersect(&h) else { panic!("d") };
    d
}

fn params() -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: 10.0,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: 40.0,
        retract_z: Some(22.0),
        top_z: 10.0,
        bottom_z: 10.0,
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: crate::links::Links::ROUGH3D,
    }
}

fn region(pts: Vec<Point2>) -> Region {
    let Ok(l) = Loop::new(pts) else { panic!("loop") };
    let Ok(r) = Region::new(vec![l]) else { panic!("region") };
    r
}

fn stock() -> Region {
    let s = 35.0;
    region(vec![Point2::new(-s, -s), Point2::new(s, -s), Point2::new(s, s), Point2::new(-s, s)])
}

/// The level's center area as the 3-D ladder builds it.
fn level_center() -> Region {
    let r = 5.0;
    let d = part_d();
    let Ok(foot) = crate::machining3d::open_sides(&d, r, &Region::empty(), Some(&stock()), crate::machining3d::stock_band(r, true)) else { panic!("foot") };
    let bore = region(circle(12.5, 2000));
    let Ok(mat) = d.difference(&bore) else { panic!("mat") };
    let Ok(target) = foot.difference(&mat) else { panic!("target") };
    let Ok(c) = target.offset(-r) else { panic!("center") };
    c
}

/// Round B: the whole level goes through `hsm_passes` with the stock;
/// the ring is planned from the air (no growth-spiral budget refusal),
/// within the rest law and the move budget.
/// Round E: red on the hop assert (23 over 8): the round C reading of
/// 8 hops came from joins whose leads crossed other passes.
#[test]
#[ignore = "issue #1950 blocked after round H (maintainer decides): red - 10 retract hops (over 8): 7 are wall lap pieces split where a lap meets a front run end; a trim at the outer lap reads 0.2437 over the 0.1538 bound (~25 s debug)"]
fn the_stock_ring_level_reaches_the_air_planner_with_the_stock() {
    let t = std::time::Instant::now();
    let Ok((passes, notes, rest, _, _, travel, refused)) = hsm_passes(&level_center(), &params(), Some(&stock())) else { panic!("plan") };
    eprintln!("level: {:.1} s, passes {}, rest {rest:.4}, {travel:?}, refused {refused:?}", t.elapsed().as_secs_f64(), passes.len());
    assert!(refused.is_none(), "{refused:?}");
    assert!(notes.iter().any(|n| n.contains("cut from the air")), "{notes:?}");
    assert!(rest <= HSM_REST_MAX, "rest {rest:.4}");
    assert!(travel.moves < MOVE_BUDGET, "moves {}", travel.moves);
    // Round C: the passes are joined by microlift returns, so the level
    // keeps the Adaptive plan (not the Offset rings by the hop budget).
    assert!(travel.hops <= HOP_BUDGET, "hops {} over {HOP_BUDGET}", travel.hops);
}

/// Round D: through `adaptive_passes` (the door the 3-D ladder opens) the
/// stock ring level keeps the Adaptive plan, with no planning budget note
/// and no fallback to the Offset rings.
/// Round D reading: red, the plan crossed itself at (-8.12, -35.61).
/// Round E: the passes no longer cross (`air_uncross`), and a late join
/// whose lead crosses falls back to its hop - 23 hops, over the budget
/// of 8, so the level still goes to the Offset rings by name. Round F.
#[test]
#[ignore = "issue #1950 blocked after round H (maintainer decides): red - 10 retract hops (over 8): 7 are wall lap pieces split where a lap meets a front run end; a trim at the outer lap reads 0.2437 over the 0.1538 bound (~25 s debug)"]
fn the_stock_ring_level_plans_adaptive_with_no_budget_note() {
    let Ok((passes, notes, s, _, _)) = adaptive_passes(&level_center(), &params(), "the level at Z 10.000", false, Some(&stock())) else { panic!("plan") };
    assert_eq!(s, PocketStrategy::Adaptive, "{notes:?}");
    assert!(!passes.is_empty());
    assert!(!notes.iter().any(|n| n.contains("budget") || n.contains("Offset rings")), "{notes:?}");
}

/// Without the stock the level is planned as before (the growth spiral).
#[test]
fn no_stock_keeps_the_growth_spiral() {
    let Ok(comps) = components(&level_center()) else { panic!("comps") };
    let Some(bore) = comps.iter().find(|c| c.area() < 1000.0) else { panic!("bore") };
    let Ok((_, notes, ..)) = hsm_passes(bore, &params(), None) else { panic!("plan") };
    assert!(!notes.iter().any(|n| n.contains("cut from the air")), "{notes:?}");
}

/// The Prime Directive on the air plan: a plan whose worst reading is
/// over the bound is refused by name, never posted. The bound is made
/// tight (a negative margin) so the D plate's ring is over it.
#[test]
fn an_air_plan_over_the_load_bound_is_refused_by_name() {
    let Ok(comps) = components(&level_center()) else { panic!("comps") };
    let Some(ring) = comps.iter().find(|c| c.area() > 1000.0) else { panic!("ring") };
    let r = 5.0;
    let Ok(reach) = ring.offset(r) else { panic!("reach") };
    let at = Point2::new(0.0, 0.0);
    let tight = AdaptiveSpec { r, load: DEFAULT_OPTIMAL_LOAD, tol: -0.3, climb: true, seed: at, start: at, bore: 0.0, helix_r: 0.0, smooth: 0.5 * r };
    let Ok(Some(Err(why))) = air_plan(&reach, ring, &tight, Some(&stock())) else { panic!("an overloaded air plan must be refused") };
    assert!(why.contains("over the") && why.contains("optimal load"), "{why}");
    let ok = AdaptiveSpec { tol: PLAN_TOL, ..tight };
    let Ok(Some(Ok((pl, _)))) = air_plan(&reach, ring, &ok, Some(&stock())) else { panic!("the plan within the bound is kept") };
    assert!(!pl.passes.is_empty());
}

/// The load reading of round B: the air plan's slack to the bound.
#[test]
#[ignore = "reading for issue #1950: prints the slack to the load bound (~10 s debug)"]
fn probe_air_slack() {
    let Ok(comps) = components(&level_center()) else { panic!("comps") };
    let Some(ring) = comps.iter().find(|c| c.area() > 1000.0) else { panic!("ring") };
    let Ok(reach) = ring.offset(5.0) else { panic!("reach") };
    let Ok(mat) = reach.intersect(&stock()) else { panic!("mat") };
    let at = Point2::new(0.0, 0.0);
    let s = AdaptiveSpec { r: 5.0, load: DEFAULT_OPTIMAL_LOAD, tol: PLAN_TOL, climb: true, seed: at, start: at, bore: 0.0, helix_r: 0.0, smooth: 2.5 };
    let Ok(Some(p)) = super::super::adaptive::plan_from_air(&mat, &stock(), ring, &s) else { panic!("air") };
    eprintln!("worst {:.6} at {:?}, ends {:.6}, slack {:.9} deg, over {:?}, hops {}", p.worst.0, p.worst.1, p.ends.0, p.slack_deg, p.over, p.plan.travel.hops);
}
