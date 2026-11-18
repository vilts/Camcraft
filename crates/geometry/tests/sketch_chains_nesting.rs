//! Provenance: hand-built nests with depth and area known in closed form (issue 1861).
//! Gate map (M4.4): covers camcraft_geometry::sketch_chains.

mod common;

use camcraft_geometry::dxf::Primitive;
use camcraft_geometry::geo2d::{Loop, Region};
use camcraft_geometry::sketch_chains::{ChainOptions, ClosedChain, LoopRole, chain};
use common::*;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

#[test]
fn nested_rectangles_and_circles_classify_as_outer_and_hole() {
    let mut input = rect("cut", 0.0, 0.0, 100.0, 100.0);
    input.push(circle("cut", 50.0, 50.0, 30.0));
    input.extend(rect("cut", 45.0, 45.0, 55.0, 55.0));
    input.push(circle("holes", 10.0, 10.0, 3.0));
    input.push(circle("cut", 200.0, 50.0, 5.0));
    let chains = chain(&input, ChainOptions::default()).unwrap();
    assert!(chains.open.is_empty() && chains.rejected.is_empty());
    let by_area = |area: f64| {
        chains
            .closed
            .iter()
            .find(|c| (c.area - area).abs() < 1e-6)
            .unwrap()
    };
    let (plate, bore, island) = (by_area(10_000.0), by_area(PI * 900.0), by_area(100.0));
    let (drill, apart) = (by_area(PI * 9.0), by_area(PI * 25.0));
    assert_eq!(role(plate), (LoopRole::Outer, 0));
    assert_eq!(role(bore), (LoopRole::Hole, 1));
    assert_eq!(role(island), (LoopRole::Outer, 2));
    assert_eq!(role(drill), (LoopRole::Hole, 1));
    assert_eq!(role(apart), (LoopRole::Outer, 0));
    assert_eq!(plate.nesting.clone().unwrap().parent, None);
    assert_eq!(bore.nesting.clone().unwrap().parent, Some(plate.id));
    assert_eq!(island.nesting.clone().unwrap().parent, Some(bore.id));
    // A hole on another layer still nests in the plate.
    assert_eq!(drill.nesting.clone().unwrap().parent, Some(plate.id));

    // The same reading as geo2d::Region's even-odd rule over the flattened loops.
    let loops: Vec<Loop> = chains
        .closed
        .iter()
        .map(|c| Loop::new(flatten(c)).unwrap())
        .collect();
    let region = Region::new(loops).unwrap();
    let signed: f64 = chains
        .closed
        .iter()
        .map(|c| {
            if role(c).0 == LoopRole::Outer {
                c.area
            } else {
                -c.area
            }
        })
        .sum();
    assert!(
        (region.area() - signed).abs() < 1.0,
        "{} vs {signed}",
        region.area()
    );
    assert!(region.contains(p(5.0, 50.0)) && !region.contains(p(30.0, 50.0)));
    assert!(region.contains(p(50.0, 50.0)) && !region.contains(p(10.0, 10.0)));
}

#[test]
fn containment_follows_the_arcs_not_their_chords() {
    // A half disc: the chord x = 0 and a semicircle bulging to +x. Its chord polygon is empty.
    let mut input = vec![
        line("cut", 0.0, 10.0, 0.0, -10.0),
        arc("cut", 0.0, 0.0, 10.0, -FRAC_PI_2, PI),
    ];
    input.push(circle("cut", 7.0, 0.0, 1.0)); // inside the bulge
    input.push(circle("cut", -3.0, 0.0, 1.0)); // left of the chord
    input.push(circle("cut", 8.0, 8.0, 1.0)); // inside the bounding box, outside the arc
    let chains = chain(&input, ChainOptions::default()).unwrap();
    assert!(chains.rejected.is_empty());
    let half = chains
        .closed
        .iter()
        .find(|c| c.segments.len() == 2)
        .unwrap();
    near(half.area, PI * 50.0);
    let at = |x: f64| {
        let found = chains.closed.iter().find(
            |c| matches!(c.segments[0].curve, Primitive::Circle { center, .. } if center.x == x),
        );
        role(found.unwrap())
    };
    assert_eq!(at(7.0), (LoopRole::Hole, 1));
    assert_eq!(at(-3.0), (LoopRole::Outer, 0));
    assert_eq!(at(8.0), (LoopRole::Outer, 0));
    // Concentric circles alternate, and a ring cut by a concave arc reads the same.
    let rings: Vec<_> = (1..=5)
        .map(|i| circle("r", 0.0, 0.0, f64::from(i)))
        .collect();
    let rings = chain(&rings, ChainOptions::default()).unwrap();
    let mut depths: Vec<_> = rings.closed.iter().map(|c| (c.area, role(c))).collect();
    depths.sort_by(|a, b| b.0.total_cmp(&a.0));
    let roles: Vec<_> = depths.iter().map(|d| d.1).collect();
    let (outer, hole) = (LoopRole::Outer, LoopRole::Hole);
    assert_eq!(
        roles,
        [(outer, 0), (hole, 1), (outer, 2), (hole, 3), (outer, 4)]
    );
}

#[test]
fn long_loop_chains_in_bounded_time() {
    // 40,000 sides of a circle-like polygon, given in a scrambled order.
    let n = 40_000usize;
    let at = |i: usize| {
        let a = TAU * (i % n) as f64 / n as f64;
        (1000.0 * a.cos(), 1000.0 * a.sin())
    };
    let mut input = Vec::with_capacity(n);
    for k in 0..n {
        let i = (k * 7919) % n;
        let ((x0, y0), (x1, y1)) = (at(i), at(i + 1));
        input.push(line("big", x0, y0, x1, y1));
    }
    input.push(circle("big", 0.0, 0.0, 10.0));
    let chains = chain(&input, ChainOptions::default()).unwrap();
    assert!(chains.rejected.is_empty() && chains.open.is_empty());
    let polygon = chains
        .closed
        .iter()
        .find(|c| c.segments.len() == n)
        .unwrap();
    assert!((polygon.area - PI * 1e6).abs() < 1.0);
    let hole = chains
        .closed
        .iter()
        .find(|c| c.segments.len() == 1)
        .unwrap();
    assert_eq!(role(hole), (LoopRole::Hole, 1));

    // A tall strip whose long sides are 25,000 short collinear lines each: every one of them
    // shares its x range, so the crossing search has to sweep along y to stay inside its budget.
    let n = 25_000;
    let mut strip = vec![
        line("tall", 0.0, 0.0, 5.0, 0.0),
        line("tall", 5.0, f64::from(n), 0.0, f64::from(n)),
    ];
    for i in 0..n {
        let (y0, y1) = (f64::from(i), f64::from(i + 1));
        strip.push(line("tall", 5.0, y0, 5.0, y1));
        strip.push(line("tall", 0.0, y1, 0.0, y0));
    }
    let chains = chain(&strip, ChainOptions::default()).unwrap();
    assert!(chains.rejected.is_empty() && chains.open.is_empty());
    near(chains.closed[0].area, 5.0 * f64::from(n));
}

#[test]
fn shuffled_nests_of_mixed_shapes_keep_their_depth_and_parent() {
    let mut random = Lehmer(42);
    for round in 0..200 {
        // Every shape lies inside the disc of radius `reach` and contains the disc of 0.425 * reach;
        // the next shape gets 0.38 * reach around a centre moved by at most 0.015 * reach.
        let mut input = Vec::new();
        let (mut cx, mut cy, mut reach) =
            (random.unit() * 500.0 - 250.0, random.unit() * 500.0, 100.0);
        let levels = 2 + random.next(5) as usize;
        let mut areas = Vec::new();
        for _ in 0..levels {
            let half = 0.6 * reach;
            let (shape, area) = match random.next(4) {
                0 => (
                    vec![circle("n", 0.0, 0.0, 0.8 * reach)],
                    PI * 0.64 * reach * reach,
                ),
                1 => (rect("n", -half, -half, half, half), 4.0 * half * half),
                2 => {
                    let corner = 0.25 * half;
                    let shape = rounded_rect("n", 2.0 * half, 2.0 * half, corner);
                    let moved = shape
                        .iter()
                        .map(|item| shift(item, -half - 0.333_333, -half - 0.333_333));
                    (
                        moved.collect(),
                        4.0 * half * half - (4.0 - PI) * corner * corner,
                    )
                }
                _ => {
                    // A half disc of radius d on its chord, moved so its inscribed circle (d / 2)
                    // is centred; its corners reach d * sqrt(1.25) = 0.95 * reach.
                    let d = 0.85 * reach;
                    let shape = [
                        line("n", 0.0, d, 0.0, -d),
                        arc("n", 0.0, 0.0, d, -FRAC_PI_2, PI),
                    ];
                    let moved = shape.iter().map(|item| shift(item, -0.5 * d, 0.0));
                    (moved.collect(), 0.5 * PI * d * d)
                }
            };
            let angle = (random.unit() - 0.5) * 40.0; // several turns either way
            input.extend(shape.iter().map(|item| shift(&turn(item, angle), cx, cy)));
            areas.push(area);
            cx += (random.unit() - 0.5) * 0.04 * reach;
            cy += (random.unit() - 0.5) * 0.04 * reach;
            reach *= 0.38;
        }
        for i in (1..input.len()).rev() {
            input.swap(i, random.next(i as u64 + 1) as usize);
        }
        let input: Vec<_> = input
            .iter()
            .map(|item| {
                if random.next(2) == 0 {
                    flip(item)
                } else {
                    item.clone()
                }
            })
            .collect();
        let chains = chain(&input, ChainOptions::default()).unwrap();
        assert!(
            chains.rejected.is_empty() && chains.open.is_empty(),
            "round {round}: {:?}",
            chains.rejected
        );
        assert_eq!(chains.closed.len(), levels, "round {round}");
        let mut nest: Vec<&ClosedChain> = chains.closed.iter().collect();
        nest.sort_by(|a, b| b.area.total_cmp(&a.area));
        for (depth, closed) in nest.iter().enumerate() {
            assert!(
                (closed.area - areas[depth]).abs() < 1e-6 * areas[depth].max(1.0),
                "round {round}"
            );
            let nesting = closed.nesting.clone().unwrap();
            let expected = if depth % 2 == 0 {
                LoopRole::Outer
            } else {
                LoopRole::Hole
            };
            assert_eq!(
                (nesting.role, nesting.depth),
                (expected, depth),
                "round {round}"
            );
            assert_eq!(
                nesting.parent,
                depth.checked_sub(1).map(|outer| nest[outer].id),
                "round {round}"
            );
        }
    }
}
