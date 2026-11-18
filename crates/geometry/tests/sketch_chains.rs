//! Provenance: hand-built sketches with areas and nesting known in closed form (issue 1861).
//! Gate map (M4.4): covers camcraft_geometry::sketch_chains.

mod common;

use camcraft_geometry::dxf::{Primitive, Unit, Units};
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::sketch_chains::{ChainOptions, LoopRole, chain};
use common::*;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

#[test]
fn unordered_lines_chain_into_one_counterclockwise_loop() {
    // A 10 x 5 rectangle, its sides out of order and drawn in mixed directions.
    let input = vec![
        line("cut", 10.0, 5.0, 10.0, 0.0),
        line("cut", 0.0, 5.0, 0.0, 0.0),
        line("cut", 0.0, 0.0, 10.0, 0.0),
        line("cut", 0.0, 5.0, 10.0, 5.0),
    ];
    let chains = chain(&input, ChainOptions::default()).unwrap();
    assert!(chains.open.is_empty() && chains.rejected.is_empty());
    let [rect] = chains.closed.as_slice() else {
        panic!("one loop, got {}", chains.closed.len())
    };
    near(rect.area, 50.0);
    assert_eq!(rect.layer, "cut");
    assert_eq!(rect.max_gap, 0.0);
    // Counterclockwise from the lowest corner: bottom, right, top, left.
    let order: Vec<usize> = rect.segments.iter().map(|s| s.primitive).collect();
    assert_eq!(order, [2, 0, 3, 1]);
    let reversed: Vec<bool> = rect.segments.iter().map(|s| s.reversed).collect();
    assert_eq!(reversed, [false, true, true, false]);
    assert_eq!(rect.segments[0].start, Point2::new(0.0, 0.0));
    assert_eq!(
        rect.segments[2].curve,
        Primitive::Line {
            start: p(10.0, 5.0),
            end: p(0.0, 5.0)
        }
    );
    for (i, s) in rect.segments.iter().enumerate() {
        assert_eq!(s.end, rect.segments[(i + 1) % 4].start);
    }
    assert_eq!(rect.nesting.clone().unwrap().role, LoopRole::Outer);
}

#[test]
fn chain_id_ignores_draw_order_and_direction_but_not_layer_or_geometry() {
    let base = slot("cut", 0.0);
    let id = chain(&base, ChainOptions::default()).unwrap().closed[0].id;
    // Every rotation of the input, and every primitive drawn the other way.
    for turn in 0..base.len() {
        let mut input = base.clone();
        input.rotate_left(turn);
        assert_eq!(
            chain(&input, ChainOptions::default()).unwrap().closed[0].id,
            id
        );
        let flipped: Vec<_> = input.iter().map(flip).collect();
        let chains = chain(&flipped, ChainOptions::default()).unwrap();
        assert_eq!(chains.closed[0].id, id);
        near(chains.closed[0].area, 20.0 * 10.0 + PI * 25.0);
    }
    let other_layer = chain(&slot("mark", 0.0), ChainOptions::default())
        .unwrap()
        .closed[0]
        .id;
    let moved = chain(&slot("cut", 0.001), ChainOptions::default())
        .unwrap()
        .closed[0]
        .id;
    assert_ne!(other_layer, id);
    assert_ne!(moved, id);
    assert_eq!(id.to_string().len(), 32);
    // The value is part of the contract: a project file may store it.
    assert_eq!(id.to_string(), SLOT_ID);
}

#[test]
fn arc_metadata_survives_chaining() {
    let chains = chain(&slot("cut", 0.0), ChainOptions::default()).unwrap();
    let [stadium] = chains.closed.as_slice() else {
        panic!("one loop")
    };
    assert_eq!(stadium.segments.len(), 4);
    let arcs: Vec<_> = stadium
        .segments
        .iter()
        .filter_map(|s| match s.curve {
            Primitive::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => Some((
                s.primitive,
                s.reversed,
                center,
                radius,
                start_angle,
                sweep_angle,
            )),
            _ => None,
        })
        .collect();
    // The right cap was drawn clockwise; in the counterclockwise loop both caps sweep +pi (G3).
    assert_eq!(arcs.len(), 2);
    let (index, reversed, center, radius, start, sweep) = arcs[0];
    assert_eq!(
        (index, reversed, center, radius),
        (1, true, p(20.0, 5.0), 5.0)
    );
    near(start, -FRAC_PI_2);
    near(sweep, PI);
    let (index, reversed, center, radius, start, sweep) = arcs[1];
    assert_eq!(
        (index, reversed, center, radius),
        (3, false, p(0.0, 5.0), 5.0)
    );
    near(start, FRAC_PI_2);
    near(sweep, PI);
}

#[test]
fn gap_wider_than_tolerance_stays_open_and_is_reported() {
    // The last side stops 0.02 mm short of the first corner.
    let input = vec![
        line("cut", 0.0, 0.0, 10.0, 0.0),
        line("cut", 10.0, 0.0, 10.0, 5.0),
        line("cut", 10.0, 5.0, 0.0, 5.0),
        line("cut", 0.0, 5.0, 0.0, 0.02),
    ];
    let chains = chain(&input, ChainOptions::default()).unwrap();
    assert!(chains.closed.is_empty() && chains.rejected.is_empty());
    let [open] = chains.open.as_slice() else {
        panic!("one open chain")
    };
    assert_eq!(open.segments.len(), 4);
    assert_eq!((open.start, open.end), (p(0.0, 0.0), p(0.0, 0.02)));
    assert_eq!(open.segments[0].start, open.start);
    assert_eq!(open.segments[3].end, open.end);

    // A tolerance that covers the gap closes it, and says how wide the bridged gap is.
    let chains = chain(
        &input,
        ChainOptions {
            join_tolerance: 0.05,
        },
    )
    .unwrap();
    assert!(chains.open.is_empty());
    near(chains.closed[0].max_gap, 0.02);
    near(chains.closed[0].area, 50.0);
    // Exactly at the tolerance joins; just under it does not.
    assert_eq!(
        chain(
            &input,
            ChainOptions {
                join_tolerance: 0.02
            }
        )
        .unwrap()
        .closed
        .len(),
        1
    );
    assert_eq!(
        chain(
            &input,
            ChainOptions {
                join_tolerance: 0.0199
            }
        )
        .unwrap()
        .open
        .len(),
        1
    );
}

#[test]
fn open_chain_of_lines_and_arcs_starts_at_its_lower_end() {
    // (20,0) <- arc <- (10,0) <- line <- (0,0), drawn right to left and out of order.
    let input = vec![
        arc("cut", 15.0, 0.0, 5.0, 0.0, PI),
        line("cut", 10.0, 0.0, 0.0, 0.0),
        line("other", 20.0, 0.0, 30.0, 0.0),
    ];
    let chains = chain(&input, ChainOptions::default()).unwrap();
    assert_eq!(chains.open.len(), 2);
    let cut = &chains.open[0];
    assert_eq!((cut.layer.as_str(), cut.start), ("cut", p(0.0, 0.0)));
    near(cut.end.x, 20.0);
    assert_eq!(
        cut.segments
            .iter()
            .map(|s| (s.primitive, s.reversed))
            .collect::<Vec<_>>(),
        [(1, true), (0, true)]
    );
    let Primitive::Arc { sweep_angle, .. } = cut.segments[1].curve else {
        panic!("arc")
    };
    near(sweep_angle, -PI);
    // Layers never join, even where their ends coincide.
    assert_eq!(
        (chains.open[1].layer.as_str(), chains.open[1].segments.len()),
        ("other", 1)
    );
    assert_ne!(chains.open[0].id, chains.open[1].id);
}

#[test]
fn arcs_that_close_a_circle_and_tangent_fillets_are_clean_loops() {
    // Two half arcs make one circle; an arc of almost a full turn closes on itself.
    let halves = vec![
        arc("c", 0.0, 0.0, 5.0, 0.0, PI),
        arc("c", 0.0, 0.0, 5.0, PI, PI),
    ];
    let chains = chain(&halves, ChainOptions::default()).unwrap();
    assert!(chains.rejected.is_empty(), "{:?}", chains.rejected);
    near(chains.closed[0].area, PI * 25.0);
    let nearly = vec![arc("c", 0.0, 0.0, 5.0, 0.3, -(TAU - 1e-3))];
    let chains = chain(&nearly, ChainOptions::default()).unwrap();
    assert!(chains.rejected.is_empty(), "{:?}", chains.rejected);
    assert!((chains.closed[0].area - PI * 25.0).abs() < 1e-2);
    let Primitive::Arc { sweep_angle, .. } = chains.closed[0].segments[0].curve else {
        panic!("arc")
    };
    assert!(sweep_angle > 0.0, "the loop is walked counterclockwise");

    // A rounded rectangle whose coordinates were rounded to 1e-6, as a DXF writer would.
    let chains = chain(
        &rounded_rect("cut", 40.0, 20.0, 3.0),
        ChainOptions::default(),
    )
    .unwrap();
    assert!(
        chains.rejected.is_empty() && chains.open.is_empty(),
        "{:?}",
        chains.rejected
    );
    let [loop_] = chains.closed.as_slice() else {
        panic!("one loop")
    };
    assert_eq!(loop_.segments.len(), 8);
    assert!((loop_.area - (40.0 * 20.0 - (4.0 - PI) * 9.0)).abs() < 1e-4);
    assert!(loop_.max_gap < 2e-6);
}

#[test]
fn dxf_polyline_bulges_and_circles_chain_with_their_arcs() {
    // A closed LWPOLYLINE slot (two bulge-1 caps) with a CIRCLE inside, as the DXF reader gives it.
    let text = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
0\nCIRCLE\n8\ncut\n10\n10\n20\n5\n40\n2\n\
0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n20\n20\n0\n42\n1\n10\n20\n20\n10\n10\n0\n20\n10\n42\n1\n\
0\nENDSEC\n0\nEOF\n";
    let document = camcraft_geometry::dxf::parse(text.as_bytes()).unwrap();
    assert_eq!(document.units, Units::Known(Unit::Millimeters));
    let chains = chain(&document.primitives, ChainOptions::default()).unwrap();
    assert!(
        chains.rejected.is_empty() && chains.open.is_empty(),
        "{:?}",
        chains.rejected
    );
    let stadium = chains
        .closed
        .iter()
        .find(|c| c.segments.len() == 4)
        .unwrap();
    near(stadium.area, 200.0 + PI * 25.0);
    let sweeps: Vec<f64> = stadium
        .segments
        .iter()
        .filter_map(|s| match s.curve {
            Primitive::Arc {
                radius,
                sweep_angle,
                ..
            } => Some(sweep_angle / radius),
            _ => None,
        })
        .collect();
    assert_eq!(sweeps.len(), 2);
    sweeps.iter().for_each(|s| near(*s, PI / 5.0));
    // The same slot as loose primitives has the same identity as the polyline's.
    assert_eq!(stadium.id.to_string(), SLOT_ID);
    let bore = chains
        .closed
        .iter()
        .find(|c| c.segments.len() == 1)
        .unwrap();
    assert_eq!(role(bore), (LoopRole::Hole, 1));
    assert_eq!(bore.nesting.clone().unwrap().parent, Some(stadium.id));
}
