//! Provenance: hand-built degenerate sketches; every one must be refused by name (issue 1861).
//! Gate map (M4.4): covers camcraft_geometry::sketch_chains.

mod common;

use camcraft_geometry::dxf::LayeredPrimitive;
use camcraft_geometry::geo2d::{Loop, Region};
use camcraft_geometry::sketch_chains::{
    ChainError, ChainId, ChainOptions, LoopRole, SketchChains, chain,
};
use common::*;
use std::f64::consts::PI;

fn chained(input: &[LayeredPrimitive]) -> SketchChains {
    chain(input, ChainOptions::default()).unwrap()
}

fn only_error(chains: &SketchChains) -> &ChainError {
    let [rejected] = chains.rejected.as_slice() else {
        panic!("one rejection: {:?}", chains.rejected)
    };
    &rejected.error
}

#[test]
fn self_crossing_loop_is_refused_by_name() {
    // A bow tie: the diagonals cross at (5, 5).
    let input = vec![
        line("cut", 0.0, 0.0, 10.0, 10.0),
        line("cut", 10.0, 10.0, 10.0, 0.0),
        line("cut", 10.0, 0.0, 0.0, 10.0),
        line("cut", 0.0, 10.0, 0.0, 0.0),
    ];
    let chains = chained(&input);
    assert!(chains.closed.is_empty() && chains.open.is_empty());
    assert_eq!(chains.rejected[0].primitives, [0, 1, 2, 3]);
    let ChainError::SelfCrossing { at } = *only_error(&chains) else {
        panic!("{:?}", chains.rejected)
    };
    near(at.x, 5.0);
    near(at.y, 5.0);
    let text = chains.clone().strict().unwrap_err().to_string();
    assert_eq!(
        text,
        "sketch chaining: closed loop crosses or touches itself at (5, 5)"
    );

    // A side that runs back over the one before it overlaps; an arc cutting a far side crosses.
    let hairpin = vec![
        line("cut", 0.0, 0.0, 10.0, 0.0),
        line("cut", 10.0, 0.0, 4.0, 0.0),
        line("cut", 4.0, 0.0, 4.0, 5.0),
        line("cut", 4.0, 5.0, 0.0, 5.0),
        line("cut", 0.0, 5.0, 0.0, 0.0),
    ];
    assert!(matches!(
        only_error(&chained(&hairpin)),
        ChainError::SelfCrossing { .. }
    ));
    let mut bulge = rect("cut", 0.0, 0.0, 10.0, 4.0);
    bulge[0] = arc("cut", 5.0, 0.0, 5.0, PI, -PI); // the bottom side bulges up through the top
    assert!(matches!(
        only_error(&chained(&bulge)),
        ChainError::SelfCrossing { .. }
    ));
}

#[test]
fn branching_vertex_is_a_named_diagnostic() {
    // A rectangle with a spur from one corner: three ends meet at (10, 0).
    let mut input = rect("cut", 0.0, 0.0, 10.0, 5.0);
    input.push(line("cut", 10.0, 0.0, 15.0, -5.0));
    input.extend(rect("cut", 20.0, 0.0, 30.0, 5.0));
    let chains = chained(&input);
    let ChainError::Branch { at, degree } = *only_error(&chains) else {
        panic!("{:?}", chains.rejected)
    };
    assert_eq!((at, degree), (p(10.0, 0.0), 3));
    assert_eq!(chains.rejected[0].primitives, [0, 1, 2, 3, 4]);
    // The clean rectangle beside it is still chained.
    assert_eq!(chains.closed.len(), 1);
    near(chains.closed[0].area, 50.0);
    assert!(chains.strict().is_err());
}

#[test]
fn degenerate_pieces_are_refused_by_name() {
    // A side shorter than the tolerance cannot be told from a joint.
    let mut input = rect("cut", 0.0, 0.0, 10.0, 5.0);
    input.push(line("cut", 50.0, 0.0, 50.005, 0.0));
    input.push(line("cut", 60.0, 0.0, 60.0, 0.0));
    input.push(circle("cut", 80.0, 0.0, 0.001));
    input.push(arc("cut", 90.0, 0.0, 0.001, 0.0, 1.0));
    let chains = chained(&input);
    let errors: Vec<_> = chains
        .rejected
        .iter()
        .map(|r| (r.primitives.clone(), r.error.clone()))
        .collect();
    assert_eq!(
        errors,
        [
            (vec![4], ChainError::DegenerateSegment { primitive: 4 }),
            (vec![5], ChainError::DegenerateSegment { primitive: 5 }),
            (vec![6], ChainError::ZeroArea),
            (vec![7], ChainError::DegenerateSegment { primitive: 7 }),
        ]
    );
    assert_eq!(chains.closed.len(), 1);

    // A repeated primitive is dropped by name, in either direction, and the loop still closes.
    let mut doubled = rect("cut", 0.0, 0.0, 10.0, 5.0);
    doubled.push(flip(&doubled[1]));
    doubled.push(circle("cut", 5.0, 2.5, 1.0));
    doubled.push(circle("cut", 5.0, 2.5, 1.0));
    doubled.push(circle("other", 5.0, 2.5, 1.0));
    let chains = chained(&doubled);
    let errors: Vec<_> = chains.rejected.iter().map(|r| r.error.clone()).collect();
    assert_eq!(
        errors,
        [
            ChainError::DuplicatePrimitive {
                primitive: 4,
                of: 1
            },
            ChainError::DuplicatePrimitive {
                primitive: 6,
                of: 5
            },
        ]
    );
    assert_eq!(chains.closed.len(), 3);
}

#[test]
fn invalid_input_is_an_error_not_a_panic() {
    let square = rect("cut", 0.0, 0.0, 10.0, 5.0);
    for join_tolerance in [0.0, -0.01, 1e-7, 10.5, f64::NAN, f64::INFINITY] {
        assert_eq!(
            chain(&square, ChainOptions { join_tolerance }),
            Err(ChainError::InvalidTolerance)
        );
    }
    let bad = |item: LayeredPrimitive| {
        let mut input = square.clone();
        input.push(item);
        chain(&input, ChainOptions::default()).unwrap_err()
    };
    assert_eq!(
        bad(line("cut", f64::NAN, 0.0, 1.0, 1.0)),
        ChainError::NonFinite { primitive: 4 }
    );
    assert_eq!(
        bad(circle("cut", 0.0, 0.0, f64::INFINITY)),
        ChainError::NonFinite { primitive: 4 }
    );
    assert_eq!(
        bad(line("cut", 0.0, 0.0, 1e7, 1.0)),
        ChainError::OutOfRange { primitive: 4 }
    );
    assert_eq!(
        bad(circle("cut", 9e6, 0.0, 2e6)),
        ChainError::OutOfRange { primitive: 4 }
    );
    assert!(matches!(
        bad(circle("cut", 0.0, 0.0, 0.0)),
        ChainError::InvalidPrimitive { primitive: 4, .. }
    ));
    assert!(matches!(
        bad(arc("cut", 0.0, 0.0, -1.0, 0.0, 1.0)),
        ChainError::InvalidPrimitive { .. }
    ));
    assert!(matches!(
        bad(arc("cut", 0.0, 0.0, 1.0, 0.0, 0.0)),
        ChainError::InvalidPrimitive { .. }
    ));
    assert!(matches!(
        bad(arc("cut", 0.0, 0.0, 1.0, 0.0, 7.0)),
        ChainError::InvalidPrimitive { .. }
    ));
    let text = bad(arc("cut", 0.0, 0.0, 1.0, 0.0, 7.0)).to_string();
    assert_eq!(
        text,
        "sketch chaining: primitive 4 is invalid: arc sweep must be non-zero and at most a turn"
    );
    assert_eq!(
        chain(&[], ChainOptions::default()),
        Ok(SketchChains::default())
    );
}

#[test]
fn crossing_loops_get_no_outer_or_hole_reading() {
    let mut input = rect("a", 0.0, 0.0, 10.0, 10.0);
    input.extend(rect("b", 5.0, 5.0, 15.0, 15.0)); // overlaps the first
    input.push(circle("a", 2.0, 2.0, 1.0)); // inside the first only
    input.push(circle("a", 40.0, 0.0, 1.0)); // clear of both
    input.push(circle("t", 60.0, 0.0, 2.0));
    input.push(circle("t", 64.0, 0.0, 2.0)); // tangent to the one before
    let chains = chained(&input);
    assert!(chains.rejected.is_empty());
    let id = |area: f64, layer: &str| -> ChainId {
        chains
            .closed
            .iter()
            .find(|c| c.layer == layer && (c.area - area).abs() < 1e-6)
            .unwrap()
            .id
    };
    let (a, b) = (id(100.0, "a"), id(100.0, "b"));
    for chain in &chains.closed {
        let small = chain.segments.len() == 1 && chain.layer == "a";
        match (&chain.nesting, small) {
            (Ok(nesting), true) if chain.segments[0].start.x > 30.0 => {
                assert_eq!((nesting.role, nesting.depth), (LoopRole::Outer, 0));
            }
            // The circle inside a crossed rectangle inherits its ambiguity.
            (Err(ChainError::NestingAmbiguous { other, .. }), true) => assert_eq!(*other, b),
            (Err(ChainError::NestingAmbiguous { other, at }), false) if chain.layer != "t" => {
                assert_eq!(*other, if chain.id == a { b } else { a });
                assert!([p(10.0, 5.0), p(5.0, 10.0)].contains(at), "{at:?}");
            }
            (Err(ChainError::NestingAmbiguous { at, .. }), false) => {
                near(at.x, 62.0);
                near(at.y, 0.0);
            }
            other => panic!("unexpected nesting {other:?} on layer {}", chain.layer),
        }
    }
    assert!(matches!(
        chains.strict(),
        Err(ChainError::NestingAmbiguous { .. })
    ));
}

#[test]
fn dense_drawing_is_refused_within_the_work_limit() {
    // 25,000 spokes from one point: every end there is a neighbour of every other.
    let spokes: Vec<_> = (0..25_000)
        .map(|i| {
            let a = f64::from(i) * 1e-4;
            line("star", 0.0, 0.0, 50.0 * a.cos(), 50.0 * a.sin())
        })
        .collect();
    assert_eq!(
        chain(&spokes, ChainOptions::default()),
        Err(ChainError::TooComplex)
    );
    // A star within the limit is one branch, named once.
    let chains = chained(&spokes[..500]);
    let ChainError::Branch { degree, .. } = *only_error(&chains) else {
        panic!("branch")
    };
    assert_eq!(degree, 500);
}

#[test]
fn random_grid_sketches_never_panic_and_do_not_depend_on_input_order() {
    let mut random = Lehmer(1861);
    let (mut nested, mut loops, mut refused) = (0, 0, 0);
    for round in 0..400 {
        // Lines, arcs and circles on a coarse grid, so ends coincide, cross and overlap often.
        let mut input = Vec::new();
        for _ in 0..(3 + random.next(14)) {
            let mut grid = || random.next(5) as f64 * 2.0;
            let (x0, y0, x1, y1) = (grid(), grid(), grid(), grid());
            let layer = if random.next(5) == 0 { "b" } else { "a" };
            input.push(match random.next(6) {
                0 => circle(layer, x0, y0, 1.0 + random.next(3) as f64),
                1 => arc(
                    layer,
                    x0,
                    y0,
                    2.0,
                    random.next(4) as f64 * PI / 2.0,
                    PI / 2.0 * (1.0 + random.next(3) as f64),
                ),
                2 => arc(layer, x0, y0, 2.0, random.next(4) as f64 * PI / 2.0, -PI),
                _ => line(layer, x0, y0, x1, y1),
            });
        }
        let first = chained(&input);
        let used: usize = first.closed.iter().map(|c| c.segments.len()).sum::<usize>()
            + first.open.iter().map(|c| c.segments.len()).sum::<usize>()
            + first
                .rejected
                .iter()
                .map(|r| r.primitives.len())
                .sum::<usize>();
        assert_eq!(
            used,
            input.len(),
            "round {round}: every primitive is chained or rejected once"
        );
        for closed in &first.closed {
            assert!(closed.area > 0.0 && closed.max_gap <= 0.01);
        }
        // Oracle for the nesting: count the fine chord polygons that contain the loop's start.
        let polygons: Vec<Region> = first
            .closed
            .iter()
            .map(|c| Region::new(vec![Loop::new(flatten(c)).unwrap()]).unwrap())
            .collect();
        for (i, closed) in first.closed.iter().enumerate() {
            let Ok(nesting) = &closed.nesting else {
                continue;
            };
            let start = closed.segments[0].start;
            let around = polygons
                .iter()
                .enumerate()
                .filter(|(j, polygon)| *j != i && polygon.contains(start));
            assert_eq!(
                nesting.depth,
                around.count(),
                "round {round}: depth of loop {i}"
            );
            assert_eq!(nesting.role == LoopRole::Hole, nesting.depth % 2 == 1);
            nested += usize::from(nesting.depth > 0);
            // Loops with a reading are apart or nested, never partly overlapping.
            for (j, other) in first.closed.iter().enumerate().skip(i + 1) {
                if other.nesting.is_ok() {
                    let shared = polygons[i].intersect(&polygons[j]).unwrap().area();
                    let small = polygons[i].area().min(polygons[j].area());
                    assert!(
                        shared < 0.02 * small || shared > 0.98 * small,
                        "round {round}: loops {i} and {j} share {shared} of {small}"
                    );
                }
            }
        }
        let mut shuffled = input.clone();
        for i in (1..shuffled.len()).rev() {
            shuffled.swap(i, random.next(i as u64 + 1) as usize);
        }
        let flipped: Vec<_> = shuffled.iter().map(flip).collect();
        let second = chained(&flipped);
        let ids = |chains: &SketchChains| {
            let closed: Vec<_> = chains
                .closed
                .iter()
                .map(|c| (c.id, c.nesting.clone()))
                .collect();
            let open: Vec<_> = chains.open.iter().map(|c| (c.id, c.start, c.end)).collect();
            (closed, open, chains.rejected.len())
        };
        assert_eq!(ids(&first), ids(&second), "round {round}");
        loops += first.closed.len();
        refused += first.rejected.len();
    }
    // The generator must keep reaching the interesting cases.
    assert!(
        loops > 500 && nested > 5 && refused > 100,
        "{loops} loops, {nested} nested, {refused} refused"
    );
}
