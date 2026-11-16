//! Issue #882: the outline thinning ahead of the growth rings' opening,
//! measured - the bound (`within` stays inside its tolerance; the
//! opening of a finely tessellated area stays inside that area, within
//! a micron of the plain opening) and charged growth vertices (a fine
//! outline stays near the count of a coarse one, not its wall time).
//! Gate map: covers motion::machining2d::adaptive (rings, simplify).
//! Provenance: our own regressions; no vendor material consulted.

use super::{seg_dist, within};
use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::adaptive::front::densify;
use crate::machining2d::adaptive::loops::boundary_dist;
use crate::machining2d::adaptive::budget::PEAK;
use crate::machining2d::adaptive::rings::{ARC_TOL, INSET, SIMPLIFY, open};
use crate::machining2d::adaptive::{AdaptiveSpec, first_crossing, plan};

fn region(loops: Vec<Loop>) -> Region {
    let Ok(r) = Region::new(loops) else { panic!("fixture region") };
    r
}

/// A circle of `n` chords: at 6000 on R10 the chord sag is 1.4 nm, the
/// resolution a STEP bore's outline arrives at.
fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Loop {
    let pts = (0..n).map(|k| {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        Point2::new(cx + r * a.cos(), cy + r * a.sin())
    });
    let Ok(l) = Loop::new(pts.collect()) else { panic!("fixture circle") };
    l
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    let Ok(l) = Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]) else { panic!("fixture rect") };
    l
}

/// A slot `len` x `w` from the origin with round ends of `n` chords each.
fn stadium(len: f64, w: f64, n: usize) -> Region {
    let (r, mut pts) = (0.5 * w, Vec::with_capacity(2 * n + 2));
    for (cx, a0) in [(len - r, -std::f64::consts::FRAC_PI_2), (r, std::f64::consts::FRAC_PI_2)] {
        for k in 0..=n {
            let a = a0 + std::f64::consts::PI * k as f64 / n as f64;
            pts.push(Point2::new(cx + r * a.cos(), r + r * a.sin()));
        }
    }
    let Ok(l) = Loop::new(pts) else { panic!("fixture stadium") };
    region(vec![l])
}

fn verts(r: &Region) -> usize {
    r.loops().iter().map(|l| l.points().len()).sum()
}

/// The farthest any vertex of `a` lies from `b`'s boundary.
fn reach(a: &Region, b: &Region) -> f64 {
    a.loops().iter().flat_map(|l| l.points()).map(|&p| boundary_dist(b, p)).fold(0.0, f64::max)
}

/// A keyhole with an island: a fine R12 disc, a 30 x 8 slot out of it
/// (reflex and sharp corners) and a fine R3 island (a reflex arc).
fn keyhole() -> Region {
    let disc = region(vec![circle(0.0, 0.0, 12.0, 6000)]);
    let (Ok(body), island) = (disc.union(&region(vec![rect(0.0, -4.0, 30.0, 4.0)])), region(vec![circle(-5.0, 0.0, 3.0, 3000)])) else { panic!("keyhole body") };
    let Ok(k) = body.difference(&island) else { panic!("keyhole") };
    k
}

/// PR #1376 review: the opening's extra erosion is the containment
/// guarantee, so it must exceed the thinning tolerance plus the
/// round-join flattening. A later tidy must not shrink it on the
/// strength of the intersection that only backs it up.
#[test]
fn the_opening_inset_exceeds_thinning_plus_flattening() {
    let (inset, margin) = (std::hint::black_box(INSET), SIMPLIFY + ARC_TOL);
    assert!(inset > margin, "INSET {inset} mm must exceed SIMPLIFY + ARC_TOL = {margin} mm");
}

#[test]
fn a_fine_circle_thins_to_the_tolerance_and_never_past_it() {
    let fine = region(vec![circle(0.0, 0.0, 10.0, 6000)]);
    let Some(light) = within(&fine, SIMPLIFY) else { panic!("a 6000-chord circle thins") };
    // The fewest chords a 0.25 um sag allows on R10 is 445.
    let n = verts(&light);
    assert!((445..900).contains(&n), "{n} vertices kept of {}", verts(&fine));
    // The erosion's work goes with the square of its vertices: under
    // 1/40 of the outline's as it came.
    assert!(n * n * 40 < 6000 * 6000);
    // Every vertex kept is a vertex of the outline ...
    let orig = fine.loops()[0].points();
    assert!(light.loops().iter().flat_map(|l| l.points()).all(|p| orig.iter().any(|q| q == p)));
    // ... and no vertex dropped is farther than the tolerance from the
    // thinned outline (1 nm: the lattice the region is rebuilt on).
    let off = reach(&fine, &light);
    assert!(off <= SIMPLIFY + 1e-6, "a dropped vertex stands {off} mm off");
    assert!((light.area() - fine.area()).abs() < SIMPLIFY * std::f64::consts::TAU * 10.0);
}

#[test]
fn an_outline_already_at_the_tolerance_is_left_as_it_is() {
    // Sharp corners, and round joins flattened at the rings' own arc
    // tolerance: nothing to drop, and the region is handed back whole.
    let sq = region(vec![rect(0.0, 0.0, 20.0, 10.0)]);
    assert!(within(&sq, SIMPLIFY).is_none());
    let Ok(round) = sq.offset_fine(-3.0, ARC_TOL).and_then(|e| e.offset_fine(3.0, ARC_TOL)) else { panic!("rounded") };
    let kept = within(&round, SIMPLIFY).map_or(verts(&round), |l| verts(&l));
    assert!(kept * 10 >= verts(&round) * 9, "{kept} of {}", verts(&round));
    // Degenerate tolerances and a sliver thinner than the tolerance are
    // never simplified away.
    assert!(within(&sq, 0.0).is_none() && within(&sq, f64::NAN).is_none() && within(&sq, -1.0).is_none());
    let sliver = region(vec![rect(0.0, 0.0, 5.0, 1e-4)]);
    assert!(within(&sliver, SIMPLIFY).is_none_or(|l| verts(&l) >= 3 && l.area() > 0.0));
    assert!(seg_dist(Point2::new(3.0, 4.0), Point2::new(0.0, 0.0), Point2::new(0.0, 0.0)) == 5.0);
}

/// Every boundary sample of `inner` (20 um apart) outside `area`.
fn outside(inner: &Region, area: &Region) -> usize {
    inner.loops().iter().map(|l| densify(l.points(), 0.02, true).iter().filter(|&&p| !area.contains(p)).count()).sum()
}

/// The Prime Directive's side of #882: the opening of a finely
/// tessellated area never leaves that area - not by the thinning's
/// tolerance, not by the arc flattening - and stays within a micron of
/// the opening the unthinned outline gives: the inset on a wall,
/// `sqrt 2` of it where a fillet rounds a square corner. Where two walls
/// converge on a neck the opening closes (R2.5: the 4 mm gap between
/// the island and the disc), the fillet's end slides along them by more
/// - a few microns, inside the area all the same.
#[test]
fn the_opening_of_a_fine_outline_stays_inside_it_and_on_the_plain_opening() {
    let area = keyhole();
    for (rho, bound) in [(0.6, 1e-3), (1.5, 1e-3), (2.5, 5e-3)] {
        let Ok(plain) = area.offset_fine(-rho, ARC_TOL).and_then(|e| e.offset_fine(rho, ARC_TOL)) else { panic!("plain opening") };
        let Ok(got) = open(&area, rho) else { panic!("opening") };
        assert_eq!(got.loops().len(), plain.loops().len(), "rho {rho}");
        assert_eq!(outside(&got, &area), 0, "rho {rho}: boundary samples outside the area opened");
        let (out, back) = (reach(&got, &plain), reach(&plain, &got));
        eprintln!("rho {rho}: {} -> {} vertices, {out:.6} / {back:.6} mm off the plain opening", verts(&plain), verts(&got));
        assert!(out.max(back) <= bound && out.max(back) >= 0.5 * INSET, "rho {rho}: {out} / {back} mm off the plain opening");
        assert!(verts(&got) * 4 < verts(&plain), "rho {rho}: {} of {} vertices", verts(&got), verts(&plain));
    }
}

/// A slot with finely tessellated ends charges near its coarse twin's
/// growth-vertex count (not its measured wall time): the
/// fine ends keep the ~100 chords an end the tolerance needs, the twin
/// has 24), not the 3000 wall vertices a ring that came, and the load,
/// crossing and wall laws are held on the fine outline.
#[test]
#[ignore = "heavy lane: release --ignored (fine and coarse rounded slots grown whole, ~56 s debug)"]
fn a_fine_ended_slot_charges_growth_vertices_near_its_coarse_twin() {
    let r = 5.0;
    let grown = |n: usize| {
        let slot = stadium(30.0, 14.0, n);
        let (Ok(center), seed) = (slot.offset(-r), Point2::new(15.0, 7.0)) else { panic!("center") };
        let Ok(reach) = center.offset(r) else { panic!("reach") };
        let spec = AdaptiveSpec {
            r,
            load: crate::load::DEFAULT_OPTIMAL_LOAD,
            tol: 0.05,
            climb: true,
            seed,
            start: Point2::new(seed.x - 1.2, seed.y),
            bore: r + 1.2,
            helix_r: 1.2,
            smooth: 2.5,
        };
        PEAK.with(|c| c.set(0));
        let t0 = std::time::Instant::now();
        let Ok(pl) = plan(&reach, &center, &spec) else { panic!("plan") };
        let cost = PEAK.with(|c| c.get());
        eprintln!("{n}-chord ends: center {} vertices, planned in {:.1} s, {cost} growth vertices", verts(&center), t0.elapsed().as_secs_f64());
        (pl, center, reach, cost)
    };
    let (coarse, _, _, base) = grown(24);
    let (pl, center, reach, cost) = grown(1500);
    assert!(verts(&center) > 2500, "the fine ends reach the planner: {} vertices", verts(&center));
    assert!(base > 0 && coarse.refused.is_none() && pl.refused.is_none(), "both grow: {:?} {:?}", coarse.notes, pl.notes);
    assert!(cost as f64 <= 2.0 * base as f64, "fine {cost} vs coarse {base} growth vertices");
    // The laws, on the fine outline.
    assert!(first_crossing(&pl.passes).is_none(), "{:?}", pl.notes);
    assert!(!pl.notes.iter().any(|n| n.contains("exceeds") || n.contains("overload")), "{:?}", pl.notes);
    assert!(pl.rest < crate::machining2d::pocket_hsm::HSM_REST_MAX * reach.area(), "rest {} {:?}", pl.rest, pl.notes);
    // Every point stays in the center area as it came (never the walls).
    let off = pl.passes.iter().flatten().filter(|&&q| !center.contains(q)).map(|&q| boundary_dist(&center, q)).fold(0.0, f64::max);
    assert!(off <= 1e-6, "the walk leaves the center area by {off} mm");
}
