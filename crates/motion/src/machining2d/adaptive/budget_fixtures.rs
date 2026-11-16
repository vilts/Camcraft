//! Issue #850: the growth plans' geometry budget (`budget`, law 3)
//! measured on our own pockets - every fixture that grows today stays
//! well under it, and the island trips it by name (fast, at lowered
//! limits; the full island is pinned in `adaptive_hsm`'s heavy lane).
//! Gate map: covers motion::machining2d::adaptive (budget, grow).
//! Provenance: our own regressions; no vendor material consulted.

use super::super::{AdaptiveSpec, plan_within};
use super::{GEOM_MAX, PEAK, Work};
use crate::cl::Toolpath;
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::load::DEFAULT_OPTIMAL_LOAD;
use crate::machining2d::{PocketParams, PocketStrategy, pocket};

/// Every fixture that grows today stays at least this many times under
/// [`GEOM_MAX`].
const MARGIN: f64 = 1.9;

fn rect_loop(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    let pts = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    let Ok(l) = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()) else { panic!("fixture loop") };
    l
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let Ok(r) = Region::new(vec![rect_loop(x0, y0, x1, y1)]) else { panic!("fixture region") };
    r
}

/// `adaptive_hsm`'s pocket: D10, two levels to Z -5, Helix R2.
fn params() -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: 10.0,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: Some(1.0),
        top_z: 0.0,
        bottom_z: -5.0,
        stepdown: 5.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: crate::links::Links::PLANAR,
    }
}

/// The pocket, and the most geometry any growth plan in it spent.
fn spent(reg: &Region) -> (Toolpath, u64) {
    PEAK.with(|p| p.set(0));
    let Ok(t) = pocket(reg, &params()) else { panic!("pocket") };
    (t, PEAK.with(|p| p.get()))
}

/// The island's growth refused at lowered law-3 limits (the planner's
/// own loop, [`Work::with_limits`]): named, through the plan's `refused`
/// path - no passes, so `pocket_hsm` hands the area to the Offset rings
/// by that name (as `adaptive_band` pins for #13's up-front refusal). The
/// fast default-suite twin of `adaptive_hsm`'s heavy island pin.
#[test]
fn the_island_trips_lowered_geometry_limits_by_name() {
    let r = 5.0;
    let isl = Region::new(vec![
        rect_loop(0.0, 0.0, 60.0, 40.0),
        rect_loop(24.0, 14.0, 36.0, 26.0),
    ]);
    let Ok(isl) = isl else { panic!("island") };
    let (Ok(center), seed) = (isl.offset(-r), Point2::new(13.0, 18.0)) else { panic!("center") };
    let Ok(reach) = center.offset(r) else { panic!("reach") };
    let spec = AdaptiveSpec {
        r,
        load: DEFAULT_OPTIMAL_LOAD,
        tol: 0.05,
        climb: true,
        seed,
        start: Point2::new(seed.x - 2.0, seed.y),
        bore: r + 2.0,
        helix_r: 2.0,
        smooth: 2.5,
    };
    let t0 = std::time::Instant::now();
    let Ok(pl) = plan_within(&reach, &center, &spec, Work::with_limits(20_000, 200.0)) else { panic!("plan") };
    eprintln!("island at lowered limits: {:.1} s, {:?}", t0.elapsed().as_secs_f64(), pl.refused);
    let Some(why) = &pl.refused else { panic!("refused: {:?}", pl.notes) };
    assert!(why.contains("too detailed and its path too long") && why.contains("both limits passed") && why.contains("(limit 20 k)") && why.contains("(limit 0.2 k)"), "{why}");
    assert!(pl.passes.is_empty() && pl.joins.is_empty(), "a refused plan has no passes: the Offset rings take the area");
}

/// The pockets that grow today keep their plans: each spends at most
/// `1 / MARGIN` of the budget (measured: slot 252 k, channel 175 k) and
/// is never refused by it. (The slot's own plan is pinned in the debug
/// suite too, `adaptive::tests`.)
#[test]
#[ignore = "heavy lane: release --ignored (the 60 x 14 slot and 60 x 22 channel pockets grown whole, ~140 s debug)"]
fn the_fixtures_that_grow_today_stay_under_the_geometry_budget() {
    for (name, reg) in [("narrow slot 14x60", rect(0.0, 0.0, 60.0, 14.0)), ("channel 22x60", rect(0.0, 0.0, 60.0, 22.0))] {
        let (t, peak) = spent(&reg);
        eprintln!("{name}: {peak} vertices ({:.2} of the budget)", peak as f64 / GEOM_MAX as f64);
        assert!(peak > 0, "{name} grows");
        assert!(peak as f64 * MARGIN <= GEOM_MAX as f64, "{name}: {peak} vertices, under {MARGIN}x margin");
        assert!(!t.notes.iter().any(|n| n.contains("too detailed")), "{name}: {:?}", t.notes);
    }
}
