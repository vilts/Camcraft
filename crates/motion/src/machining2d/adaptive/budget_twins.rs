//! Issue #1051: the planning budget's verdict does not turn on
//! nanometres. The rotary suite's r7 bar and its window, developed and
//! planned with every ring's opening eroded a few nanometres deeper than
//! it dilates back (`rings::OPEN_INSET`, #982's perturbation): every twin
//! decides each level as the unperturbed fixture does - HSM-planned, never
//! refused past the budget - and each margin attempt stays a stated
//! margin under both budget laws.
//! Gate map: covers motion::machining2d::adaptive (budget, grow) through
//! motion::multiaxis (rotary_offset). Heavy lane: release --ignored.
//! Provenance: our own regressions on `tests/rotary_adaptive_fixture`;
//! no vendor material consulted.

#[allow(dead_code)]
#[path = "../../../tests/rotary_adaptive_fixture/mod.rs"]
mod rotary;

use super::super::rings::OPEN_INSET;
use super::{GEOM_MAX, PEAK, PEAK_LEN, WORK_MAX};
use crate::machining2d::PocketStrategy;
use crate::machining3d::Pocket3DParams;
use crate::multiaxis::rotary_offset_developed;
use rotary::{LevelRead, R, bar, params, planner_overloads, read_levels, stock};

/// The openings' perturbations (nm). On Linux x86-64 the first margin
/// attempt of the r7 bar's level 5.0 runs out of margin at 0, 8, 16, 32
/// and 510 nm and finishes at 62, 125 and 250 nm; the window's levels
/// 6.0 and 5.0 finish at 8, 125 and 250 nm and run out at the others.
const TWINS_NM: [f64; 5] = [0.0, 8.0, 16.0, 62.0, 250.0];

/// Every margin attempt of every twin stays this far under the length
/// budget (measured: at most 24.7 k of 30 k tool radii, 1.21x) ...
const LEN_MARGIN: f64 = 1.15;

/// ... and this far under the geometry budget (at most 400 k of 600 k
/// vertices, 1.50x).
const GEOM_MARGIN: f64 = 1.4;

/// One twin's reading: its levels, its notes, and the most any margin
/// attempt in it spent (vertices, tool radii of revolution).
struct Twin {
    nm: f64,
    reads: Vec<LevelRead>,
    notes: Vec<String>,
    verts: u64,
    len_r: f64,
}

/// The fixture developed with the openings `nm` deeper, on this thread
/// (the hook and the peaks are thread-local).
fn twin(nm: f64, p: &Pocket3DParams) -> Twin {
    OPEN_INSET.with(|c| c.set(nm * 1e-6));
    PEAK.with(|c| c.set(0));
    PEAK_LEN.with(|c| c.set(0.0));
    let (f, st) = (bar(), stock(7.0, -16.0, 16.0));
    let dev = match rotary_offset_developed(&f, Some(&st), p) {
        Ok(t) => t,
        Err(e) => panic!("the developed ladder, openings {nm} nm deeper: {e}"),
    };
    OPEN_INSET.with(|c| c.set(0.0));
    let reads = read_levels(&dev, 7.0, 0.0, p.stepdown);
    Twin { nm, reads, notes: dev.notes, verts: PEAK.with(|c| c.get()), len_r: PEAK_LEN.with(|c| c.get()) / R }
}

/// Every twin of the fixture, each on its own thread.
fn twins(p: &Pocket3DParams) -> Vec<Twin> {
    std::thread::scope(|s| {
        let hs: Vec<_> = TWINS_NM.iter().map(|&nm| s.spawn(move || twin(nm, p))).collect();
        hs.into_iter()
            .map(|h| match h.join() {
                Ok(t) => t,
                Err(_) => panic!("a twin's plan panicked"),
            })
            .collect()
    })
}

/// The verdict every twin shares: no level refused past the planning
/// budget, every level under the skin HSM-planned (its proved arcs
/// posted, no note naming it ring-cut), no planner overload, the load
/// held; and the budget margins stated above.
fn assert_same_verdict(what: &str, ts: &[Twin], levels: &[f64]) {
    for t in ts {
        let nm = t.nm;
        println!("{what}, openings {nm} nm deeper: peak attempt {} vertices, {:.0} r", t.verts, t.len_r);
        for r in &t.reads {
            println!(
                "  radius {:.3}: {} planar moves ({} arcs), load {:.1}% D, breaks>5 {}, rings {}",
                r.rho, r.moves, r.arcs, r.load * 100.0, r.breaks, r.ringed(7.0)
            );
        }
        assert!(!t.notes.iter().any(|n| n.contains("planning budget")), "{what} at {nm} nm: refused past the budget: {:?}", t.notes);
        assert!(planner_overloads(&t.notes).is_empty(), "{what} at {nm} nm: {:?}", t.notes);
        for &rho in levels {
            let Some(r) = t.reads.iter().find(|r| (r.rho - rho).abs() < 1e-6) else { panic!("{what} at {nm} nm: radius {rho} is not cut: {:?}", t.notes) };
            assert!(r.arcs > 0 && !r.ringed(7.0) && !r.named, "{what} at {nm} nm: radius {rho} is HSM-planned: {:?}", t.notes);
            assert!(r.load <= crate::load::DEFAULT_OPTIMAL_LOAD * 1.35, "{what} at {nm} nm: radius {rho}: load {:.1}% D", r.load * 100.0);
            assert_eq!(r.breaks, 0, "{what} at {nm} nm: radius {rho}: in-cut breaks past 5 deg");
        }
        assert!(t.verts > 0 && t.verts as f64 * GEOM_MARGIN <= GEOM_MAX as f64, "{what} at {nm} nm: {} vertices in one attempt", t.verts);
        assert!(t.len_r * LEN_MARGIN <= WORK_MAX, "{what} at {nm} nm: {:.0} r in one attempt", t.len_r);
    }
}

/// The r7 bar (`rotary_adaptive`'s whole bar): levels 6.5, 6.0, 5.5 and
/// 5.0 are HSM-planned in every twin. Before #1051 level 5.0 was refused
/// past the budget at 0 and 16 nm (its first attempt ran out of margin
/// after 22.5 k r and the second inherited that spend) and planned at
/// 250 nm (the first attempt finished, 22.3 k r).
#[test]
#[ignore = "heavy lane: release --ignored (the developed r7 bar planned 5 times on 5 threads, ~1 min release)"]
fn the_r7_bars_levels_decide_the_same_way_under_sub_micron_perturbations() {
    let ts = twins(&params(PocketStrategy::Adaptive));
    // Issue #1050: level 5.0 posts no in-cut break past 5 degrees from
    // either margin (planned inside the first it posted up to 5, the
    // closing laps' fillet kinks, before `adaptive::conform`).
    assert_same_verdict("the r7 bar", &ts, &[6.5, 6.0, 5.5, 5.0]);
}

/// The window (`rotary_adaptive`'s 12 long window, 1 mm stepdown): levels
/// 6.0 and 5.0 are HSM-planned in every twin, no break past 5 degrees.
/// Before #1051 they were refused past the budget at 0 and 16 nm and
/// planned at 250 nm.
#[test]
#[ignore = "heavy lane: release --ignored (the developed window planned 5 times on 5 threads, ~1 min release)"]
fn the_windows_levels_decide_the_same_way_under_sub_micron_perturbations() {
    let p = Pocket3DParams { top_z: 16.0, bottom_z: 4.0, stepdown: 1.0, ..params(PocketStrategy::Adaptive) };
    assert_same_verdict("the window", &twins(&p), &[6.0, 5.0]);
}
