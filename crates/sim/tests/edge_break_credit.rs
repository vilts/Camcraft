//! #303: the operation owns its break. A chamfer cone ridden along a
//! finished part's top edge is judged against the bevel the op declared
//! (`cl::EdgeBreak`), not the model's sharp edge: down to the bevel is
//! clean, past it is still a gouge charged at its depth under the
//! bevel, and an edge nothing declared keeps the model's verdict.
//!
//! Gate map (#303): covers camcraft_sim::stock_sim (edge_break,
//! deviate) and camcraft_sim::cl (EdgeBreak).

use camcraft_sim::cl::{EdgeBreak, Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::stock_sim::{self, credit_breaks, deviate, Dexel, SimOp, DEV_TOL};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
}

/// The finished part: a 20 x 20 x 10 block, its top edge the loop.
fn part() -> Vec<Triangle> {
    cube(p(0.0, 0.0, 0.0), p(20.0, 20.0, 10.0)).facets.to_vec()
}

const LOOP: [(f64, f64); 5] = [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0), (0.0, 0.0)];

/// A 90-degree cone (slope 1) riding the top loop with its tip at `z`.
fn ride(tip_z: f64) -> Vec<Move> {
    LOOP.windows(2).map(|w| feed(p(w[0].0, w[0].1, tip_z), p(w[1].0, w[1].1, tip_z))).collect()
}

/// The break a 1 mm, 45-degree chamfer of that loop declares.
fn declared(edge: &[(f64, f64)]) -> EdgeBreak {
    EdgeBreak { edge: edge.iter().map(|&(x, y)| p(x, y, 10.0)).collect(), depth: 1.0, slope: 1.0, volume: None }
}

fn cut(tip_z: f64) -> Dexel {
    let facets = part();
    let mut d = Dexel::from_facets(&facets, 0.25).expect("the finished part as stock");
    let moves = ride(tip_z);
    let op = SimOp { moves: &moves, radius: 3.0, ball: false, cone: Some(1.0), corner: None };
    stock_sim::simulate(&mut d, &[op]).expect("the ride cuts");
    d
}

#[test]
fn the_declared_chamfer_is_the_finished_part_not_a_gouge() {
    let d = cut(9.0);
    let facets = part();
    let mut dev = deviate(&d, &facets, DEV_TOL).expect("verdict");
    assert!(dev.gouge_cells > 0, "the sharp model convicts the bevel - the bug's own reading");
    let credited = credit_breaks(&mut dev, &d, &facets, &[declared(&LOOP)]);
    assert!(credited > 0);
    assert_eq!(dev.gouge_cells, 0, "worst {} at {:?}", dev.max_gouge, dev.max_gouge_at);
    assert!(dev.is_clean());
}

#[test]
fn a_ride_deeper_than_its_declared_break_is_still_a_gouge_at_its_excess() {
    // The same declaration, the ride 0.3 mm lower: every bevel column
    // stands 0.3 under the bevel, and that is what the verdict charges.
    let d = cut(8.7);
    let facets = part();
    let mut dev = deviate(&d, &facets, DEV_TOL).expect("verdict");
    let before = dev.max_gouge;
    credit_breaks(&mut dev, &d, &facets, &[declared(&LOOP)]);
    assert!(dev.gouge_cells > 0, "an over-deep chamfer passed");
    assert!((dev.max_gouge - 0.3).abs() < 1e-6, "charged {} (was {before})", dev.max_gouge);
    assert!(dev.max_gouge_at.is_some(), "the worst gouge still locates itself");
}

#[test]
fn an_edge_no_operation_declared_keeps_the_models_verdict() {
    // Only the y = 0 edge is declared: the other three sides' bevels
    // are cuts nobody asked for, convicted at their full depth.
    let d = cut(9.0);
    let facets = part();
    let mut dev = deviate(&d, &facets, DEV_TOL).expect("verdict");
    let before = dev.gouge_cells;
    credit_breaks(&mut dev, &d, &facets, &[declared(&LOOP[..2])]);
    assert!(dev.gouge_cells > 0 && dev.gouge_cells < before, "{} of {before}", dev.gouge_cells);
    assert!(dev.max_gouge > 0.85, "the undeclared bevel is charged whole (centres 1/8 mm in): {}", dev.max_gouge);
    let at = dev.max_gouge_at.expect("located");
    assert!(at.y > 0.9, "the worst gouge is on an undeclared side: {at:?}");
}

#[test]
fn a_model_top_above_the_edge_is_never_the_bevels() {
    // A boss standing 5 mm over the declared edge's level, inside the
    // bevel's width: cutting it is a gouge whatever the break says.
    let mut facets = part();
    facets.extend(cube(p(0.3, 5.0, 10.0), p(0.8, 6.0, 15.0)).facets.iter().copied());
    let mut d = Dexel::from_facets(&facets, 0.25).expect("stock");
    let moves = ride(9.0);
    let op = SimOp { moves: &moves, radius: 3.0, ball: false, cone: Some(1.0), corner: None };
    stock_sim::simulate(&mut d, &[op]).expect("the ride cuts");
    let mut dev = deviate(&d, &facets, DEV_TOL).expect("verdict");
    credit_breaks(&mut dev, &d, &facets, &[declared(&LOOP)]);
    assert!(dev.gouge_cells > 0, "the boss was credited to the bevel");
    assert!(dev.max_gouge > 5.0, "the boss is charged from its own top: {}", dev.max_gouge);
}

#[test]
fn a_sloped_edge_bevel_is_the_cone_swept_along_it() {
    // An edge climbing 1 in 2 under a 90-degree cone: directly over the
    // edge the bevel stands exactly `depth` under it; beside it, the
    // swept cone's lowest point - checked against a brute-force sweep.
    let b = EdgeBreak { edge: vec![p(0.0, 0.0, 0.0), p(10.0, 0.0, 5.0)], depth: 1.0, slope: 1.0, volume: None };
    let (f, z) = b.floor_at(5.0, 0.0).expect("on the edge");
    assert!((f - 1.5).abs() < 1e-12 && (z - 2.5).abs() < 1e-12, "{f} {z}");
    for (x, y) in [(5.0, 0.6), (2.0, 0.9), (9.5, 0.3), (0.0, 0.5)] {
        let (f, _) = b.floor_at(x, y).expect("inside the width");
        let brute = (0..=100_000)
            .map(|i| {
                let u = 10.0 * f64::from(i) / 100_000.0;
                0.5 * u - 1.0 + ((x - u).powi(2) + y * y).sqrt()
            })
            .fold(f64::INFINITY, f64::min);
        assert!((f - brute).abs() < 1e-6, "({x},{y}): {f} vs {brute}");
    }
    assert!(b.floor_at(5.0, 1.5).is_none(), "past the width nothing is declared");
    let bad = EdgeBreak { depth: f64::NAN, ..b };
    assert!(!bad.is_sane() && bad.floor_at(5.0, 0.0).is_none(), "an insane record declares nothing");
}
