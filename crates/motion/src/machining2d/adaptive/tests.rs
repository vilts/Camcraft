//! The planner's own laws (Gate map (M11.4 C): covers
//! motion::machining2d::adaptive).

use super::*;

#[test]
fn the_planner_stock_reads_a_side_cut_as_the_yardstick_does() {
    // A channel, then a parallel pass 2 mm beside it: the planner's
    // stock reads the angle the measuring stock's law gives a straight
    // wall at that width on the same rim (`quality::rim_for`).
    let Ok(sq) = Region::square(60.0) else { panic!("square") };
    let Ok(mut ras) = Raster::from_region(&sq, 0.1, 0.0) else { panic!("ras") };
    let rim = crate::quality::rim_for(10.0, 0.1);
    ras.walk(&[Point2::new(5.0, 30.0), Point2::new(55.0, 30.0)], 5.0, rim, 0.625);
    let (deg, at) = ras.walk(&[Point2::new(5.0, 32.0), Point2::new(40.0, 32.0)], 5.0, rim, 0.625);
    let want = crate::load::deg_at_rim(0.2, 10.0, rim);
    assert!((deg - want).abs() < 3.0, "a 2 mm side cut reads {deg} at {at:?}, want {want}");
}

#[test]
fn a_degenerate_spec_is_refused_not_planned() {
    let Ok(sq) = Region::square(30.0) else { panic!("square") };
    let spec = AdaptiveSpec {
        r: 0.0,
        load: 0.1,
        tol: 0.0,
        climb: true,
        seed: Point2::new(15.0, 15.0),
        start: Point2::new(15.0, 15.0),
        bore: 0.0,
        helix_r: 0.0,
        smooth: 1.0,
    };
    assert!(plan(&sq, &sq, &spec).is_err());
}

/// The 60 x 14 slot (1.4 D wide; `dy` wider) as `pocket_hsm` plans it:
/// the center area, what the cutter standing in it can reach, its plan.
fn slot_plan(dy: f64) -> (Region, Region, AdaptivePlan) {
    let pts = vec![Point2::new(0.0, 0.0), Point2::new(60.0, 0.0), Point2::new(60.0, 14.0 + dy), Point2::new(0.0, 14.0 + dy)];
    let (Ok(l), r) = (crate::geo2d::Loop::new(pts), 5.0) else { panic!("loop") };
    let Ok(slot) = Region::new(vec![l]) else { panic!("slot") };
    let (Ok(center), r) = (slot.offset(-r), r) else { panic!("center") };
    let Ok(reach) = center.offset(r) else { panic!("reach") };
    let seed = Point2::new(30.0, 7.0);
    let spec = AdaptiveSpec {
        r,
        load: crate::load::DEFAULT_OPTIMAL_LOAD,
        tol: 0.05,
        climb: true,
        seed,
        start: Point2::new(30.0 - 1.2, 7.0),
        bore: r + 1.2,
        helix_r: 1.2,
        smooth: 2.5,
    };
    let Ok(pl) = plan(&reach, &center, &spec) else { panic!("plan") };
    (center, reach, pl)
}

/// The slot's verdict: cut where it bites, its pieces joined by microlift
/// returns (one join a pass, nine in ten of them lifted), never crossing,
/// the load held - no overload named, the lifted plan not dropped for the
/// bare spiral - and nothing but grid crumbs left.
fn assert_lifted_within_the_load(what: &str, reach: &Region, pl: &AdaptivePlan) {
    assert!(pl.passes.len() > 1 && pl.joins.len() == pl.passes.len(), "{what}: lifted pieces: {:?}", pl.notes);
    assert!(pl.joins.iter().skip(1).filter(|j| j.is_some()).count() * 10 >= 9 * (pl.passes.len() - 1), "{what}: returns lifted: {:?}", pl.notes);
    assert!(first_crossing(&pl.passes).is_none(), "{what}: {:?}", pl.notes);
    assert!(!pl.notes.iter().any(|n| n.contains("exceeds") || n.contains("overload") || n.contains("posted as planned")), "{what}: {:?}", pl.notes);
    assert!(pl.rest < crate::machining2d::pocket_hsm::HSM_REST_MAX * reach.area(), "{what}: rest {} {:?}", pl.rest, pl.notes);
}

/// Round 3: a 60 x 14 slot (1.4 D wide) is no star cell from its middle; it
/// is grown as one ring family - never crossing itself, the load held,
/// nothing but the named corner scallops left. Round 4: cut where it
/// bites, its pieces joined by microlift returns (one join a pass).
#[test]
fn a_channel_is_grown_as_one_spiral_that_never_crosses() {
    budget::PEAK.with(|p| p.set(0));
    let (center, reach, pl) = slot_plan(0.0);
    // Issue #850: it spends ~246 k of the 600 k geometry budget.
    let peak = budget::PEAK.with(|p| p.get());
    assert!(peak > 0 && peak as f64 * 1.9 <= budget::GEOM_MAX as f64, "{peak} vertices");
    assert_lifted_within_the_load("the slot", &reach, &pl);
    // Every point stays in the center area (the walls are never cut).
    let Ok(grown) = center.offset(1e-3) else { panic!("grown") };
    assert!(pl.passes.iter().flatten().all(|&q| grown.contains(q)));
}

/// The slot planned with every ring's opening eroded `inset` mm deeper
/// than it dilates back (`rings::OPEN_INSET`), on a slot `dy` mm wider.
fn perturbed_slot(inset: f64, dy: f64) -> (Region, AdaptivePlan) {
    rings::OPEN_INSET.with(|c| c.set(inset));
    let (_, reach, pl) = slot_plan(dy);
    rings::OPEN_INSET.with(|c| c.set(0.0));
    (reach, pl)
}

/// Issue #982: the slot's verdict does not turn on nanometres. Its twin
/// with every opening 32 nm deeper read 48.0 deg on the lifted plan
/// against the spiral's 47.5 before the fix (a sliver the pieces' longer
/// chords left, one rim sample), which dropped the lifted plan for the
/// bare spiral - and that one falls to Offset rings on the travel law.
/// The twin now decides as the slot does: lifted, the load held.
#[test]
fn the_slot_and_its_32_nm_twin_decide_the_same_way() {
    let (reach, pl) = perturbed_slot(3.2e-5, 0.0);
    assert_lifted_within_the_load("the opening 32 nm deeper", &reach, &pl);
}

/// Issue #982's sweep: openings 8 nm to 0.25 um deeper, and the slot
/// itself 8 nm to 0.51 um wider or narrower. Before the fix the lifted
/// plan was dropped at 8, 32, 48 and 180 nm of opening and at -125, -62,
/// -8, +8, +32 and +510 nm of width (Linux x86-64; other platforms'
/// libm flips other rows). Every twin now decides as the slot does.
#[test]
#[ignore = "heavy lane: release --ignored (the slot planned 20 times, ~5 s each in release, minutes in debug)"]
fn the_slot_keeps_its_verdict_under_sub_micron_perturbations() {
    for nm in [8.0, 16.0, 24.0, 32.0, 48.0, 62.0, 90.0, 125.0, 180.0, 250.0] {
        let (reach, pl) = perturbed_slot(nm * 1e-6, 0.0);
        assert_lifted_within_the_load(&format!("the opening {nm} nm deeper"), &reach, &pl);
    }
    for nm in [-250.0, -125.0, -62.0, -32.0, -8.0, 8.0, 32.0, 62.0, 250.0, 510.0] {
        let (reach, pl) = perturbed_slot(0.0, nm * 1e-6);
        assert_lifted_within_the_load(&format!("the slot {nm} nm wider"), &reach, &pl);
    }
}
