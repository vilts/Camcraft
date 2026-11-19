//! Pins of the air-side stock ring (issue #1950) on the user's D plate:
//! a disc R30 cut flat at y 19.719 with a bore R12.5, in a 70 x 70 stock,
//! the footprint the stock grown by 2r + 0.01 (the air band). Ours.

use super::*;
use crate::geo2d::Loop;
use crate::load::DEFAULT_OPTIMAL_LOAD;
use crate::machining2d::adaptive::boundary_dist;
use crate::machining2d::pocket_hsm::{HSM_REST_MAX, MOVE_BUDGET, PLAN_TOL, components};

const R: f64 = 5.0;

fn circle(r: f64, n: usize) -> Vec<Point2> {
    (0..n).map(|k| {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        Point2::new(r * a.cos(), r * a.sin())
    }).collect()
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

/// The ring's centre area and the bore's, and the part material.
fn level() -> (Region, Region) {
    let Ok(d) = region(circle(30.0, 2000)).intersect(&region(vec![Point2::new(-40.0, -40.0), Point2::new(40.0, -40.0), Point2::new(40.0, 19.719_008_6), Point2::new(-40.0, 19.719_008_6)])) else { panic!("d") };
    let Ok(mat) = d.difference(&region(circle(12.5, 2000))) else { panic!("mat") };
    let Ok(foot) = stock().offset(2.0 * R + 0.01) else { panic!("foot") };
    let Ok(target) = foot.difference(&mat) else { panic!("target") };
    let Ok(c) = target.offset(-R) else { panic!("center") };
    let Ok(comps) = components(&c) else { panic!("comps") };
    let (mut ring, mut bore) = (Region::empty(), Region::empty());
    for p in comps {
        if p.area() > 1000.0 { ring = p } else { bore = p }
    }
    (ring, bore)
}

fn spec(at: Point2) -> AdaptiveSpec {
    AdaptiveSpec { r: R, load: DEFAULT_OPTIMAL_LOAD, tol: PLAN_TOL, climb: true, seed: at, start: at, bore: 0.0, helix_r: 0.0, smooth: 0.5 * R }
}

fn material(center: &Region) -> Region {
    let Ok(m) = center.offset(R).and_then(|m| m.intersect(&stock())) else { panic!("material") };
    m
}

fn ring_plan() -> (Region, Region, AirPlan) {
    let (ring, _) = level();
    let mat = material(&ring);
    let t = std::time::Instant::now();
    let Ok(Some(p)) = plan_from_air(&mat, &stock(), &ring, &spec(Point2::new(0.0, 0.0))) else { panic!("no air plan") };
    eprintln!(
        "air ring: {:.1} s, fronts {:?}, passes {}, moves {}, rest {:.4}, worst {:.4} at {:?}, run ends {:.4} at {:?}",
        t.elapsed().as_secs_f64(),
        p.fronts,
        p.plan.passes.len(),
        p.plan.travel.moves,
        p.plan.rest / mat.area(),
        p.worst.0,
        p.worst.1,
        p.ends.0,
        p.ends.1
    );
    (ring, mat, p)
}

#[test]
fn a_ring_round_an_island_is_cut_from_the_air_within_budget() {
    let (_, mat, p) = ring_plan();
    assert!(p.plan.refused.is_none());
    assert!(p.plan.rest / mat.area() <= HSM_REST_MAX, "rest {:.4}", p.plan.rest / mat.area());
    assert!(p.plan.travel.moves < MOVE_BUDGET, "moves {}", p.plan.travel.moves);
    let limit = DEFAULT_OPTIMAL_LOAD * (1.0 + PLAN_TOL);
    assert!(p.ends.0 <= limit, "run-end load {:.4} at {:?} over {limit:.4}", p.ends.0, p.ends.1);
    assert!(p.worst.0 <= limit, "worst load {:.4} at {:?} over {limit:.4}", p.worst.0, p.worst.1);
}

#[test]
fn every_air_ring_pass_stays_in_the_centre_area() {
    let (ring, _, p) = ring_plan();
    for pass in &p.plan.passes {
        for (i, &q) in pass.iter().enumerate() {
            assert!(ring.contains(q) || boundary_dist(&ring, q) < 1e-6, "{q:?} outside the centre area");
            // The midpoint of each move too.
            if let Some(&n) = pass.get(i + 1) {
                let m = Point2::new(0.5 * (q.x + n.x), 0.5 * (q.y + n.y));
                assert!(ring.contains(m) || boundary_dist(&ring, m) < 1e-6, "move {q:?} -> {n:?} leaves the centre area");
            }
        }
    }
}

#[test]
fn the_air_entry_disc_is_clear_of_the_stock() {
    let (_, _, p) = ring_plan();
    let Some(&q) = p.plan.passes.first().and_then(|w| w.first()) else { panic!("no pass") };
    let s = stock();
    assert!(!s.contains(q) && boundary_dist(&s, q) >= R, "entry {q:?} is {:.4} from the stock", boundary_dist(&s, q));
}

/// The hop reading of rounds F and G: each pass, where it starts and ends, and
/// how it is reached (a retract hop, or a return low or over the retract
/// plane). Each open run of a front reads as a hop: its start stands
/// beside the last front, where the rim (`r + 1.5` cells) reads its wall.
#[test]
#[ignore = "reading for issue #1950: prints how each air ring pass is reached (~6 s debug)"]
fn probe_air_hops() {
    let (_, _, p) = ring_plan();
    eprintln!("hops {} fronts {:?}", p.plan.travel.hops, p.fronts);
    for (k, (pass, j)) in p.plan.passes.iter().zip(&p.plan.joins).enumerate() {
        let (Some(&a), Some(&b)) = (pass.first(), pass.last()) else { continue };
        let prev = k.checked_sub(1).and_then(|i| p.plan.passes.get(i)).and_then(|q| q.last().copied());
        eprintln!(
            "{k:3} n {:4} len {:7.2} start ({:7.2},{:7.2}) end ({:7.2},{:7.2}) gap {:6.2} join {}",
            pass.len(),
            super::super::front::len(pass),
            a.x, a.y, b.x, b.y,
            prev.map_or(0.0, |q| super::super::front::dist(q, a)),
            match j { None => "HOP".to_string(), Some(r) => format!("{} lift {:.2} down {}", if r.high { "high" } else { "low" }, r.lift, r.down.len()) }
        );
    }
}

#[test]
fn a_piece_with_no_air_side_keeps_the_growth_spiral() {
    let (_, bore) = level();
    let mat = material(&bore);
    let Ok(p) = plan_from_air(&mat, &stock(), &bore, &spec(Point2::new(0.0, 0.0))) else { panic!("plan") };
    assert!(p.is_none());
}
